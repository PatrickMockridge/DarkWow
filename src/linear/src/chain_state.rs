/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Single authoritative chain state (Bitcoin Core CChainState pattern).
//!
//! One instance per chain. One height. One consensus. One VM pool.
//! Replaces the dual-`LinearBlockchain` pattern that caused diverged caches.
//!
//! ## Lock ordering (MUST be followed to prevent deadlocks)
//!
//! When multiple locks must be held simultaneously, acquire in this order:
//!   1. `connect_lock`        — outermost, serializes all block application
//!   2. `vm_cache`            — RandomX VM pool
//!   3. `competing_seen`      — dedup set for competing blocks
//!   4. `competing_blocks`    — competing block storage
//!   5. `consensus`           — target/timestamp state
//!
//! `take_competing_blocks` and `put_competing_blocks` acquire only
//! `competing_blocks` + `competing_seen` (never `connect_lock`),
//! which cannot deadlock with `connect_block` because the latter
//! holds `connect_lock` before those inner locks.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex};

use blake3::Hash as Blake3Hash;
use randomx::{RandomXCache, RandomXFlags, RandomXVM};
use sled::transaction::Transactional;
use tracing::info;
use dwow_sdk::pasta::group::ff::PrimeField;

use crate::{
    Block, CumulativeSupplyChain, FinalityConfig, LinearError, LinearStore, Nullifier,
    PoWConsensus, Result, UncleBlock, validation, COINBASE_MATURITY,
};

/// How long the tip can be stale before the node considers itself
/// in Initial Block Download (24 hours for a 60s block time chain).
/// Single authoritative chain state.
///
/// Replaces both `dwow_chain::LinearBlockchain` (base lib) and
/// `crate::blockchain::LinearBlockchain` (dwowd wrapper). One instance.
/// All block insertion goes through `connect_block()`.
pub struct CChainState {
    /// Storage backend (sled)
    pub store: Arc<LinearStore>,
    /// Cumulative supply chain — single authoritative source for
    /// Pedersen commitment chain S_H = S_{H-1} + C_H and TOTAL_SUPPLY.
    pub supply_chain: CumulativeSupplyChain,
    /// PoW consensus / difficulty adjustment
    pub consensus: Mutex<PoWConsensus>,
    /// Finality configuration
    pub finality_config: FinalityConfig,

    // --- Cached state (always derived from store, never authoritative) ---
    /// Current chain height
    height: AtomicU64,
    /// RandomX VM pool keyed by randomx_key.
    /// Each cached VM is wrapped in a Mutex — calculate_hash mutates the
    /// C scratchpad internally, so concurrent access from multiple smol tasks
    /// on the same VM causes a segfault. The per-VM Mutex serializes access.
    vm_cache: Mutex<HashMap<[u8; 32], Arc<std::sync::Mutex<RandomXVM>>>>,
    /// RandomXCache pool keyed by randomx_key.
    /// RandomXCache (256 MB) is the heavy allocation — internally Arc-wrapped
    /// so cloning is O(1). External callers that need exclusive VMs
    /// (miner, broadcast, stratum) clone the cached cache and create a fresh
    /// VM around it (2 MB scratchpad). This eliminates the 256 MB allocation
    /// churn that causes SIGSEGV under Docker memory pressure.
    cache_pool: Mutex<HashMap<[u8; 32], RandomXCache>>,
    /// Coin commitments → block height (for maturity tracking).
    /// Typed CoinCommitment per Phase X — CoinCommitment has Hash for HashMap.
    coin_set: Mutex<HashMap<CoinCommitment, u64>>,
    /// Uncle coin hashes (blake3) → block height. Uncle coins are NOT Poseidon
    /// CoinCommitments — they are blake3 hashes of uncle block headers. Stored
    /// separately to prevent type confusion with Poseidon coin commitments.
    uncle_coin_set: Mutex<HashMap<[u8; 32], u64>>,
    /// Spent nullifiers → block height (double-spend prevention, height-tracked for pruning).
    /// Typed BTreeSet<Nullifier> per Phase 1 — Nullifier has Ord for SMT key ordering.
    nullifier_set: Mutex<BTreeMap<Nullifier, u64>>,
    /// Competing blocks at the same height — potential uncles for fork resolution.
    /// When two miners produce blocks at height N simultaneously, the first
    /// received becomes canonical and the second is stored here. The next block
    /// mined (N+1) can include these as uncles with partial rewards.
    /// Key: height, Value: competing block at that height.
    competing_blocks: Mutex<HashMap<u64, Vec<Block>>>,
    /// Dedup set for competing blocks — prevents storing the same block twice (H7).
    competing_seen: Mutex<HashSet<Blake3Hash>>,
    /// Serializes connect_block calls — prevents concurrent block application
    /// from racing on height, VM cache, and sled writes (RandomX FFI segfaults).
    connect_lock: Mutex<()>,
}

impl CChainState {
    /// Create a new chain state backed by the given sled database.
    pub fn new(
        db: Arc<sled::Db>,
        target_block_time: u64,
        initial_target: u32,
        min_target: u32,
        max_target: u32,
        finality_config: FinalityConfig,
    ) -> Result<Arc<Self>> {
        let store = Arc::new(LinearStore::new(db.clone())?);
        let consensus = PoWConsensus::new(target_block_time, initial_target, min_target, max_target);
        let _ = consensus.load(store.consensus_tree());
        // Restore accumulated chain work from sled (survives restarts)
        if let Some(work_bytes) = store.consensus.get("accumulated_work").ok().flatten() {
            if work_bytes.len() == 8 {
                let work = u64::from_le_bytes(work_bytes[..8].try_into().unwrap_or([0u8; 8]));
                consensus.accumulated_work.store(work, Ordering::SeqCst);
            }
        }
        let height = store.get_height().unwrap_or(0);

        // Create initial VM with zero key (wrapped in Mutex for thread safety)
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let cache = randomx::RandomXCache::new(flags, &[0u8; 32])
            .map_err(|e| LinearError::RandomXError(format!("VM cache: {}", e)))?;
        let vm = Arc::new(std::sync::Mutex::new(
            RandomXVM::new(flags, Some(cache), None)
                .map_err(|e| LinearError::RandomXError(format!("VM: {}", e)))?,
        ));
        let mut vm_cache = HashMap::new();
        vm_cache.insert([0u8; 32], vm);

        // Restore coin_set and nullifier_set from sled trees
        // (survive restarts — no more in-memory-only state loss)
        let coin_set = {
            let mut map = HashMap::new();
            for item in store.coins.iter() {
                if let Ok((k, v)) = item {
                    if k.len() == 32 && v.len() == 8 {
                        let mut coin_bytes = [0u8; 32];
                        let mut height_bytes = [0u8; 8];
                        coin_bytes.copy_from_slice(&k);
                        height_bytes.copy_from_slice(&v);
                        if let Ok(coin) = CoinCommitment::from_bytes(coin_bytes) {
                            map.insert(coin, u64::from_le_bytes(height_bytes));
                        }
                    }
                }
            }
            Mutex::new(map)
        };
        let uncle_coin_set: Mutex<HashMap<[u8; 32], u64>> =
            Mutex::new(HashMap::new());
        let nullifier_set = {
            let mut map = BTreeMap::new();
            for item in store.nullifiers.iter() {
                if let Ok((k, _)) = item {
                    if k.len() == 32 {
                        let mut nf_bytes = [0u8; 32];
                        nf_bytes.copy_from_slice(&k);
                        if let Ok(nf) = Nullifier::from_bytes(nf_bytes) {
                            // Pre-existing nullifiers on restart: tag with height 0.
                            // These are already committed in the chain SMT and cannot
                            // be removed. Height 0 means "do not prune" — pruning
                            // skips entries with h < prune_h, and since prune_h is
                            // always > 0 after genesis, these survive.
                            map.insert(nf, 0u64);
                        }
                    }
                }
            }
            Mutex::new(map)
        };

        // Initialize cumulative supply chain — restores latest from sled.
        let supply_chain = CumulativeSupplyChain::new(&db)?;

        Ok(Arc::new(Self {
            store,
            supply_chain,
            consensus: Mutex::new(consensus),
            finality_config,
            height: AtomicU64::new(height),
            vm_cache: Mutex::new(vm_cache),
            cache_pool: Mutex::new(HashMap::new()),
            coin_set,
            uncle_coin_set,
            nullifier_set,
            competing_blocks: Mutex::new(HashMap::new()),
            competing_seen: Mutex::new(HashSet::new()),
            connect_lock: Mutex::new(()),
        }))
    }

    // --- Height ---

    /// Current chain height (O(1) atomic read).
    pub fn get_height(&self) -> u64 {
        self.height.load(Ordering::SeqCst)
    }

    fn set_height(&self, h: u64) {
        self.height.store(h, Ordering::SeqCst);
    }

    // --- Block access ---

    pub fn get_block(&self, height: u64) -> Result<Block> {
        self.store.get_block(height).map_err(|e| LinearError::StorageError(e.to_string()))
    }

    pub fn get_latest_block(&self) -> Result<Block> {
        let h = self.get_height();
        if h == 0 {
            return Err(LinearError::BlockNotFound(0));
        }
        self.get_block(h)
    }

    // --- RandomX VM ---

    /// Hash a block using the cached VM for its key.
    /// Encapsulates lock+hash+unlock — no MutexGuard escapes this function.
    /// Safe for async contexts because no !Send type is held across yield points.
    pub fn hash_block_with_cached_vm(&self, block: &Block) -> blake3::Hash {
        let vm = self.get_vm(block.header.randomx_key);
        let guard = vm.lock().unwrap();
        block.hash_with_vm(&*guard)
    }

    /// Get or create a RandomX VM for the given key.
    /// Returns Arc<Mutex<RandomXVM>> — caller MUST lock before hashing.
    /// Prefer `hash_block_with_cached_vm` for async contexts to avoid Send issues.
    /// Maximum number of RandomX VMs to cache. Each VM holds ~258 MB of
    /// memory (256 MB cache + 2 MB scratchpad). Old blocks are never re-hashed
    /// with their original key, so only the most recent heights are accessed.
    /// Set to 6 to accommodate 2 mining nodes × 3 recent keys each.
    const MAX_CACHED_VMS: usize = 6;

    /// Maximum number of RandomXCache entries to pool for external callers.
    /// Each cache is 256 MB. Caches are internally Arc-wrapped — cloning for
    /// external VM creation is O(1). Same eviction policy as vm_cache.
    const MAX_CACHED_CACHES: usize = 6;

    pub fn get_vm(&self, key: [u8; 32]) -> Arc<std::sync::Mutex<RandomXVM>> {
        let mut cache = self.vm_cache.lock().unwrap();
        if let Some(vm) = cache.get(&key) {
            return vm.clone();
        }
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(flags, &key)
            .expect("Failed to create RandomX cache");
        let vm = Arc::new(std::sync::Mutex::new(
            RandomXVM::new(flags, Some(rx_cache), None)
                .expect("Failed to create RandomX VM"),
        ));
        cache.insert(key, vm.clone());
        // Evict oldest entry when cache exceeds capacity.
        // Old blocks are never re-hashed — only recent heights need cached VMs.
        if cache.len() > Self::MAX_CACHED_VMS {
            if let Some(oldest) = cache.keys().min().cloned() {
                cache.remove(&oldest);
            }
        }
        vm
    }

    /// Get or create a RandomXCache for the given key.
    ///
    /// RandomXCache is the heavy allocation (256 MB) and is internally
    /// Arc-wrapped — cloning it is O(1). External callers (miner, broadcast,
    /// stratum, mm_rpc) clone the cached cache and create a fresh VM around
    /// it, eliminating the 256 MB allocation churn that causes SIGSEGV under
    /// Docker memory pressure.
    ///
    /// The fresh VM still allocates 2 MB of scratchpad memory — negligible
    /// compared to the 256 MB cache. This pool ensures the 256 MB allocation
    /// happens ONCE per key, not once per operation.
    pub fn get_cache(&self, key: [u8; 32]) -> RandomXCache {
        let mut pool = self.cache_pool.lock().unwrap();
        if let Some(cache) = pool.get(&key) {
            return cache.clone();
        }
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let cache = randomx::RandomXCache::new(flags, &key)
            .expect("Failed to create RandomX cache");
        pool.insert(key, cache.clone());
        // Evict oldest entry when pool exceeds capacity
        if pool.len() > Self::MAX_CACHED_CACHES {
            if let Some(oldest) = pool.keys().min().cloned() {
                pool.remove(&oldest);
            }
        }
        cache
    }

    // --- Coin / nullifier sets ---

    pub fn has_coin(&self, coin: &CoinCommitment) -> bool {
        self.coin_set.lock().unwrap().contains_key(coin)
    }

    pub fn is_coin_mature(&self, coin: &CoinCommitment, current_height: u64) -> bool {
        match self.coin_set.lock().unwrap().get(coin) {
            Some(&created_at) => current_height.saturating_sub(created_at) >= COINBASE_MATURITY,
            None => false,
        }
    }

    pub fn has_nullifier(&self, nullifier: &Nullifier) -> bool {
        self.nullifier_set.lock().unwrap().contains_key(nullifier)
    }

    /// Return the block height at which this nullifier was created, if present.
    pub fn nullifier_height(&self, nullifier: &Nullifier) -> Option<u64> {
        self.nullifier_set.lock().unwrap().get(nullifier).copied()
    }

    /// Take competing blocks at the current height for uncle inclusion.
    /// Called by the miner task before building a new block. The competing
    /// blocks (mined by other nodes at the same height as the canonical tip)
    /// are removed from storage and returned. The caller includes them in
    /// the next block's `uncle_merkle_root` and passes them to
    /// `apply_block_with_uncles`.
    pub fn take_competing_blocks(&self, height: u64) -> Vec<Block> {
        let blocks = self.competing_blocks.lock().unwrap().remove(&height).unwrap_or_default();
        // Clean dedup set for consumed blocks (H7 follow-up)
        if !blocks.is_empty() {
            let mut seen = self.competing_seen.lock().unwrap();
            for b in &blocks {
                // Recompute hash — we need a VM. Use quick blake3 of header bytes.
                let h = blake3::hash(&serde_json::to_vec(&b.header).unwrap_or_else(|e| { tracing::error!(target: "dwow_chain::chain_state", "BlockHeader serialization failed: {}", e); vec![0u8; 32] }));
                seen.remove(&h);
            }
        }
        blocks
    }

    /// Put competing blocks back at a given height (H3.4 fix).
    /// Called by the miner task if block acceptance fails — the competing
    /// blocks were destructively removed by `take_competing_blocks()` and
    /// must be restored so the competing miner doesn't lose their uncle
    /// reward opportunity.
    pub fn put_competing_blocks(&self, height: u64, blocks: Vec<Block>) {
        if blocks.is_empty() {
            return;
        }
        let mut seen = self.competing_seen.lock().unwrap();
        let mut competing = self.competing_blocks.lock().unwrap();
        for b in &blocks {
            let h = blake3::hash(&serde_json::to_vec(&b.header).unwrap_or_else(|e| { tracing::error!(target: "dwow_chain::chain_state", "BlockHeader serialization failed: {}", e); vec![0u8; 32] }));
            seen.insert(h);
        }
        competing.entry(height).or_default().extend(blocks);
    }

    /// Clean up competing block entries older than MAX_UNCLE_DEPTH (H11).
    /// Called after a canonical block is committed to prevent unbounded growth.
    fn prune_competing(&self, current_height: u64) {
        let max_depth = 6u64; // MAX_UNCLE_DEPTH
        if current_height <= max_depth {
            return;
        }
        let cutoff = current_height - max_depth;
        let mut competing = self.competing_blocks.lock().unwrap();
        let mut seen = self.competing_seen.lock().unwrap();
        competing.retain(|&height, blocks| {
            if height < cutoff {
                for b in blocks {
                    let h = blake3::hash(&serde_json::to_vec(&b.header).unwrap_or_else(|e| { tracing::error!(target: "dwow_chain::chain_state", "BlockHeader serialization failed: {}", e); vec![0u8; 32] }));
                    seen.remove(&h);
                }
                false
            } else {
                true
            }
        });
    }

    // --- Block insertion: single atomic path ---

    /// Apply a fully-validated block to the chain state.
    ///
    /// This is the ONLY path for block insertion — used by genesis, sync,
    /// broadcast, miner RPC, stratum, and merge mining. It replaces both
    /// `insert_validated_block` (non-atomic) and `apply_canonical_block`
    /// (dual-instance).
    ///
    /// The caller is responsible for WASM execution (the library crate
    /// does not depend on the WASM runtime). The contracts overlay batch
    /// from WASM execution is passed in as `contracts_batch`.
    pub fn connect_block(
        &self,
        block: &Block,
        uncles: &[UncleBlock],
        contracts_batch: Option<sled::Batch>,
        supply_chain_batch: Option<sled::Batch>,
    ) -> Result<()> {
        // Serialize all block application — prevents concurrent connect_block
        // calls from racing on height, VM cache, sled writes, and RandomX FFI.
        let _lock = self.connect_lock.lock().unwrap();
        let vm = self.get_vm(block.header.randomx_key);
        let current_height = self.get_height();
        let block_height = block.header.height;

        // H5.4: Early height-gap rejection — reject far-future blocks before
        // acquiring any more locks or doing expensive validation. Blocks more
        // than 1 ahead of our tip cannot connect and will fail HeightDiscontinuity.
        // Competing blocks (same height) and next-block (height+1) proceed.
        if block_height > current_height + 1 {
            return Err(LinearError::HeightDiscontinuity {
                expected: current_height + 1,
                got: block_height,
            });
        }

        // --- Competing block at current height → store as potential uncle ---
        // Polkadot BABE/GRANDPA parachain inclusion pattern: when two miners
        // produce blocks at the same height, the first received is canonical.
        // The competing block is stored as a potential uncle — it will be
        // included in the next block's uncle_merkle_root for a partial reward.
        //
        // Stage 1 PoW is validated (hash must meet the block's own declared
        // target). Stage 2 target validation is SKIPPED — the competing block
        // was mined on a different fork with different timestamp history, so
        // our canonical chain's get_next_work_required would return the wrong
        // expected target. Full validation happens if/when we reorganize to
        // that fork (longest-chain-wins).
        if block_height == current_height {
            // Lock the cached VM for hashing — prevents concurrent RandomX FFI
            // with other tasks (miner_task, GetTip, RPC) accessing the same key.
            let guard = vm.lock().unwrap();
            let hash_u32 = {
                let h = block.hash_with_vm(&*guard);
                u32::from_le_bytes(h.as_bytes()[0..4].try_into().unwrap())
            };
            if hash_u32 > block.header.target {
                return Err(LinearError::InvalidPoW(
                    block.hash_with_vm(&*guard).to_string()
                ));
            }
            // H1 fix: validate target range for competing blocks.
            // Stage 2 validation (target == expected_target) cannot be
            // performed without the fork's timestamp history, but we
            // can enforce bounds.
            {
                let consensus = self.consensus.lock().unwrap();
                let min = consensus.min_target();
                let max = consensus.max_target();
                if block.header.target < min || block.header.target > max {
                    drop(guard);
                    return Err(LinearError::BlockIsInvalid(
                        format!("Competing block target {} outside bounds [{}, {}]",
                            block.header.target, min, max)
                    ));
                }
                drop(consensus);
            }
            // H4 fix: validate that competing block's previous hash
            // matches the canonical parent at current_height - 1.
            // Without this check, unrelated blocks can pollute the
            // competing store.
            if current_height > 0 {
                let parent = self.get_block(current_height)?;
                let parent_vm = self.get_vm(parent.header.randomx_key);
                let parent_guard = parent_vm.lock().unwrap();
                let parent_hash = parent.hash_with_vm(&*parent_guard);
                drop(parent_guard);
                if block.header.previous != parent_hash {
                    drop(guard);
                    return Err(LinearError::InvalidPreviousHash(
                        format!("Competing block previous {} != canonical parent {}",
                            hex::encode(block.header.previous.as_bytes()),
                            hex::encode(parent_hash.as_bytes()))
                    ));
                }
            }
            // H6 fix: build recent timestamps for competing-block validation.
            let recent_ts: Vec<u64> = {
                let start = if block_height > 11 { block_height - 11 } else { 1 };
                let mut ts = Vec::new();
                for h in start..block_height {
                    if let Ok(b) = self.get_block(h) {
                        ts.push(b.header.timestamp);
                    }
                }
                ts
            };
            // H6 fix: apply timestamp validation to competing blocks.
            if let Err(e) = validation::check_block_timestamp(
                block.header.timestamp,
                block_height,
                &recent_ts,
            ) {
                drop(guard);
                return Err(e);
            }
            // H5 fix: cap competing blocks per height at MAX_COMPETING_BLOCKS
            const MAX_COMPETING_BLOCKS: usize = 20;
            // H7: Dedup by hash — reject duplicate competing blocks
            let block_hash = block.hash_with_vm(&*guard);
            drop(guard); // Release VM lock before acquiring other locks
            {
                let mut seen = self.competing_seen.lock().unwrap();
                if seen.contains(&block_hash) {
                    return Ok(());  // Already stored, silently ignore
                }
                seen.insert(block_hash);
            }
            let mut competing = self.competing_blocks.lock().unwrap();
            let entry = competing.entry(block_height).or_default();
            if entry.len() >= MAX_COMPETING_BLOCKS {
                return Ok(());  // Silent drop — competing store is full
            }
            entry.push(block.clone());
            drop(competing);
            info!(target: "chain_state",
                "Competing block at h={} stored as potential uncle", block_height);
            return Ok(());
        }

        // --- Uncle parent lookup ---
        // Before full validation, check whether this block builds on our
        // canonical tip or on a competing block (uncle). If it builds on an
        // uncle, this is an uncle chain extension — store as competing block
        // at the next height. The uncle chain may grow longer than the
        // canonical chain, triggering reorganization.
        let tip_hash = if current_height > 0 {
            let prev = self.get_block(current_height)?;
            let prev_vm = self.get_vm(prev.header.randomx_key);
            let prev_guard = prev_vm.lock().unwrap();
            Some(prev.hash_with_vm(&*prev_guard))
        } else {
            None
        };

        // Check if block builds on canonical tip or an uncle
        let builds_on_tip = tip_hash
            .as_ref()
            .map(|th| block.header.previous == *th)
            .unwrap_or(true); // Genesis always builds on tip

        if !builds_on_tip {
            // Block doesn't build on our canonical tip. Check if it builds
            // on a competing block (uncle) at current_height.
            let mut competing = self.competing_blocks.lock().unwrap();
            let uncle_parent = competing
                .get(&current_height)
                .and_then(|blocks| {
                    blocks.iter().find(|b| {
                        let pvm = self.get_vm(b.header.randomx_key);
                        let pguard = pvm.lock().unwrap();
                        b.hash_with_vm(&*pguard) == block.header.previous
                    })
                });
            if uncle_parent.is_some() {
                // Uncle chain extension: store as competing at next height.
                // Stage 1 PoW validated first (same as competing path).
                let guard = vm.lock().unwrap();
                let hash_u32 = {
                    let h = block.hash_with_vm(&*guard);
                    u32::from_le_bytes(h.as_bytes()[0..4].try_into().unwrap())
                };
                if hash_u32 > block.header.target {
                    return Err(LinearError::InvalidPoW(
                        block.hash_with_vm(&*guard).to_string()
                    ));
                }
                // H2 fix: validate target range for uncle chain extensions.
                {
                    let consensus = self.consensus.lock().unwrap();
                    let min = consensus.min_target();
                    let max = consensus.max_target();
                    if block.header.target < min || block.header.target > max {
                        drop(guard);
                        return Err(LinearError::BlockIsInvalid(
                            format!("Uncle chain target {} outside bounds [{}, {}]",
                                block.header.target, min, max)
                        ));
                    }
                    drop(consensus);
                }
                // H6 fix: build recent timestamps for uncle chain extension validation.
                let recent_ts: Vec<u64> = {
                    let start = if block_height > 11 { block_height - 11 } else { 1 };
                    let mut ts = Vec::new();
                    for h in start..block_height {
                        if let Ok(b) = self.get_block(h) {
                            ts.push(b.header.timestamp);
                        }
                    }
                    ts
                };
                // H6 fix: apply timestamp validation to uncle chain extensions.
                if let Err(e) = validation::check_block_timestamp(
                    block.header.timestamp,
                    block_height,
                    &recent_ts,
                ) {
                    drop(guard);
                    return Err(e);
                }
                drop(guard);

                // H5 fix: cap competing blocks per height
                const MAX_COMPETING_BLOCKS_UNCLE: usize = 20;
                let block_hash = block.hash_with_vm(
                    &*vm.lock().unwrap()
                );
                let mut seen = self.competing_seen.lock().unwrap();
                if !seen.contains(&block_hash) {
                    seen.insert(block_hash);
                    drop(seen);
                    let entry = competing.entry(block_height).or_default();
                    if entry.len() < MAX_COMPETING_BLOCKS_UNCLE {
                        entry.push(block.clone());
                    }
                }
                drop(competing);
                info!(target: "chain_state",
                    "Uncle chain extension at h={} stored as competing", block_height);
                return Ok(());
            }
            drop(competing);
            // Block doesn't build on tip or known uncle — falls through
            // to full validation below. check_block_header will reject
            // with InvalidPreviousHash.
        }

        // --- Stage 1 & 2 PoW validation ---
        let expected_target = {
            self.consensus.lock().unwrap()
                .get_next_work_required(&self.store, block_height)
        };

        // Lock the VM for validation hashing — prevents concurrent RandomX FFI
        let guard = vm.lock().unwrap();
        validation::check_block_header(
            block, &*guard, expected_target, current_height, tip_hash.as_ref(),
        )?;
        drop(guard);

        // CRITICAL-4: Timestamp validation (time warp protection + future limit)
        {
            let mut recent_ts: Vec<u64> = Vec::with_capacity(11);
            let start = if block_height > 11 { block_height - 11 } else { 1 };
            for h in start..block_height {
                if let Ok(b) = self.store.get_block(h) {
                    recent_ts.push(b.header.timestamp);
                }
            }
            validation::check_block_timestamp(
                block.header.timestamp, block_height, &recent_ts,
            )?;
        }

        // --- Finality: anchored block conflict ---
        if let Ok(existing) = self.store.get_block(block_height) {
            if self.finality_config.should_enforce(existing.header.finality_flags)
                && (existing.header.anchor_tx_id != [0u8; 32]
                    || existing.header.anchor_monero_height != 0)
            {
                return Err(LinearError::AnchoredBlockConflict);
            }
        }

        // --- Update consensus BEFORE building batch (H4 fix) ---
        // Previously: save_to_batch (old state) → sled tx → record_block + adjust_target + save (new state).
        // Crash between sled tx and save() = block committed, consensus stale → desync.
        // Now: record_block + adjust_target → save_to_batch (new state) → sled tx (atomic).
        // In-memory consensus is updated BEFORE the sled tx.
        //
        // H5.2 fix: roll back consensus on ANY error between the update and
        // the sled commit, not just TransactionError. Previously, a serde
        // failure in block serialization would leave consensus updated but
        // block uncommitted — permanent desync.
        let pre_timestamps: Vec<u64>;
        let pre_target: u32;
        {
            let consensus = self.consensus.lock().unwrap();
            pre_target = consensus.target();
            pre_timestamps = consensus.snapshot_timestamps();
            consensus.record_block(block.header.timestamp);
            consensus.adjust_target();
        }

        // Wrap batch-build + commit in a closure. Any error rolls back
        // in-memory consensus — covers serde failures (which the old
        // TransactionError-only rollback missed) and sled failures.
        let height = block_height;
        let commit_result = (|| -> Result<()> {
            // --- Build commit batch ---
            let mut blocks_batch = sled::Batch::default();
            let block_value = serde_json::to_vec(block)
                .map_err(|e| LinearError::SerializationError(e.to_string()))?;
            blocks_batch.insert(&height.to_le_bytes(), block_value);

            let mut uncles_batch = sled::Batch::default();
            for uncle in uncles {
                let uncle_hash = blake3::hash(&serde_json::to_vec(&uncle.header)
                    .unwrap_or_else(|e| { tracing::error!(target: "dwow_chain::chain_state", "Uncle header serialization failed: {}", e); vec![0u8; 32] }));
                let uncle_value = serde_json::to_vec(uncle)
                    .map_err(|e| LinearError::SerializationError(e.to_string()))?;
                uncles_batch.insert(uncle_hash.as_bytes(), uncle_value);
            }

            // Validate uncle_merkle_root consistency — matches Python spec.
            // H2.2: If no uncles, merkle root must be zero. A non-zero
            // root with empty uncles is a commitment to nothing.
            if uncles.is_empty() {
                if block.header.uncle_merkle_root != [0u8; 32] {
                    return Err(LinearError::UncleMerkleRootMismatch(
                        "no uncles but uncle_merkle_root is non-zero".into()
                    ));
                }
            } else {
                let has_root = block.header.uncle_merkle_root != [0u8; 32];
                if !has_root {
                    return Err(LinearError::UncleMerkleRootMismatch(
                        "uncles present but uncle_merkle_root is zero".into()
                    ));
                }
            }

            // Coin and nullifier batches
            let mut coins_batch = sled::Batch::default();
            let nullifiers_batch = sled::Batch::default();
            for (tx_idx, tx) in block.transactions.iter().enumerate() {
                // Coinbase detected via PoWRewardV1 contract call (function 0x05).
                let has_pow_reward = tx_idx == 0 && tx.contract_calls.first()
                    .map_or(false, |c| c.data.first() == Some(&0x05));
                if has_pow_reward {
                    // Extract coin commitment and nullifier from PoWRewardV1 params.
                    let pow_data = &tx.contract_calls[0].data[1..]; // skip selector
                    if let Ok(params) = dwow_serial::deserialize::<dwow_native_token_contract::model::PoWRewardParamsV1>(pow_data) {
                        coins_batch.insert(&params.output.coin.inner().to_repr(), &height.to_le_bytes());
                    }
                }
            }

            // Accumulate chain work
            let block_target = block.header.target;
            let block_work = if block_target == 0 { 0u64 } else { (u32::MAX as u64) / (block_target as u64) };
            let consensus = self.consensus.lock().unwrap();
            let accumulated = consensus.accumulated_work.load(Ordering::SeqCst).saturating_add(block_work);
            consensus.accumulated_work.store(accumulated, Ordering::SeqCst);

            let mut consensus_batch = sled::Batch::default();
            consensus.save_to_batch(&mut consensus_batch);
            consensus_batch.insert("accumulated_work", &accumulated.to_le_bytes());
            drop(consensus);

            // --- Atomic commit (sled cross-tree transaction) ---
            let contracts = contracts_batch.unwrap_or_default();
            let sc_batch = supply_chain_batch.unwrap_or_default();
            (&self.store.blocks, &self.store.uncles,
             &self.store.contracts, &self.store.consensus,
             &self.store.coins, &self.store.nullifiers,
             self.supply_chain.tree())
                .transaction(|(tx_blocks, tx_uncles, tx_contracts, tx_consensus,
                               tx_coins, tx_nullifiers, tx_supply)| {
                    tx_blocks.apply_batch(&blocks_batch)?;
                    tx_uncles.apply_batch(&uncles_batch)?;
                    tx_contracts.apply_batch(&contracts)?;
                    tx_consensus.apply_batch(&consensus_batch)?;
                    tx_coins.apply_batch(&coins_batch)?;
                    tx_nullifiers.apply_batch(&nullifiers_batch)?;
                    // Supply chain entry committed atomically with everything else.
                    // No post-commit mirror needed — if this transaction succeeds,
                    // both the contracts tree and supply_chain tree have the new state.
                    tx_supply.apply_batch(&sc_batch)?;
                    Ok(())
                })
                .map_err(|e: sled::transaction::TransactionError<sled::Error>| {
                    LinearError::StorageError(format!("commit: {}", e))
                })
        })();

        // H5.2: Roll back in-memory consensus on ANY error (not just TransactionError).
        if commit_result.is_err() {
            let consensus = self.consensus.lock().unwrap();
            consensus.force_target(pre_target);
            consensus.restore_timestamps(pre_timestamps);
        }

        commit_result?;

        // --- In-memory state (only after commit succeeds) ---
        if height > current_height {
            self.set_height(height);
        }

        // Update in-memory caches (sled already committed)
        for tx in &block.transactions {
            let has_pow_reward = tx.contract_calls.first()
                .map_or(false, |c| c.data.first() == Some(&0x05));
            if has_pow_reward {
                let pow_data = &tx.contract_calls[0].data[1..]; // skip selector
                if let Ok(params) = dwow_serial::deserialize::<dwow_native_token_contract::model::PoWRewardParamsV1>(pow_data) {
                    self.coin_set.lock().unwrap().insert(CoinCommitment(params.output.coin.inner()), height);
                    // Phase 1: params.nullifier is already a typed Nullifier — no bytes round-trip
                    self.nullifier_set.lock().unwrap().insert(params.nullifier, height);
                }
            }
        }

        // --- Uncle coin creation (subtractive Pedersen split) ---
        // For each accepted uncle, create a deterministic coin at the consensus
        // level. No ZK proof needed — the split is pure Pedersen arithmetic.
        //   C_effective = C_base - sum(C_uncle_i)
        //   Supply invariant: canonical_value + sum(pin_rewards) == base_reward
        let mut _total_pin: u64 = 0;  // accumulated but consumed by caller via total_reward
        for uncle in uncles.iter().filter(|u| u.pin_accepted && u.pin_reward > 0) {
            let mut hasher = blake3::Hasher::new();
            hasher.update(uncle.header.previous.as_bytes());
            hasher.update(&uncle.header.height.to_le_bytes());
            hasher.update(&uncle.pin_reward.to_le_bytes());
            hasher.update(&height.to_le_bytes());
            let uncle_coin: [u8; 32] = hasher.finalize().into();
            // Insert uncle coin into uncle_coin_set (blake3 hash, NOT a Poseidon CoinCommitment).
            // Uncle coins are structurally different from coin commitments and SHALL NOT
            // share the same key type — per spec §8.4 (Coin ≠ [u8; 32]).
            self.uncle_coin_set.lock().unwrap().insert(uncle_coin, height);
            _total_pin += uncle.pin_reward;
        }
        // Verify supply invariant: canonical + uncles == base_reward
        let base_reward = dwow_sdk::blockchain::expected_reward(height as u32);
        let pin_rewards: Vec<u64> = uncles.iter()
            .filter(|u| u.pin_accepted && u.pin_reward > 0)
            .map(|u| u.pin_reward)
            .collect();
        CumulativeSupplyChain::verify_uncle_split(
            base_reward,
            block.header.total_reward,
            &pin_rewards,
        )?;

        // --- Coinbase maturity enforcement (Phase 3c) ---
        // Reject transactions that spend immature coinbase coins.
        // Checked at the consensus layer (not WASM) — maturity is a
        // consensus rule, not a contract rule. Bitcoin enforces it in
        // CheckInputs(); DarkWow enforces it here.
        for tx in &block.transactions {
            // Skip coinbase transactions (they create coins, don't spend).
            // Detected via PoWRewardV1 contract call (function 0x05).
            if tx.contract_calls.first().map_or(false, |c| c.data.first() == Some(&0x05)) {
                continue;
            }
            for nullifier in &tx.nullifiers {
                // Check if this nullifier was created by a coinbase output.
                // The nullifier's creation height is stored in nullifier_set.
                if let Some(&created_at) = self.nullifier_set.lock().unwrap().get(nullifier) {
                    // V.9 fix: use nullifier's own height for maturity, not coin_set lookup.
                    // Previously used nullifier.to_bytes() as coin_set key — but nullifier
                    // bytes ≠ coin commitment bytes, so the lookup always returned None,
                    // rejecting ALL non-coinbase transactions that spent coinbase outputs.
                    if height.saturating_sub(created_at) < COINBASE_MATURITY {
                        return Err(LinearError::BlockIsInvalid(
                            format!(
                                "Immature coinbase spend at height {}: nullifier created at {}, needs {} blocks maturity",
                                height, created_at, COINBASE_MATURITY
                            )
                        ));
                    }
                }
            }
        }

        // Clean up orphaned competing blocks (H11)
        self.prune_competing(height);

        // Prune in-memory coin set. This mirrors the sled coins tree for
        // fast lookup but grows unboundedly. Entries older than COINBASE_MATURITY
        // are evicted — sled is the authoritative source for old coins.
        //
        // nullifier_set is now a HashMap<[u8;32], u64> with height tracking.
        // Entries older than COINBASE_MATURITY are pruned — sled is the
        // authoritative source for pre-existing nullifiers on restart.
        const COINBASE_MATURITY: u64 = 100;
        if height > COINBASE_MATURITY {
            let prune_h = height - COINBASE_MATURITY;
            self.coin_set.lock().unwrap().retain(|_, h| *h >= prune_h);
            // Prune nullifiers older than maturity (sled is authoritative source)
            self.nullifier_set.lock().unwrap().retain(|_, h| *h >= prune_h);
        }

        info!(target: "chain_state", "Block {} at height {} committed",
            block.header.height, height);
        Ok(())
    }

    /// Convenience: apply a single block without uncles or contracts overlay.
    /// Async for caller compatibility (dwowd callers use .await). Delegates to
    /// the synchronous `connect_block`.
    pub async fn apply_block(&self, block: &Block) -> Result<()> {
        self.connect_block(block, &[], None, None)
    }

    /// Convenience: apply a block with uncles but no contracts overlay.
    /// Async for caller compatibility. Delegates to `connect_block`.
    pub async fn apply_block_with_uncles(
        &self,
        block: &Block,
        uncles: &[UncleBlock],
    ) -> Result<()> {
        self.connect_block(block, uncles, None, None)
    }

    /// Memory diagnostics: number of cached RandomX VMs.
    pub fn vm_cache_size(&self) -> usize {
        self.vm_cache.lock().unwrap().len()
    }

    /// Memory diagnostics: number of coins in the in-memory set.
    pub fn coin_set_size(&self) -> usize {
        self.coin_set.lock().unwrap().len()
    }

    /// Compute the coin merkle root including a new coin commitment.
    /// Used by block template generation for the coinbase coin.
    pub fn compute_root_including_coin(&self, new_coin: &CoinCommitment) -> [u8; 32] {
        let coins = self.coin_set.lock().unwrap();
        let mut sorted: Vec<&CoinCommitment> = coins.keys().collect();
        sorted.push(new_coin);
        sorted.sort_by_key(|c| c.to_bytes());
        let mut hasher = blake3::Hasher::new();
        for coin in sorted {
            hasher.update(&coin.to_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    /// Compute the current nullifier merkle root.
    /// Used by block template generation.
    pub fn compute_nullifier_root(&self) -> [u8; 32] {
        let nullifiers = self.nullifier_set.lock().unwrap();
        if nullifiers.is_empty() {
            return [0u8; 32]
        }
        // BTreeMap keys are already sorted; collect in order
        let mut hasher = blake3::Hasher::new();
        for n in nullifiers.keys() {
            hasher.update(&n.to_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    /// Try to find and reorganize to a longer chain from stored competing blocks.
    ///
    /// Called after receiving a P2P block (H9 trigger). Checks whether any
    /// competing blocks form a chain longer than our canonical chain.
    /// If so, calls reorganize_to with the competing chain.
    ///
    /// This is a lightweight trigger — no network requests required.
    /// Returns the number of blocks reorganized (0 if no reorg occurred).
    pub fn try_reorg_from_competing(&self) -> Result<usize> {
        let current_height = self.get_height();
        let competing = self.competing_blocks.lock().unwrap();

        // Find the max height of any competing block
        let mut peer_max = current_height;
        for (h, blocks) in competing.iter() {
            if !blocks.is_empty() && *h > peer_max {
                peer_max = *h;
            }
        }

        // No competing block is higher than our chain — nothing to reorg
        if peer_max <= current_height {
            return Ok(0);
        }

        // Walk uncle chains through competing blocks by parent→child links.
        // Instead of assuming sequential heights, find chains where each
        // block's previous_hash points to the parent block.
        let mut best_chain: Option<HashMap<u64, Block>> = None;
        let mut best_max: u64 = 0;

        // For each competing block at a height <= current_height + 1,
        // try to build a chain upward by matching previous_hash to parent.
        let heights: Vec<u64> = competing.keys().copied().collect();
        for start_h in heights {
            if start_h > current_height + 1 {
                continue;
            }
            for start_block in competing.get(&start_h).into_iter().flatten() {
                let mut chain: HashMap<u64, Block> = HashMap::new();

                // Build downward: use canonical blocks below start_h
                for h in 1..start_h {
                    if let Ok(block) = self.get_block(h) {
                        chain.insert(h, block);
                    }
                }
                chain.insert(start_h, start_block.clone());

                // Walk upward through competing blocks by previous_hash link
                let mut prev_hash = {
                    let vm = self.get_vm(start_block.header.randomx_key);
                    let guard = vm.lock().unwrap();
                    start_block.hash_with_vm(&*guard)
                };
                let mut h = start_h + 1;

                while let Some(blocks) = competing.get(&h) {
                    let child = blocks.iter().find(|b| b.header.previous == prev_hash);
                    if let Some(child_block) = child {
                        chain.insert(h, child_block.clone());
                        let child_vm = self.get_vm(child_block.header.randomx_key);
                        let child_guard = child_vm.lock().unwrap();
                        prev_hash = child_block.hash_with_vm(&*child_guard);
                        h += 1;
                    } else {
                        break;
                    }
                }

                let chain_max = chain.keys().max().copied().unwrap_or(0);
                if chain_max > current_height && chain_max > best_max {
                    // Validate chain continuity: every block's previous_hash
                    // must match the hash of the block at height-1
                    let mut valid = true;
                    for ch in (start_h + 1)..=chain_max {
                        if let (Some(block), Some(parent)) =
                            (chain.get(&ch), chain.get(&(ch - 1)))
                        {
                            let bvm = self.get_vm(parent.header.randomx_key);
                            let bguard = bvm.lock().unwrap();
                            let parent_hash = parent.hash_with_vm(&*bguard);
                            if block.header.previous != parent_hash {
                                valid = false;
                                break;
                            }
                        } else {
                            valid = false;
                            break;
                        }
                    }
                    if valid {
                        best_chain = Some(chain);
                        best_max = chain_max;
                    }
                }
            }
        }
        drop(competing);

        if let Some(peer_blocks) = best_chain {
            if best_max > current_height {
                return self.reorganize_to(&peer_blocks);
            }
        }

        Ok(0)
    }

    /// Chain reorganization: adopt a peer's chain if it is longer.
    ///
    /// Bitcoin's ActivateBestChain pattern (H9 fix, matches Python model 1:1).
    ///
    /// CRITICAL-2 fix: validates the ENTIRE peer chain BEFORE disconnecting
    /// any canonical blocks. If validation fails, our chain is untouched.
    ///
    /// Fork choice rule:
    /// 1. If peer chain is longer → reorganize
    /// 2. If same height → keep ours (first-seen-wins)
    ///
    /// Returns the number of blocks reorganized (0 if no reorg occurred).
    pub fn reorganize_to(&self, peer_blocks: &HashMap<u64, Block>) -> Result<usize> {
        let _lock = self.connect_lock.lock().unwrap();
        let current_height = self.get_height();

        // Find peer's max height
        let peer_max = peer_blocks.keys().max().copied().unwrap_or(0);

        // Find common ancestor (highest block both chains share)
        let mut ancestor: u64 = 0;
        for h in 1..=current_height {
            if let Ok(our_block) = self.get_block(h) {
                if let Some(peer_block) = peer_blocks.get(&h) {
                    let vm = self.get_vm(our_block.header.randomx_key);
                    let our_guard = vm.lock().unwrap();
                    let our_hash = our_block.hash_with_vm(&*our_guard);
                    drop(our_guard);
                    let peer_vm = self.get_vm(peer_block.header.randomx_key);
                    let peer_guard = peer_vm.lock().unwrap();
                    let peer_hash = peer_block.hash_with_vm(&*peer_guard);
                    drop(peer_guard);
                    if our_hash == peer_hash {
                        ancestor = h;
                    } else {
                        break; // chains diverge at this height
                    }
                }
            }
        }

        // Fork selection: compare accumulated work, not just height.
        // A shorter but heavier chain beats a longer but lighter one.
        // CRITICAL: compare incremental work from the fork point (ancestor),
        // NOT total work vs partial work. Both chains share genesis→ancestor.
        {
            let mut peer_work: u64 = 0;
            for h in (ancestor + 1)..=peer_max {
                if let Some(block) = peer_blocks.get(&h) {
                    let target = block.header.target;
                    if target > 0 {
                        peer_work = peer_work.saturating_add((u32::MAX as u64) / (target as u64));
                    }
                }
            }
            // Compute our incremental work from ancestor+1 to current_height
            let mut our_work_delta: u64 = 0;
            for h in (ancestor + 1)..=current_height {
                if let Ok(block) = self.get_block(h) {
                    let target = block.header.target;
                    if target > 0 {
                        our_work_delta = our_work_delta.saturating_add((u32::MAX as u64) / (target as u64));
                    }
                }
            }
            if peer_work <= our_work_delta {
                return Ok(0);
            }
        }

        // CRITICAL-2: Validate peer blocks BEFORE disconnecting ours.
        // Build in-memory temp chain from sled blocks up to ancestor.
        let mut temp_blocks: HashMap<u64, Block> = HashMap::new();
        let mut timestamps: Vec<u64> = Vec::with_capacity(20);
        for h in 1..=ancestor {
            if let Ok(block) = self.get_block(h) {
                timestamps.push(block.header.timestamp);
                if timestamps.len() > 20 { timestamps.remove(0); }
                temp_blocks.insert(h, block);
            }
        }
        let mut temp_height = ancestor;
        // C3 fix: compute initial target from accumulated timestamps of the
        // temp chain (canonical up to ancestor), NOT from the canonical store.
        // The canonical store's get_next_work_required would derive the target
        // using its own block timestamps, which may differ from the temp chain
        // when forks exist. The peer's fork has its own timestamp history from
        // ancestor+1 onward — the first peer block after fork must be validated
        // against the target derived from the chain it actually extends.
        let consensus = self.consensus.lock().unwrap();
        let mut current_target = consensus.initial_target();
        // Walk accumulated timestamps (canonical blocks 1..ancestor) to derive
        // the target at the fork point — same algorithm as get_next_work_required
        // but using temp chain timestamps instead of store reads.
        for window in timestamps.windows(2) {
            current_target = PoWConsensus::compute_adjustment(
                &[window[0], window[1]], current_target,
                consensus.target_block_time(),
                consensus.min_target(), consensus.max_target(),
            );
        }
        drop(consensus);

        for h in (ancestor + 1)..=peer_max {
            if let Some(peer_block) = peer_blocks.get(&h) {
                let vm = self.get_vm(peer_block.header.randomx_key);
                let guard = vm.lock().unwrap();
                let expected_target = current_target;
                let prev_hash = if temp_height > 0 {
                    temp_blocks.get(&temp_height).map(|b| {
                        let pvm = self.get_vm(b.header.randomx_key);
                        let pvm_guard = pvm.lock().unwrap();
                        b.hash_with_vm(&*pvm_guard)
                    })
                } else {
                    None
                };
                if validation::check_block_header(
                    peer_block, &*guard, expected_target, temp_height, prev_hash.as_ref(),
                ).is_err() {
                    return Ok(0); // Invalid peer block — abort, our chain untouched
                }

                // Finality: anchored block conflict check during reorg.
                // If our canonical chain has an anchored block at this height,
                // the peer block cannot replace it (matches Python model).
                if let Ok(existing) = self.get_block(h) {
                    if self.finality_config.should_enforce(existing.header.finality_flags)
                        && (existing.header.anchor_tx_id != [0u8; 32]
                            || existing.header.anchor_monero_height != 0)
                    {
                        return Ok(0); // Anchored block cannot be replaced
                    }
                }

                // Update target incrementally from peer chain timestamps
                // (matches Python model — uses accumulated peer chain, not store)
                timestamps.push(peer_block.header.timestamp);
                if timestamps.len() > 20 { timestamps.remove(0); }
                if timestamps.len() >= 2 {
                    let consensus = self.consensus.lock().unwrap();
                    current_target = PoWConsensus::compute_adjustment(
                        &timestamps, current_target,
                        consensus.target_block_time(),
                        consensus.min_target(),
                        consensus.max_target(),
                    );
                }

                temp_blocks.insert(h, peer_block.clone());
                temp_height = h;
            }
        }

        // Peer chain fully validated. Now atomically swap.
        // First, remove peer blocks from competing set — they're about to
        // become canonical and must not appear as uncle candidates later
        // (matches Python model reorg fix).
        {
            let mut competing = self.competing_blocks.lock().unwrap();
            let mut seen = self.competing_seen.lock().unwrap();
            for h in (ancestor + 1)..=peer_max {
                if let Some(peer_block) = peer_blocks.get(&h) {
                    if let Some(entries) = competing.get_mut(&h) {
                        let vm = self.get_vm(peer_block.header.randomx_key);
                        let guard = vm.lock().unwrap();
                        let peer_hash = peer_block.hash_with_vm(&*guard);
                        drop(guard);
                        entries.retain(|b| {
                            let bvm = self.get_vm(b.header.randomx_key);
                            let bvm_guard = bvm.lock().unwrap();
                            let h = b.hash_with_vm(&*bvm_guard);
                            if h == peer_hash {
                                seen.remove(&h);
                                false
                            } else {
                                true
                            }
                        });
                        if entries.is_empty() {
                            competing.remove(&h);
                        }
                    }
                }
            }
        }

        // Move our blocks above ancestor to competing (potential uncles).
        for h in (ancestor + 1)..=current_height {
            if let Ok(block) = self.get_block(h) {
                self.competing_blocks.lock().unwrap()
                    .entry(h)
                    .or_default()
                    .push(block);
            }
        }

        // Build batch: remove old blocks, insert peer blocks
        let mut blocks_batch = sled::Batch::default();
        for h in (ancestor + 1)..=current_height {
            blocks_batch.remove(&h.to_le_bytes());
        }

        let mut reorg_count: usize = 0;
        for h in (ancestor + 1)..=peer_max {
            if let Some(peer_block) = peer_blocks.get(&h) {
                let block_value = serde_json::to_vec(peer_block)
                    .map_err(|e| LinearError::SerializationError(e.to_string()))?;
                blocks_batch.insert(&h.to_le_bytes(), block_value);
                reorg_count += 1;
            }
        }

        // Commit batch to sled
        self.store.blocks.apply_batch(blocks_batch)
            .map_err(|e| LinearError::StorageError(format!("reorg commit: {}", e)))?;

        // Update height
        self.set_height(peer_max);

        info!(target: "chain_state",
            "Reorganized {} blocks from height {} to {} (ancestor={})",
            reorg_count, current_height, peer_max, ancestor);

        Ok(reorg_count)
    }
}
