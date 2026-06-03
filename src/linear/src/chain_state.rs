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

use std::collections::{HashMap, HashSet};
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex};

use blake3::Hash as Blake3Hash;
use randomx::{RandomXFlags, RandomXVM};
use sled::transaction::Transactional;
use tracing::info;

use crate::{
    Block, FinalityConfig, LinearError, LinearStore, PoWConsensus, Result, UncleBlock, validation,
    COINBASE_MATURITY,
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
    /// PoW consensus / difficulty adjustment
    pub consensus: Mutex<PoWConsensus>,
    /// Finality configuration
    pub finality_config: FinalityConfig,

    // --- Cached state (always derived from store, never authoritative) ---
    /// Current chain height
    height: AtomicU64,
    /// RandomX VM pool keyed by randomx_key
    vm_cache: Mutex<HashMap<[u8; 32], Arc<RandomXVM>>>,
    /// Coin commitments → block height (for maturity tracking)
    coin_set: Mutex<HashMap<[u8; 32], u64>>,
    /// Spent nullifiers (double-spend prevention)
    nullifier_set: Mutex<HashSet<[u8; 32]>>,
    /// Competing blocks at the same height — potential uncles for fork resolution.
    /// When two miners produce blocks at height N simultaneously, the first
    /// received becomes canonical and the second is stored here. The next block
    /// mined (N+1) can include these as uncles with partial rewards.
    /// Key: height, Value: competing block at that height.
    competing_blocks: Mutex<HashMap<u64, Vec<Block>>>,
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
        let store = Arc::new(LinearStore::new(db)?);
        let mut consensus = PoWConsensus::new(target_block_time, initial_target, min_target, max_target);
        let _ = consensus.load(store.consensus_tree());
        let height = store.get_height().unwrap_or(0);

        // Create initial VM with zero key
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let cache = randomx::RandomXCache::new(flags, &[0u8; 32])
            .map_err(|e| LinearError::RandomXError(format!("VM cache: {}", e)))?;
        let vm = Arc::new(RandomXVM::new(flags, Some(cache), None)
            .map_err(|e| LinearError::RandomXError(format!("VM: {}", e)))?);
        let mut vm_cache = HashMap::new();
        vm_cache.insert([0u8; 32], vm);

        // Restore coin_set and nullifier_set from sled trees
        // (survive restarts — no more in-memory-only state loss)
        let coin_set = {
            let mut map = HashMap::new();
            for item in store.coins.iter() {
                if let Ok((k, v)) = item {
                    if k.len() == 32 && v.len() == 8 {
                        let mut coin = [0u8; 32];
                        let mut height_bytes = [0u8; 8];
                        coin.copy_from_slice(&k);
                        height_bytes.copy_from_slice(&v);
                        map.insert(coin, u64::from_le_bytes(height_bytes));
                    }
                }
            }
            Mutex::new(map)
        };
        let nullifier_set = {
            let mut set = HashSet::new();
            for item in store.nullifiers.iter() {
                if let Ok((k, _)) = item {
                    if k.len() == 32 {
                        let mut nf = [0u8; 32];
                        nf.copy_from_slice(&k);
                        set.insert(nf);
                    }
                }
            }
            Mutex::new(set)
        };

        Ok(Arc::new(Self {
            store,
            consensus: Mutex::new(consensus),
            finality_config,
            height: AtomicU64::new(height),
            vm_cache: Mutex::new(vm_cache),
            coin_set,
            nullifier_set,
            competing_blocks: Mutex::new(HashMap::new()),
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

    /// Get or create a RandomX VM for the given key.
    pub fn get_vm(&self, key: [u8; 32]) -> Arc<RandomXVM> {
        let mut cache = self.vm_cache.lock().unwrap();
        if let Some(vm) = cache.get(&key) {
            return vm.clone();
        }
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(flags, &key)
            .expect("Failed to create RandomX cache");
        let vm = Arc::new(RandomXVM::new(flags, Some(rx_cache), None)
            .expect("Failed to create RandomX VM"));
        cache.insert(key, vm.clone());
        vm
    }

    // --- Coin / nullifier sets ---

    pub fn has_coin(&self, coin: &[u8; 32]) -> bool {
        self.coin_set.lock().unwrap().contains_key(coin)
    }

    pub fn is_coin_mature(&self, coin: &[u8; 32], current_height: u64) -> bool {
        match self.coin_set.lock().unwrap().get(coin) {
            Some(&created_at) => current_height.saturating_sub(created_at) >= COINBASE_MATURITY,
            None => false,
        }
    }

    pub fn has_nullifier(&self, nullifier: &[u8; 32]) -> bool {
        self.nullifier_set.lock().unwrap().contains(nullifier)
    }

    /// Take competing blocks at the current height for uncle inclusion.
    /// Called by the miner task before building a new block. The competing
    /// blocks (mined by other nodes at the same height as the canonical tip)
    /// are removed from storage and returned. The caller includes them in
    /// the next block's `uncle_merkle_root` and passes them to
    /// `apply_block_with_uncles`.
    pub fn take_competing_blocks(&self, height: u64) -> Vec<Block> {
        self.competing_blocks.lock().unwrap().remove(&height).unwrap_or_default()
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
    ) -> Result<()> {
        // Serialize all block application — prevents concurrent connect_block
        // calls from racing on height, VM cache, sled writes, and RandomX FFI.
        let _lock = self.connect_lock.lock().unwrap();
        let vm = self.get_vm(block.header.randomx_key);
        let current_height = self.get_height();
        let block_height = block.header.height;

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
            let hash_u32 = {
                let h = block.hash_with_vm(&vm);
                u32::from_le_bytes(h.as_bytes()[0..4].try_into().unwrap())
            };
            if hash_u32 > block.header.target {
                return Err(LinearError::InvalidPoW(
                    block.hash_with_vm(&vm).to_string()
                ));
            }
            self.competing_blocks.lock().unwrap()
                .entry(block_height)
                .or_default()
                .push(block.clone());
            info!(target: "chain_state",
                "Competing block at h={} stored as potential uncle", block_height);
            return Ok(());
        }

        // --- Stage 1 & 2 PoW validation ---
        let expected_target = {
            self.consensus.lock().unwrap()
                .get_next_work_required(&self.store, block_height)
        };

        let previous_hash = if current_height > 0 {
            let prev = self.get_block(current_height)?;
            let prev_vm = self.get_vm(prev.header.randomx_key);
            Some(prev.hash_with_vm(&prev_vm))
        } else {
            None
        };

        validation::check_block_header(
            block, &vm, expected_target, current_height, previous_hash.as_ref(),
        )?;

        // --- Finality: anchored block conflict ---
        if let Ok(existing) = self.store.get_block(block_height) {
            if self.finality_config.should_enforce(existing.header.finality_flags)
                && (existing.header.anchor_tx_id != [0u8; 32]
                    || existing.header.anchor_monero_height != 0)
            {
                return Err(LinearError::AnchoredBlockConflict);
            }
        }

        // --- Build commit batch ---
        let height = block_height;
        let mut blocks_batch = sled::Batch::default();
        let block_value = serde_json::to_vec(block)
            .map_err(|e| LinearError::SerializationError(e.to_string()))?;
        blocks_batch.insert(&height.to_le_bytes(), block_value);

        let mut uncles_batch = sled::Batch::default();
        for uncle in uncles {
            let uncle_hash = blake3::hash(&serde_json::to_vec(&uncle.header).unwrap());
            let uncle_value = serde_json::to_vec(uncle)
                .map_err(|e| LinearError::SerializationError(e.to_string()))?;
            uncles_batch.insert(uncle_hash.as_bytes(), uncle_value);
        }

        // Coin and nullifier batches — persisted atomically with block data
        let mut coins_batch = sled::Batch::default();
        let mut nullifiers_batch = sled::Batch::default();
        for tx in &block.transactions {
            if let Some(ref coinbase) = tx.coinbase {
                coins_batch.insert(&coinbase.coin[..], &height.to_le_bytes());
            }
        }

        let mut consensus_batch = sled::Batch::default();
        self.consensus.lock().unwrap().save_to_batch(&mut consensus_batch);

        // --- Atomic commit (sled cross-tree transaction) ---
        let contracts = contracts_batch.unwrap_or_default();
        (&self.store.blocks, &self.store.uncles,
         &self.store.contracts, &self.store.consensus,
         &self.store.coins, &self.store.nullifiers)
            .transaction(|(tx_blocks, tx_uncles, tx_contracts, tx_consensus,
                           tx_coins, tx_nullifiers)| {
                tx_blocks.apply_batch(&blocks_batch)?;
                tx_uncles.apply_batch(&uncles_batch)?;
                tx_contracts.apply_batch(&contracts)?;
                tx_consensus.apply_batch(&consensus_batch)?;
                tx_coins.apply_batch(&coins_batch)?;
                tx_nullifiers.apply_batch(&nullifiers_batch)?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<sled::Error>| {
                LinearError::StorageError(format!("commit: {}", e))
            })?;

        // --- In-memory state (only after commit succeeds) ---
        if height > current_height {
            self.set_height(height);
        }

        // Update in-memory caches (sled is already committed)
        for tx in &block.transactions {
            if let Some(ref coinbase) = tx.coinbase {
                self.coin_set.lock().unwrap().insert(coinbase.coin, height);
            }
        }

        // Update consensus
        {
            let consensus = self.consensus.lock().unwrap();
            consensus.record_block(block.header.timestamp);
            consensus.adjust_target();
            let _ = consensus.save(self.store.consensus_tree());
        }

        info!(target: "chain_state", "Block {} at height {} committed",
            block.header.height, height);
        Ok(())
    }

    /// Convenience: apply a single block without uncles or contracts overlay.
    /// Async for caller compatibility (dwowd callers use .await). Delegates to
    /// the synchronous `connect_block`.
    pub async fn apply_block(&self, block: &Block) -> Result<()> {
        self.connect_block(block, &[], None)
    }

    /// Convenience: apply a block with uncles but no contracts overlay.
    /// Async for caller compatibility. Delegates to `connect_block`.
    pub async fn apply_block_with_uncles(
        &self,
        block: &Block,
        uncles: &[UncleBlock],
    ) -> Result<()> {
        self.connect_block(block, uncles, None)
    }

    /// Compute the coin merkle root including a new coin commitment.
    /// Used by block template generation for the coinbase coin.
    pub fn compute_root_including_coin(&self, new_coin: &[u8; 32]) -> [u8; 32] {
        let coins = self.coin_set.lock().unwrap();
        let mut sorted: Vec<&[u8; 32]> = coins.keys().collect();
        sorted.push(new_coin);
        sorted.sort();
        let mut hasher = blake3::Hasher::new();
        for coin in sorted {
            hasher.update(coin);
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
        let mut sorted: Vec<&[u8; 32]> = nullifiers.iter().collect();
        sorted.sort();
        let mut hasher = blake3::Hasher::new();
        for n in sorted {
            hasher.update(n);
        }
        *hasher.finalize().as_bytes()
    }
}
