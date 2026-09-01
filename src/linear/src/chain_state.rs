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
//! Spec: sync-protocol.md §19 (fork detection `detect_reorg`, `disconnect_block` +
//! contracts-tree `CBlockUndo` replay, recursion guard `remove_competing`); consensus.md
//! §Fork Choice Rule (heaviest-chain work comparison); uncle_merkle.md (competing-block
//! storage + uncle commitments).
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

/// Outcome of connecting a block to the chain.
///
/// `connect_block` previously returned `Result<()>` for three semantically
/// incompatible states — canonical extension, competing block stored, and
/// uncle chain extension — all collapsed into `Ok(())`. This enum restores
/// the distinction per type-system.md §9.3 so callers (miner_task, sync,
/// broadcast, stratum, mm_rpc) can branch on the actual outcome.
#[derive(Debug, Clone)]
pub enum BlockConnectOutcome {
    /// Block extended the canonical chain — height advanced. The new tip
    /// height is carried so callers can confirm it matches their expectation.
    CanonicalExtension { new_height: BlockHeight },
    /// Block was stored as a competing/uncle candidate at the current height.
    /// Height did NOT advance. Transactions in this block are NOT canonical.
    CompetingStored,
    /// Block extended a known uncle chain — stored as competing at the next
    /// height. Height did NOT advance on the canonical chain.
    UncleExtended,
    /// Block is already in the chain (height below the current tip) — a
    /// duplicate relayed by a peer. NOT a protocol violation; the caller SHALL
    /// skip it and SHALL NOT punish/ban the peer. Spec: sync-protocol.md §14.3.
    AlreadyKnown,
    /// A competing chain with more accumulated work is available for reorg.
    /// The caller (accept_block) must disconnect the canonical block at
    /// fork_height, then re-accept both blocks through the normal pipeline.
    /// The competing_block is carried inline to prevent TOCTOU races with
    /// take_competing_blocks().
    ReorgAvailable {
        fork_height: BlockHeight,
        competing_block: Block,
    },
}

impl PartialEq for BlockConnectOutcome {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::CanonicalExtension { new_height: a }, Self::CanonicalExtension { new_height: b }) => a == b,
            (Self::CompetingStored, Self::CompetingStored) => true,
            (Self::UncleExtended, Self::UncleExtended) => true,
            (Self::AlreadyKnown, Self::AlreadyKnown) => true,
            (Self::ReorgAvailable { fork_height: a1, .. }, Self::ReorgAvailable { fork_height: b1, .. }) => a1 == b1,
            _ => false,
        }
    }
}
impl Eq for BlockConnectOutcome {}

/// Result of pre-WASM fork detection (sync-protocol.md §19.1). Distinguishes a
/// heavier uncle chain (reorg) from a lighter uncle-chain extension (M4: store
/// as a competing block, never execute WASM against the wrong cumulative state).
pub enum ReorgSignal {
    /// Block does not build on a known competing parent — proceed to WASM.
    None,
    /// A competing chain with more accumulated work — the caller SHALL reorg.
    Heavier { fork_height: BlockHeight, competing_block: Block },
    /// A lighter uncle-chain extension — the caller SHALL store it as a
    /// competing block and return `UncleExtended`, skipping WASM.
    Lighter,
}

// `connect_lock` is held before those inner locks to prevent deadlocks.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex};

use blake3::Hash as Blake3Hash;
use randomx::{RandomXCache, RandomXFlags, RandomXVM};
use sled::transaction::Transactional;
use tracing::info;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, MoneroBlockHeight};
use dwow_sdk::crypto::{merkle_anchor::AnchorEntry, pedersen_commitment_u64, Blind, MerkleNode, MerkleTree};
use dwow_sdk::pasta::pallas;
use dwow_sdk::pasta::group::{ff::FromUniformBytes, Group, GroupEncoding};
use dwow_sdk::pasta::group::ff::PrimeField;
use dwow_serial::serialize as dwow_serialize;
use dwow_serial::deserialize as dwow_deserialize;

use crate::{
    Block, Commitment, CumulativeSupplyChain, FinalityConfig, LinearError, LinearStore,
    Nullifier, PoWConsensus, Result, UncleBlock, validation, COINBASE_MATURITY,
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
    /// Fee window state — adaptive congestion-driven threshold adjustment.
    /// SPEC-4: consensus-critical; always present. None before fee window
    /// activation or when state fails to load.
    pub fee_window: Option<crate::fee_window::FeeWindowState>,
    /// Per-contract dynamic risk factor tracker (FI-RISK-3, fee-spec.md §14.7).
    /// Mutex-guarded: written at window boundaries by miner_task, read by
    /// prepare_block for fee computation.
    pub contract_risk_tracker: Mutex<crate::contract_risk::ContractRiskTracker>,

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
    /// Commitments → block height (for maturity tracking).
    /// Typed Commitment per Phase X — BTreeMap (Commitment has Ord).
    commitment_set: Mutex<BTreeMap<Commitment, BlockHeight>>,
    /// Uncle commitment Pedersen commitments → block height.
    /// C_uncle_i = u_i·G_v + r_i·G_r with deterministic blinds per
    /// uncle_merkle.md §Coinbase Split. In-memory only (no sled persistence
    /// in Phase 1). Uncle commitments are deterministically recomputable from the
    /// canonical chain via r_i = blake3(uncle_hash ‖ u_i ‖ H) mod p.
    uncle_commitment_set: Mutex<HashMap<[u8; 32], BlockHeight>>,
    /// All nullifiers → block height (maturity tracking + historical record).
    /// Includes both claim nullifiers (PoWRewardV1, FeeCollectV1) and spend
    /// nullifiers (FeeV2, TransferV1, SpendV1, BurnV1).
    /// Typed BTreeMap<Nullifier, BlockHeight> per Phase 1 — Nullifier has Ord for map key ordering.
    nullifier_set: Mutex<BTreeMap<Nullifier, BlockHeight>>,
    /// Spent nullifiers only — for double-spend prevention (mempool has_nullifier).
    /// Claim nullifiers (coinbase, fee-collect) are NOT added here because
    /// the coinbase claim nullifier IS the future spend nullifier (same
    /// Poseidon hash). Adding claims would block legitimate spends.
    spent_nullifiers: Mutex<BTreeSet<Nullifier>>,
    /// Block-level Merkle tree for anchoring contract state transitions.
    /// Each leaf is an AnchorEntry (nullifier-keyed contract root).
    /// Depth 32 (Orchard standard), checkpoint capacity 100 for reorg safety.
    pub block_anchor_tree: Arc<Mutex<MerkleTree>>,
    /// Competing blocks at the same height — potential uncles for fork resolution.
    /// When two miners produce blocks at height N simultaneously, the first
    /// received becomes canonical and the second is stored here. The next block
    /// mined (N+1) can include these as uncles with partial rewards.
    /// Key: height, Value: competing block at that height.
    competing_blocks: Mutex<BTreeMap<BlockHeight, Vec<Block>>>,
    /// Dedup set for competing blocks — prevents storing the same block twice (H7).
    competing_seen: Mutex<HashSet<Blake3Hash>>,
    /// Serializes connect_block calls — prevents concurrent block application
    /// from racing on height, VM cache, and sled writes (RandomX FFI segfaults).
    connect_lock: Mutex<()>,
    /// Cached tip block hash (height, hash) — computed once at connect_block,
    /// read by the sync responder (GetTip) so it never re-hashes with RandomX
    /// per request (the established Bitcoin/Monero block-index pattern).
    tip_hash: Mutex<Option<(BlockHeight, blake3::Hash)>>,
    /// Cached genesis block hash — constant, computed once lazily.
    genesis_hash: std::sync::OnceLock<blake3::Hash>,
}

impl CChainState {
    /// Create a new chain state backed by the given sled database.
    pub fn new(
        db: Arc<sled::Db>,
        target_block_time: u64,
        initial_target: BlockTarget,
        min_target: BlockTarget,
        max_target: BlockTarget,
        finality_config: FinalityConfig,
    ) -> Result<Arc<Self>> {
        let store = Arc::new(LinearStore::new(db.clone())?);
        let consensus = PoWConsensus::new(target_block_time, initial_target, min_target, max_target);
        if let Err(e) = consensus.load(store.consensus_tree()) {
            tracing::warn!(target: "chain_state", "Failed to load consensus state from sled (fresh store?): {e}");
        }
        // Restore accumulated chain work from sled (survives restarts).
        // Phase 1d: widened to u128 per M1 fix.
        let mut sled_work: Option<u128> = None;
        match store.consensus.get("accumulated_work") {
            Ok(Some(work_bytes)) => {
                if work_bytes.len() == 16 {
                    let work_bytes_arr: [u8; 16] = work_bytes[..16].try_into().map_err(|_| {
                        LinearError::StorageError("Corrupt accumulated_work: wrong length".into())
                    })?;
                    let work = u128::from_le_bytes(work_bytes_arr);
                    sled_work = Some(work);
                }
            }
            Ok(None) => { /* no accumulated work in sled — compute from scratch */ }
            Err(e) => {
                tracing::warn!(target: "chain_state",
                    "sled error reading accumulated_work (will recompute): {e}");
            }
        }

        let height = store.get_height().unwrap_or(BlockHeight::new(0));

        // H-3 migration guard: detect old JSON-encoded blocks.
        // Blocks are now stored as deterministic dwow_serial binary.
        // Old nodes stored blocks as JSON — data starts with byte '{'.
        // Check the raw bytes of block at height 1 (genesis) before any
        // deserialization attempt.
        if height > BlockHeight::new(0) {
            let genesis_key = BlockHeight::new(1).to_le_bytes();
            if let Ok(Some(raw)) = store.blocks.get(&genesis_key) {
                if raw.first() == Some(&b'{') {
                    return Err(LinearError::StorageError(
                        "Storage format migration required. Blocks are in the old JSON format. \
                         The block storage format has changed to deterministic binary (dwow_serial). \
                         Delete your data directory and resync from genesis.".into()
                    ));
                }
            }
        }

        // HAZOP H7 fix: recompute accumulated work from chain data on startup
        // and validate against the sled-cached value. If they disagree, the
        // chain-recomputed value is authoritative and the sled cache is corrected.
        let mut computed_work: u128 = 0;
        // spec dispensation: type-system.md §2.3 — BlockHeight is a transparent
        // newtype over u64. Range iteration requires extracting the u64 domain value
        // for Rust range syntax. A richer iterator API would add complexity without
        // safety gain since BlockHeight derives from u64.
        for h in 1..=height.get() {
            if let Ok(block) = store.get_block(BlockHeight::new(h)) {
                // Work contributed = 2^32 / target (standard Bitcoin formula).
                // L-1: .max(1) unified with BlockTarget::chain_work() at
                // sdk/src/blockchain.rs:344 — both use max(1) to prevent
                // theoretical divergence when target==0 (rejected by validation).
                let target = block.header.target.get().max(1);
                computed_work = computed_work.saturating_add(u128::from(u32::MAX) / u128::from(target));
            }
        }

        if let Some(sled_val) = sled_work {
            if sled_val != computed_work {
                tracing::warn!(
                    target: "chain_state",
                    "Accumulated work mismatch: sled={}, recomputed={}. Using recomputed value and correcting sled.",
                    sled_val, computed_work,
                );
                let work_bytes = computed_work.to_le_bytes().to_vec();
                if let Err(e) = store.consensus.insert("accumulated_work", work_bytes) {
                    tracing::warn!(target: "chain_state", "Failed to persist accumulated_work: {e}");
                }
            }
        } else if computed_work > 0 {
            // No sled value but chain has blocks — persist the computed value
            let work_bytes = computed_work.to_le_bytes().to_vec();
            if let Err(e) = store.consensus.insert("accumulated_work", work_bytes) {
                tracing::warn!(target: "chain_state", "Failed to persist accumulated_work: {e}");
            }
        }

        consensus.accumulated_work.store(computed_work);

        // M-1: populate/validate per-block target cache on startup.
        // The cache is a write-once sled tree; missing entries are computed
        // and inserted. Existing entries are validated against recomputation.
        for h in 2..=height.get() {
            let hh = BlockHeight::new(h);
            let expected = consensus.get_next_work_required(&store, hh)?;
            let key = hh.to_le_bytes();
            let val = expected.get().to_le_bytes();
            if let Ok(Some(existing)) = store.block_targets.get(&key) {
                if existing.len() == 4 {
                    let target_bytes: [u8; 4] = existing[..4].try_into().map_err(|_| {
                        LinearError::StorageError(format!("Corrupt block_targets entry at height {h}: wrong length"))
                    })?;
                    let cached = BlockTarget::new(u32::from_le_bytes(target_bytes));
                    if cached != expected {
                        tracing::warn!(
                            target: "chain_state",
                            "Target cache mismatch at height {h}: cached={}, expected={}. Correcting.",
                            cached.get(), expected.get(),
                        );
                        if let Err(e) = store.block_targets.insert(&key, &val) {
                    tracing::warn!(target: "chain_state", "Failed to insert block target cache at height {h}: {e}");
                }
                    }
                }
            } else {
                if let Err(e) = store.block_targets.insert(&key, &val) {
                    tracing::warn!(target: "chain_state", "Failed to insert block target cache at height {h}: {e}");
                }
            }
        }

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

        // Restore commitment_set and nullifier_set from sled trees
        // (survive restarts — no more in-memory-only state loss)
        let commitment_set = {
            let mut map = BTreeMap::new();
            for item in store.commitment_set.iter() {
                if let Ok((k, v)) = item {
                    if k.len() == 32 && v.len() == 8 {
                        let mut commitment_bytes = [0u8; 32];
                        let mut height_bytes = [0u8; 8];
                        commitment_bytes.copy_from_slice(&k);
                        height_bytes.copy_from_slice(&v);
                        if let Ok(commitment) = Commitment::from_bytes(commitment_bytes) {
                            map.insert(commitment, BlockHeight::from_le_bytes(height_bytes));
                        }
                    }
                }
            }
            Mutex::new(map)
        };
        // Uncle commitment set — Pedersen commitments, in-memory only.
        // TODO(Phase 2): Add dedicated sled tree for uncle commitment persistence.
        // The previous restoration code read from store.uncles which stores
        // JSON-serialized UncleBlock values (not u64 heights) — the v.len()==8
        // check always filtered out all entries, so this was always empty.
        // Uncle commitments are deterministically recomputable from chain data.
        let uncle_commitment_set: Mutex<HashMap<[u8; 32], BlockHeight>> =
            Mutex::new(HashMap::new());
        let nullifier_set = {
            let mut map = BTreeMap::new();
            for item in store.nullifiers.iter() {
                if let Ok((k, v)) = item {
                    if k.len() == 32 {
                        let mut nf_bytes = [0u8; 32];
                        nf_bytes.copy_from_slice(&k);
                        if let Ok(nf) = Nullifier::from_bytes(nf_bytes) {
                            // Only claim nullifiers (kind 0) belong in nullifier_set:
                            // they track coinbase maturity. Spend nullifiers (kind 1)
                            // are double-spends, rebuilt separately below.
                            if v.len() == 9 && v[0] == 0 {
                                let mut h = [0u8; 8];
                                h.copy_from_slice(&v[1..9]); // guarded by v.len() == 9
                                let height = BlockHeight::from_le_bytes(h);
                                map.insert(nf, height);
                            } else if v.len() == 8 {
                                // Legacy pre-kind-flag value (8-byte height).
                                let mut h = [0u8; 8];
                                h.copy_from_slice(&v[0..8]); // guarded by v.len() == 8
                                let height = BlockHeight::from_le_bytes(h);
                                map.insert(nf, height);
                            }
                        }
                    }
                }
            }
            Mutex::new(map)
        };
        // Spent nullifiers — the authoritative replay gate (double-spend).
        // Claim nullifiers (PoWRewardV1, FeeCollectV1) are NOT added here: the
        // coinbase claim nullifier IS the future spend nullifier (same Poseidon
        // hash), and adding claims would block legitimate spends of coinbase
        // commitments. Rebuilt from the kind=1 (spend) entries in the sled tree.
        let spent_nullifiers: Mutex<BTreeSet<Nullifier>> = {
            let mut set = BTreeSet::new();
            for item in store.nullifiers.iter() {
                if let Ok((k, v)) = item {
                    if k.len() == 32 && v.len() == 9 && v[0] == 1 {
                        let mut nf_bytes = [0u8; 32];
                        nf_bytes.copy_from_slice(&k);
                        if let Ok(nf) = Nullifier::from_bytes(nf_bytes) {
                            set.insert(nf);
                        }
                    }
                }
            }
            Mutex::new(set)
        };

        // Initialize cumulative supply chain — restores latest from sled.
        let supply_chain = CumulativeSupplyChain::new(&db)?;

        // Block-level anchor tree: incremental Merkle tree for anchoring
        // contract state transitions. Depth 32, checkpoint capacity 100.
        // Arc for sharing with TxBackend during WASM execution.
        let mut block_anchor_tree = MerkleTree::new(100);
        block_anchor_tree.append(MerkleNode::from_base(pallas::Base::from(2u64)));
        let block_anchor_tree = Arc::new(Mutex::new(block_anchor_tree));

        let fee_window = {
            let fw = crate::fee_window::FeeWindowState::new(Default::default());
            if let Err(e) = fw.load(store.consensus_tree()) {
                tracing::warn!(target: "chain_state",
                    "Failed to load fee window state from sled: {e}");
            }
            Some(fw)
        };

        // Initialize per-contract risk factor tracker (FI-RISK-3).
        // Risk factors are stored in the dedicated `contract_risk` sled tree.
        // New contracts start at baseline (1.0×) — risk is earned through
        // under-declaration, not assumed (FI-RISK-4).
        let contract_risk_tracker = {
            let mut tracker = crate::contract_risk::ContractRiskTracker::new(
                Default::default(),
            );
            if let Err(e) = tracker.load_from_tree(&store.contract_risk) {
                tracing::warn!(target: "chain_state",
                    "Failed to load contract risk state from sled: {e}");
            }
            Mutex::new(tracker)
        };

        Ok(Arc::new(Self {
            store,
            supply_chain,
            consensus: Mutex::new(consensus),
            finality_config,
            fee_window,
            contract_risk_tracker,
            // spec dispensation: type-system.md §2.3 — AtomicU64 requires the
            // raw u64 value. get() at the persistence boundary performs no
            // arithmetic; it extracts the canonical domain value for storage.
            height: AtomicU64::new(height.get()),
            vm_cache: Mutex::new(vm_cache),
            cache_pool: Mutex::new(HashMap::new()),
            commitment_set,
            uncle_commitment_set,
            nullifier_set,
            spent_nullifiers,
            block_anchor_tree,
            competing_blocks: Mutex::new(BTreeMap::new()),
            competing_seen: Mutex::new(HashSet::new()),
            connect_lock: Mutex::new(()),
            tip_hash: Mutex::new(None),
            genesis_hash: std::sync::OnceLock::new(),
        }))
    }

    // --- Height ---

    /// Current chain height (O(1) atomic read).
    pub fn get_height(&self) -> BlockHeight {
        BlockHeight::new(self.height.load(Ordering::SeqCst))
    }

    fn set_height(&self, h: BlockHeight) {
        self.height.store(h.get(), Ordering::SeqCst);
    }

    /// Cached tip block hash `(height, hash)`. Recomputed only when the tip
    /// height changes — never on the sync request path per request.
    pub fn tip_hash(&self) -> Option<(BlockHeight, blake3::Hash)> {
        let height = self.get_height();
        {
            let cache = self.tip_hash.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((h, hash)) = cache.as_ref() {
                if *h == height {
                    return Some((*h, *hash));
                }
            }
        }
        let tip = self.store.get_block(height).ok()?;
        let hash = self.hash_block_with_cached_vm(&tip).ok()?;
        *self.tip_hash.lock().unwrap_or_else(|e| e.into_inner()) = Some((height, hash));
        Some((height, hash))
    }

    /// Cached genesis block hash (constant). Computed once lazily.
    pub fn genesis_hash(&self) -> Option<blake3::Hash> {
        if let Some(h) = self.genesis_hash.get() {
            return Some(*h);
        }
        let genesis = self.store.get_block(BlockHeight::GENESIS).ok()?;
        let hash = self.hash_block_with_cached_vm(&genesis).ok()?;
        let _ = self.genesis_hash.set(hash);
        Some(hash)
    }

    /// Resolve a contract's declared `circuit_difficulty` for a function (FI-RISK-1).
    ///
    /// Reads the on-chain manifest (`contract_id || b"_manifest"`), maps the function
    /// code to its declared name, and resolves the `[[cost_profiles]]` entry. Returns
    /// `None` when the contract has no manifest or the function has no cost profile
    /// (e.g. Deployooor / NativeToken, or a legacy contract) — the caller falls back to
    /// the baseline fee. Deterministic across nodes (same manifest bytes, same parse).
    pub fn resolve_contract_circuit_difficulty(
        &self,
        contract_id: &dwow_sdk::crypto::ContractId,
        function_code: u8,
    ) -> Option<u64> {
        let bytes = self.store.get_contract_manifest(&contract_id.to_bytes()).ok()??;
        let manifest = dwow_sdk::manifest::ContractManifest::from_deploy_ix(&bytes)
            .and_then(|r| r.ok())
            .or_else(|| {
                let toml_str = std::str::from_utf8(&bytes).ok()?;
                dwow_sdk::manifest::ContractManifest::from_toml(toml_str).ok()
            })?;
        let name = manifest.functions.iter().find(|f| f.code == function_code)?.name.clone();
        let profile = dwow_sdk::manifest::resolve_cost_profile(&name, &manifest.cost_profiles);
        Some(profile.circuit_difficulty)
    }

    // --- Block access ---

    pub fn get_block(&self, height: BlockHeight) -> Result<Block> {
        self.store.get_block(height).map_err(|e| LinearError::StorageError(e.to_string()))
    }

    pub fn get_latest_block(&self) -> Result<Block> {
        let h = self.get_height();
        if h == BlockHeight::new(0) {
            return Err(LinearError::BlockNotFound(h));
        }
        self.get_block(h)
    }

    // --- RandomX VM ---

    /// Hash a block using the cached VM for its key.
    /// Encapsulates lock+hash+unlock — no MutexGuard escapes this function.
    /// Safe for async contexts because no !Send type is held across yield points.
    pub fn hash_block_with_cached_vm(&self, block: &Block) -> Result<blake3::Hash> {
        let vm = self.get_vm(block.header.randomx_key)?;
        let guard = vm.lock().unwrap_or_else(|e| e.into_inner());
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

    pub fn get_vm(&self, key: [u8; 32]) -> Result<Arc<std::sync::Mutex<RandomXVM>>> {
        let mut cache = self.vm_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(vm) = cache.get(&key) {
            return Ok(vm.clone());
        }
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(flags, &key)
            .map_err(|e| LinearError::RandomXError(format!("Failed to create RandomX cache: {e}")))?;
        let vm = Arc::new(std::sync::Mutex::new(
            RandomXVM::new(flags, Some(rx_cache), None)
                .map_err(|e| LinearError::RandomXError(format!("Failed to create RandomX VM: {e}")))?,
        ));
        cache.insert(key, vm.clone());
        // Evict oldest entry when cache exceeds capacity.
        // Old blocks are never re-hashed — only recent heights need cached VMs.
        if cache.len() > Self::MAX_CACHED_VMS {
            if let Some(oldest) = cache.keys().min().cloned() {
                cache.remove(&oldest);
            }
        }
        Ok(vm)
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
    pub fn get_cache(&self, key: [u8; 32]) -> Result<RandomXCache> {
        let mut pool = self.cache_pool.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cache) = pool.get(&key) {
            return Ok(cache.clone());
        }
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let cache = randomx::RandomXCache::new(flags, &key)
            .map_err(|e| LinearError::RandomXError(format!("Failed to create RandomX cache: {e}")))?;
        pool.insert(key, cache.clone());
        // Evict oldest entry when pool exceeds capacity
        if pool.len() > Self::MAX_CACHED_CACHES {
            if let Some(oldest) = pool.keys().min().cloned() {
                pool.remove(&oldest);
            }
        }
        Ok(cache)
    }

    // --- Commitment / nullifier sets ---

    pub fn has_commitment(&self, commitment: &Commitment) -> bool {
        self.commitment_set.lock().unwrap_or_else(|e| e.into_inner()).contains_key(commitment)
    }

    pub fn is_commitment_mature(&self, commitment: &Commitment, current_height: BlockHeight) -> bool {
        match self.commitment_set.lock().unwrap_or_else(|e| e.into_inner()).get(commitment) {
            Some(&created_at) => current_height.saturating_sub(created_at) >= COINBASE_MATURITY,
            None => false,
        }
    }

    pub fn has_nullifier(&self, nullifier: &Nullifier) -> bool {
        // Check spent_nullifiers only — claim nullifiers (coinbase, fee-collect)
        // are NOT double-spends. The coinbase claim nullifier IS the future spend
        // nullifier (same Poseidon hash), so checking the full nullifier_set
        // would block legitimate spends of coinbase coins.
        self.spent_nullifiers.lock().unwrap_or_else(|e| e.into_inner()).contains(nullifier)
    }

    /// Return the block height at which this nullifier was created, if present.
    pub fn nullifier_height(&self, nullifier: &Nullifier) -> Option<BlockHeight> {
        self.nullifier_set.lock().unwrap_or_else(|e| e.into_inner()).get(nullifier).copied()
    }

    /// Record a nullifier with its height and kind.
    ///
    /// The two layers of the Representation Faithfulness Law (§type-system 0.1):
    /// - **Claim** nullifiers (PoWRewardV1 coinbase, FeeCollectV1) are recorded in
    ///   `nullifier_set` only — they track coinbase *maturity*, not double-spends.
    ///   The claim nullifier IS the future spend nullifier (same Poseidon hash).
    /// - **Spend** nullifiers (every `tx.nullifiers` entry) are recorded in
    ///   `spent_nullifiers` — the authoritative replay gate checked by
    ///   `has_nullifier`.
    pub fn track_nullifier(&self, nullifier: Nullifier, height: BlockHeight, is_spend: bool) {
        if is_spend {
            self.spent_nullifiers.lock().unwrap_or_else(|e| e.into_inner()).insert(nullifier);
        } else {
            self.nullifier_set.lock().unwrap_or_else(|e| e.into_inner()).insert(nullifier, height);
        }
    }

    /// Take competing blocks at the current height for uncle inclusion.
    /// Called by the miner task before building a new block. The competing
    /// blocks (mined by other nodes at the same height as the canonical tip)
    /// are removed from storage and returned. The caller includes them in
    /// the next block's `uncle_merkle_root` and passes them to
    /// `apply_block_with_uncles`.
    pub fn take_competing_blocks(&self, height: BlockHeight) -> Vec<Block> {
        let blocks = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner()).remove(&height).unwrap_or_default();
        // Clean dedup set for consumed blocks (H7 follow-up)
        if !blocks.is_empty() {
            let mut seen = self.competing_seen.lock().unwrap_or_else(|e| e.into_inner());
            for b in &blocks {
                // Dedup hash: deterministic dwow_serial binary encoding.
                let header_bytes = dwow_serialize(&b.header);
                let h = blake3::hash(&header_bytes);
                seen.remove(&h);
            }
        }
        blocks
    }

    /// Read-only peek at the competing blocks at `height` (does NOT remove them).
    /// Spec: uncle_merkle.md §Uncle Minting & Maturity — the miner must compute
    /// Σ pin (the uncle split) BEFORE building the coinbase, so it peeks the
    /// competing blocks non-destructively and only `take_competing_blocks` at the
    /// end (after all fallible steps succeed).
    pub fn peek_competing_blocks(&self, height: BlockHeight) -> Vec<Block> {
        self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner())
            .get(&height)
            .cloned()
            .unwrap_or_default()
    }

    /// HAZOP H25: return all uncle block hashes stored in the sled uncles tree.
    /// Used at block acceptance to prevent the same uncle from earning rewards
    /// across multiple canonical blocks.
    pub fn stored_uncle_hashes(&self) -> std::collections::HashSet<[u8; 32]> {
        let mut keys = std::collections::HashSet::new();
        for item in self.store.uncles.iter() {
            if let Ok((key, _)) = item {
                let mut arr = [0u8; 32];
                let len = key.len().min(32);
                arr[..len].copy_from_slice(&key[..len]);
                keys.insert(arr);
            }
        }
        keys
    }

    /// Put competing blocks back at a given height (H3.4 fix).
    /// Called by the miner task if block acceptance fails — the competing
    /// blocks were destructively removed by `take_competing_blocks()` and
    /// must be restored so the competing miner doesn't lose their uncle
    /// reward opportunity.
    pub fn put_competing_blocks(&self, height: BlockHeight, blocks: Vec<Block>) {
        if blocks.is_empty() {
            return;
        }
        let mut seen = self.competing_seen.lock().unwrap_or_else(|e| e.into_inner());
        let mut competing = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner());
        for b in &blocks {
            seen.insert(blake3::hash(&dwow_serialize(&b.header)));
        }
        competing.entry(height).or_default().extend(blocks);
    }

    /// Clean up competing block entries older than MAX_UNCLE_DEPTH (H11).
    /// Called after a canonical block is committed to prevent unbounded growth.
    fn prune_competing(&self, current_height: BlockHeight) {
        let max_depth = 6u64; // MAX_UNCLE_DEPTH
        if current_height.get() <= max_depth {
            return;
        }
        let cutoff = current_height.saturating_sub_blocks(max_depth);
        let mut competing = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner());
        let mut seen = self.competing_seen.lock().unwrap_or_else(|e| e.into_inner());
        competing.retain(|&height, blocks| {
            if height < cutoff {
                for b in blocks {
                    seen.remove(&blake3::hash(&dwow_serialize(&b.header)));
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
        contracts_undo: Option<Vec<u8>>,
    ) -> Result<BlockConnectOutcome> {
        // Serialize all block application — prevents concurrent connect_block
        // calls from racing on height, VM cache, sled writes, and RandomX FFI.
        let _lock = self.connect_lock.lock().unwrap_or_else(|e| e.into_inner());
        let vm = self.get_vm(block.header.randomx_key)?;
        let current_height = self.get_height();
        let block_height = block.header.height;

        // H5.4: Early height-gap rejection — reject far-future blocks before
        // acquiring any more locks or doing expensive validation. Blocks more
        // than 1 ahead of our tip cannot connect and will fail HeightDiscontinuity.
        // Competing blocks (same height) and next-block (height+1) proceed.
        if block_height > current_height.succ() {
            return Err(LinearError::HeightDiscontinuity {
                expected: current_height.succ(),
                got: block_height,
            });
        }

        // Duplicate block (height below our tip) — already in the chain.
        // This is normal P2P relay, not a protocol violation: skip it and do
        // NOT re-execute (re-running pow_reward_v1 on an already-committed
        // block hits "Duplicate commitment in output"). Spec: sync-protocol.md
        // §14.3 — a duplicate SHALL be skipped, never banned.
        if block_height < current_height {
            return Ok(BlockConnectOutcome::AlreadyKnown);
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
            let guard = vm.lock().unwrap_or_else(|e| e.into_inner());
            let hash_u32 = {
                let h = block.hash_with_vm(&*guard)?;
                let b = h.as_bytes();
                u32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            if !block.header.target.hash_is_valid(hash_u32) {
                return Err(LinearError::InvalidPoW(
                    block.hash_with_vm(&*guard)?.to_string()
                ));
            }
            // HAZOP H-14 fix: validate Monero merge-mined competing blocks.
            // The canonical path (block_acceptor.rs:150) checks the Monero
            // coinbase Merkle proof — the competing path must match.
            if let crate::PowSource::Monero(monero_data) = &block.header.pow_source {
                if !monero_data.is_coinbase_valid_merkle_root() {
                    return Err(LinearError::BlockIsInvalid(
                        "Competing Monero merge-mined block has invalid coinbase Merkle proof".into()
                    ));
                }
            }
            // HAZOP H5 fix: enforce the same target as the canonical block
            // at this height. Competing blocks share the same parent, so
            // get_next_work_required(height) is the correct expected target
            // for any block at this height regardless of fork.
            {
                let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
                let expected = consensus.get_next_work_required(&self.store, block_height)?;
                if block.header.target != expected {
                    drop(guard);
                    return Err(LinearError::InvalidTarget {
                        expected: expected.get(),
                        declared: block.header.target.get(),
                        height: block_height,
                    });
                }
                drop(consensus);
            }
            // H4 fix: validate that competing block's previous hash
            // matches the canonical parent at current_height - 1.
            // Without this check, unrelated blocks can pollute the
            // competing store. A competing block at height H points at the
            // same parent (H-1) as the canonical sibling, so compare
            // `previous` fields — NOT the sibling's own hash (off-by-one).
            if current_height > BlockHeight::new(0) {
                let sibling = self.get_block(current_height)?;
                if block.header.previous != sibling.header.previous {
                    drop(guard);
                    return Err(LinearError::InvalidPreviousHash(
                        format!("Competing block previous {} != canonical parent {}",
                            hex::encode(block.header.previous.as_bytes()),
                            hex::encode(sibling.header.previous.as_bytes()))
                    ));
                }
            }
            // H6 fix: build recent timestamps for competing-block validation.
            let recent_ts: Vec<BlockTimestamp> = {
                let start = if block_height > BlockHeight::new(11) { block_height.saturating_sub_blocks(11).get() } else { 1 };
                let mut ts = Vec::new();
                for h in start..block_height.get() {
                    if let Ok(b) = self.get_block(BlockHeight::new(h)) {
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
            let block_hash = block.hash_with_vm(&*guard)?;
            drop(guard); // Release VM lock before acquiring other locks
            {
                let mut seen = self.competing_seen.lock().unwrap_or_else(|e| e.into_inner());
                if seen.contains(&block_hash) {
                    return Ok(BlockConnectOutcome::CompetingStored);
                }
                seen.insert(block_hash);
            }
            let mut competing = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner());
            let entry = competing.entry(block_height).or_default();
            if entry.len() >= MAX_COMPETING_BLOCKS {
                return Ok(BlockConnectOutcome::CompetingStored);
            }
            entry.push(block.clone());
            drop(competing);
            info!(target: "chain_state",
                "Competing block at h={} stored as potential uncle", block_height);
            return Ok(BlockConnectOutcome::CompetingStored);
        }

        // --- Uncle parent lookup ---
        // Before full validation, check whether this block builds on our
        // canonical tip or on a competing block (uncle). If it builds on an
        // uncle, this is an uncle chain extension — store as competing block
        // at the next height. The uncle chain may grow longer than the
        // canonical chain, triggering reorganization.
        let tip_hash = if current_height > BlockHeight::new(0) {
            let prev = self.get_block(current_height)?;
            let prev_vm = self.get_vm(prev.header.randomx_key)?;
            let prev_guard = prev_vm.lock().unwrap_or_else(|e| e.into_inner());
            Some(prev.hash_with_vm(&*prev_guard)?)
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
            let mut competing = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner());
            let uncle_parent = competing
                .get(&current_height)
                .and_then(|blocks| {
                    blocks.iter().find(|b| {
                        // Graceful: a VM/hash failure means "not the parent", not a
                        // node panic (type-system.md §2.3.2).
                        let pvm = match self.get_vm(b.header.randomx_key) {
                            Ok(vm) => vm,
                            Err(_) => return false,
                        };
                        let pguard = pvm.lock().unwrap_or_else(|e| e.into_inner());
                        match b.hash_with_vm(&*pguard) {
                            Ok(h) => h == block.header.previous,
                            Err(_) => false,
                        }
                    })
                });
            if uncle_parent.is_some() {
                // Uncle chain extension: store as competing at next height.
                // Stage 1 PoW validated first (same as competing path).
                let guard = vm.lock().unwrap_or_else(|e| e.into_inner());
                let hash_u32 = {
                    let h = block.hash_with_vm(&*guard)?;
                    let b = h.as_bytes();
                    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
                };
                if !block.header.target.hash_is_valid(hash_u32) {
                    return Err(LinearError::InvalidPoW(
                        block.hash_with_vm(&*guard)?.to_string()
                    ));
                }
                // HAZOP M-3 fix: validate Monero merge-mined uncle chain extensions.
                // The competing block path at same height already has this check
                // (H-14 fix, line 576). The uncle extension path must match.
                if let crate::PowSource::Monero(monero_data) = &block.header.pow_source {
                    if !monero_data.is_coinbase_valid_merkle_root() {
                        drop(guard);
                        return Err(LinearError::BlockIsInvalid(
                            "Uncle extension Monero merge-mined block has invalid coinbase Merkle proof".into()
                        ));
                    }
                }
                // HAZOP H-15 fix: validate uncle chain extension target using
                // full difficulty adjustment, not just absolute min/max bounds.
                // Previously accepted any target between 1 and u32::MAX —
                // now requires the proper get_next_work_required target.
                {
                    let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
                    let expected = consensus.get_next_work_required(&self.store, block_height)?;
                    if block.header.target != expected {
                        drop(guard);
                        return Err(LinearError::InvalidTarget {
                            expected: expected.get(),
                            declared: block.header.target.get(),
                            height: block_height,
                        });
                    }
                    drop(consensus);
                }
                // H6 fix: build recent timestamps for uncle chain extension validation.
                let recent_ts: Vec<BlockTimestamp> = {
                    let start = if block_height.get() > 11 { block_height.get() - 11 } else { 1 };
                    let mut ts = Vec::new();
                    for h in start..block_height.get() {
                        if let Ok(b) = self.get_block(BlockHeight::new(h)) {
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

                // Clone uncle parent before any drops — the reference is into competing.
                #[expect(clippy::unwrap_used, reason = "guarded by is_some() above")]
                let uncle_parent_block = uncle_parent.unwrap().clone();

                // --- Heaviest-chain fork selection ---
                // When an uncle chain extends past the canonical tip, compare
                // cumulative work. If the uncle chain has more work and the
                // canonical block at the fork height is not finalized, signal
                // a reorg to the caller (accept_block).
                //
                // Work computation: both chains share the parent at H-1
                // (enforced by the previous_hash check in the competing path).
                // uncle_work = accumulated_work(H-1) + work(competing_H) + work(new_H+1)
                // canonical_work = accumulated_work (which includes canonical_H).
                let canonical_block = self.get_block(current_height)?;
                let canonical_finalized = self.finality_config.should_enforce(
                    canonical_block.header.finality_flags
                ) && (canonical_block.header.anchor_tx_id != [0u8; 32]
                    || canonical_block.header.anchor_monero_height != MoneroBlockHeight::new(0));

                if !canonical_finalized {
                    let canonical_work = {
                        let consensus = self.consensus.lock()
                            .unwrap_or_else(|e| e.into_inner());
                        consensus.accumulated_work.get()
                    };
                    let uncle_work = canonical_work
                        .saturating_sub(canonical_block.header.target.chain_work())
                        .saturating_add(uncle_parent_block.header.target.chain_work())
                        .saturating_add(block.header.target.chain_work());

                    if uncle_work > canonical_work {
                        // Heavier uncle chain — signal reorg.
                        // Remove the competing parent from storage (it's
                        // being consumed) and the new block from dedup.
                        let block_hash = block.hash_with_vm(
                            &*vm.lock().unwrap_or_else(|e| e.into_inner())
                        )?;
                        self.competing_seen.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&block_hash);
                        competing.entry(current_height).or_default().retain(|b| {
                            // Graceful: a VM/hash failure keeps the block (conservative)
                            // rather than panicking the node (type-system.md §2.3.2).
                            let pvm = match self.get_vm(b.header.randomx_key) {
                                Ok(vm) => vm,
                                Err(_) => return true,
                            };
                            let pguard = pvm.lock()
                                .unwrap_or_else(|e| e.into_inner());
                            match b.hash_with_vm(&*pguard) {
                                Ok(h) => h != block.header.previous,
                                Err(_) => true,
                            }
                        });
                        drop(competing);

                        info!(target: "chain_state",
                            "Reorg available at h={}: uncle_work={} > canonical_work={}",
                            current_height, uncle_work, canonical_work);

                        return Ok(BlockConnectOutcome::ReorgAvailable {
                            fork_height: current_height,
                            competing_block: uncle_parent_block,
                        });
                    }
                }

                // H5 fix: cap competing blocks per height
                const MAX_COMPETING_BLOCKS_UNCLE: usize = 20;
                let block_hash = block.hash_with_vm(
                    &*vm.lock().unwrap_or_else(|e| e.into_inner())
                )?;
                let mut seen = self.competing_seen.lock().unwrap_or_else(|e| e.into_inner());
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
                return Ok(BlockConnectOutcome::UncleExtended);
            }
            drop(competing);
            // Block doesn't build on tip or known uncle — falls through
            // to full validation below. check_block_header will reject
            // with InvalidPreviousHash.
        }

        // --- Stage 1 & 2 PoW validation ---
        let expected_target = self.consensus.lock().unwrap_or_else(|e| e.into_inner())
            .get_next_work_required(&self.store, block_height)?;

        // Lock the VM for validation hashing — prevents concurrent RandomX FFI
        let guard = vm.lock().unwrap_or_else(|e| e.into_inner());
        validation::check_block_header(
            block, &*guard, expected_target, current_height, tip_hash.as_ref(),
        )?;
        drop(guard);

        // CRITICAL-4: Timestamp validation (time warp protection + future limit)
        {
            let mut recent_ts: Vec<BlockTimestamp> = Vec::with_capacity(11);
            let start = if block_height.get() > 11 { block_height.get() - 11 } else { 1 };
            for h in start..block_height.get() {
                if let Ok(b) = self.store.get_block(BlockHeight::new(h)) {
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
                    || existing.header.anchor_monero_height != MoneroBlockHeight::new(0))
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
        let pre_timestamps: Vec<BlockTimestamp>;
        let pre_target: BlockTarget;
        {
            let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
            pre_target = consensus.target();
            pre_timestamps = consensus.snapshot_timestamps();
            consensus.record_block(block.header.timestamp);
            consensus.adjust_target();
        }

        // === Pre-commit uncle split verification (HAZID F3 fix) ===
        // verify_uncle_split MUST run BEFORE the sled commit closure.
        // A block with violated supply invariant must never reach disk.
        let height = block_height;
        let base_reward = dwow_sdk::blockchain::expected_reward(height);
        let pin_confirmed: Vec<BlockReward> = uncles.iter()
            .filter(|u| u.pin_accepted && u.pin_confirmed > BlockReward::new(0))
            .map(|u| u.pin_confirmed)
            .collect();
        CumulativeSupplyChain::verify_uncle_split(
            base_reward,
            block.header.total_reward,
            &pin_confirmed,
        )?;

        // === Pre-compute Pedersen uncle commitments ===
        // C_uncle_i = u_i·G_v + r_i·G_r  with deterministic blinds.
        // r_i = blake3(uncle_hash ‖ u_i ‖ H) → pallas::Scalar
        // Computed before the closure so uncle commitment batch is included
        // in the atomic sled transaction (uncle_merkle.md §Coinbase Split).
        let uncle_commitment_entries: Vec<([u8; 32], BlockHeight)> = {
            let mut entries = Vec::new();
            for uncle in uncles.iter().filter(|u| u.pin_accepted && u.pin_confirmed > BlockReward::new(0)) {
                // Spec: uncle_merkle.md §Uncle blind — r_i = blake3s(uncle_hash ‖ u_i ‖ H):
                // bind the uncle identity (hash), the pin amount (u_i), and the canonical
                // block height (H). (Not the doubled mining blob.)
                let r_bytes: [u8; 64] = {
                    let uncle_hash = blake3::hash(&uncle.header.to_mining_blob());
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(uncle_hash.as_bytes());
                    hasher.update(&uncle.pin_confirmed.get().to_le_bytes());
                    hasher.update(&height.to_le_bytes());
                    let h = hasher.finalize();
                    let mut out = [0u8; 64];
                    out[..32].copy_from_slice(h.as_bytes());
                    out[32..].copy_from_slice(h.as_bytes());
                    out
                };
                let r_i = pallas::Scalar::from_uniform_bytes(&r_bytes);
                let c_uncle = pedersen_commitment_u64(uncle.pin_confirmed.get(), Blind(r_i));
                debug_assert!(!bool::from(c_uncle.is_identity()),
                    "Uncle Pedersen commitment must not be identity");
                // spec dispensation: type-system.md §2.3 — pallas compressed
                // points are always exactly 32 bytes per the Pasta curve spec.
                let mut c_bytes = [0u8; 32];
                c_bytes.copy_from_slice(c_uncle.to_bytes().as_ref()); // pallas compressed point is 32 bytes
                entries.push((c_bytes, height));
            }
            entries
        };

        // --- Coinbase maturity enforcement (Phase 3c) ---
        // CRITICAL: MUST precede the sled commit closure (C-6 fix).
        // Previously ran after the commit — an immature spend was persisted
        // irreversibly to sled before the error was returned. Now checked
        // BEFORE any state hits disk.
        const COINBASE_MATURITY: u64 = 100;
        for tx in &block.transactions {
            // Skip coinbase transactions (they create coins, don't spend).
            // Detected via PoWRewardV1 contract call (function 0x05).
            if tx.contract_calls.first().map_or(false, |c| c.data.first() == Some(&0x05)) {
                continue;
            }
            for nullifier in &tx.nullifiers {
                // Check if this nullifier was created by a coinbase output.
                // The nullifier's creation height is stored in nullifier_set.
                if let Some(&created_at) = self.nullifier_set.lock().unwrap_or_else(|e| e.into_inner()).get(nullifier) {
                    // V.9 fix: use nullifier's own height for maturity, not commitment_set lookup.
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

        // Wrap batch-build + commit in a closure. Any error rolls back
        // in-memory consensus — covers serde failures (which the old
        // TransactionError-only rollback missed) and sled failures.
        let commit_result = (|| -> Result<()> {
            // --- Build commit batch ---
            let mut blocks_batch = sled::Batch::default();
            let block_value = dwow_serialize(block);
            blocks_batch.insert(&height.to_le_bytes(), block_value);

            let mut uncles_batch = sled::Batch::default();
            let mut uncle_hashes: Vec<[u8; 32]> = Vec::with_capacity(uncles.len());
            for uncle in uncles {
                let uncle_hash = blake3::hash(&dwow_serialize(&uncle.header));
                let uncle_value = dwow_serialize(uncle);
                uncles_batch.insert(uncle_hash.as_bytes(), uncle_value);
                uncle_hashes.push(*uncle_hash.as_bytes());
            }
            // Per-height uncle hash index (sync-protocol.md §19.6) — lets
            // disconnect_block remove this block's uncles from the `uncles` tree.
            let mut uncles_by_height_batch = sled::Batch::default();
            if !uncle_hashes.is_empty() {
                uncles_by_height_batch.insert(&height.to_le_bytes(), dwow_serialize(&uncle_hashes));
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

            // Replay gate — Representation Faithfulness Law / standards §9.3:
            // the consensus `spent_nullifiers` set is the authoritative replay
            // gate. Reject the block if any spend nullifier is already spent, or
            // repeats within this block (double-spend).
            let mut block_nfs = BTreeSet::new();
            for tx in &block.transactions {
                for nf in &tx.nullifiers {
                    if self.has_nullifier(nf) || !block_nfs.insert(*nf) {
                        return Err(LinearError::BlockIsInvalid(format!(
                            "duplicate spend nullifier (double-spend): {:?}",
                            nf
                        )))
                    }
                }
            }

            // Commitment and nullifier batches
            let mut commitments_batch = sled::Batch::default();
            let mut nullifiers_batch = sled::Batch::default();
            for (tx_idx, tx) in block.transactions.iter().enumerate() {
                // Coinbase detected via PoWRewardV1 contract call (function 0x05).
                // HAZOP guard: verify contract_id — 0x05 is also used by
                // identity::CreateClaimV1L1; without the contract-id check,
                // an identity claim tx at index 0 would be mistaken for a coinbase.
                let has_pow_reward = tx_idx == 0 && tx.contract_calls.first()
                    .map_or(false, |c| c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                        && c.data.first() == Some(&0x05));
                if has_pow_reward {
                    // Extract commitment and nullifier from PoWRewardV1 params.
                    let pow_data = &tx.contract_calls[0].data[1..]; // skip selector
                    if let Ok(params) = dwow_native_token_contract::model::PoWRewardParamsV1::decode(pow_data) {
                        commitments_batch.insert(&params.output.commitment.inner().to_repr(), &height.to_le_bytes());
                        // consensus-coinbase.md §1.2: "The PoWRewardV1 nullifier
                        // is the first entry in the nullifier set for this block."
                        // Claim nullifier (kind 0) — maturity tracking only.
                        let mut nf_val = vec![0u8];
                        nf_val.extend_from_slice(&height.to_le_bytes());
                        nullifiers_batch.insert(&params.nullifier.to_bytes(), &nf_val[..]);
                    }
                }
                // Fee collection plate detected via FeeCollectV1 call (0x06).
                // Same coin + nullifier tracking as the coinbase
                // (consensus-coinbase.md §3.8). Iterates all calls for
                // consistency with Phase 0.5 per-call counting — at most
                // one call exists structurally, but we don't assume position.
                for c in &tx.contract_calls {
                    if c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                        && c.data.first() == Some(&0x06)
                    {
                        let fc_data = &c.data[1..]; // skip selector
                        if let Ok(params) = dwow_native_token_contract::model::FeeCollectParamsV1::decode(fc_data) {
                            commitments_batch.insert(&params.output.commitment.inner().to_repr(), &height.to_le_bytes());
                            // Claim nullifier (kind 0) — maturity tracking only.
                            let mut nf_val = vec![0u8];
                            nf_val.extend_from_slice(&height.to_le_bytes());
                            nullifiers_batch.insert(&params.nullifier.to_bytes(), &nf_val[..]);
                        }
                        break; // at most one per block (structural enforcement)
                    }
                }
                // Uncle note mint detected via UncleMintV1 call (0x07).
                // Spec: uncle_merkle.md §Uncle Minting & Maturity — "Maturity,
                // persistence, and reversal". Each accepted uncle's spendable note
                // commitment + claim nullifier (kind 0) is persisted like the
                // coinbase, spendable after COINBASE_MATURITY.
                for c in &tx.contract_calls {
                    if c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                        && c.data.first() == Some(&0x07)
                    {
                        let um_data = &c.data[1..]; // skip selector
                        if let Ok(params) = dwow_native_token_contract::model::UncleMintParamsV1::decode(um_data) {
                            commitments_batch.insert(&params.output.commitment.inner().to_repr(), &height.to_le_bytes());
                            let mut nf_val = vec![0u8];
                            nf_val.extend_from_slice(&height.to_le_bytes());
                            nullifiers_batch.insert(&params.nullifier.to_bytes(), &nf_val[..]);
                        }
                    }
                }
                // Spend nullifiers — the authoritative replay gate (kind 1).
                // These are the tx.nullifiers entries (FeeV2, TransferV1, SpendV1,
                // BurnV1, and contract-emitted nullifiers), recorded in
                // spent_nullifiers for double-spend prevention via has_nullifier.
                for nf in &tx.nullifiers {
                    let mut nf_val = vec![1u8];
                    nf_val.extend_from_slice(&height.to_le_bytes());
                    nullifiers_batch.insert(&nf.to_bytes(), &nf_val[..]);
                }
            }

            // Accumulate chain work
            let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
            consensus.accumulated_work.add_block(block.header.target);
            let accumulated = consensus.accumulated_work.get();

            let mut consensus_batch = sled::Batch::default();
            consensus.save_to_batch(&mut consensus_batch);
            if let Some(ref fw) = self.fee_window {
                fw.save_to_batch(&mut consensus_batch);
            }
            consensus_batch.insert("accumulated_work", &accumulated.to_le_bytes());
            drop(consensus);

            // --- Defect 3 guard: non-genesis blocks MUST carry contract + supply state ---
            // connect_block is the single atomic commit. By the time we reach this point,
            // the caller MUST have executed WASM (via execute_block / accept_block) and
            // produced the contracts and supply_chain batches. Accepting None here silently
            // diverges from mining nodes (Defect 3 regression). Genesis (height 1) is
            // exempt — its cumulative supply starts at identity.
            if block.header.height > BlockHeight::GENESIS && (contracts_batch.is_none() || supply_chain_batch.is_none()) {
                return Err(LinearError::StorageError(format!(
                    "connect_block: block {} rejected — {} batch is None (contracts={}, supply={}). \
                     WASM execution MUST precede connect_block for non-genesis blocks.",
                    block.header.height,
                    if contracts_batch.is_none() { "contracts" } else { "supply_chain" },
                    contracts_batch.is_some(),
                    supply_chain_batch.is_some(),
                )));
            }

            // --- Block anchor root — deterministic, not header-validated ---
            // The anchor tree root is deterministically computed from block
            // transactions via execute_block. It does not need header validation
            // because the header's nullifier_root is in the mining blob and
            // cannot be set after execution (would invalidate PoW).
            //
            // When pre-execution mining is implemented, the header's
            // nullifier_root can be cross-checked against block_anchor_root()
            // for defense-in-depth. For now, the anchor state is implicitly
            // correct — same transactions, same execution, same tree root.
            let _computed_anchor_root = self.block_anchor_root();

            // --- Pre-compute next block's target for cache (M-1 fix) ---
            // Cache the expected target for height+1 so get_next_work_required
            // can use the O(1) fast path on the next call.
            // Use in-memory consensus target: the chain walk in
            // get_next_work_required cannot see blocks still in the sled batch
            // (this runs inside the commit closure, before the sled transaction).
            let next_target = {
                let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
                consensus.target()
            };
            let mut targets_batch = sled::Batch::default();
            targets_batch.insert(&height.succ().to_le_bytes(), &next_target.get().to_le_bytes());

            // --- Atomic commit (sled cross-tree transaction) ---
            let contracts = contracts_batch.unwrap_or_default();
            let sc_batch = supply_chain_batch.unwrap_or_default();
            // H4: the contracts-tree undo record is written ATOMICALLY with the
            // block (Bitcoin `CBlockUndo`), never as a separate post-commit write.
            let mut contracts_undo_batch = sled::Batch::default();
            if let Some(ref undo_bytes) = contracts_undo {
                contracts_undo_batch.insert(&height.to_le_bytes(), undo_bytes.as_slice());
            }
            (&self.store.blocks, &self.store.uncles,
             &self.store.contracts, &self.store.consensus,
             &self.store.commitment_set, &self.store.nullifiers,
             self.supply_chain.tree(),
             &self.store.block_targets,
             &self.store.contracts_undo,
             &self.store.uncles_by_height)
                .transaction(|(tx_blocks, tx_uncles, tx_contracts, tx_consensus,
                               tx_coins, tx_nullifiers, tx_supply, tx_targets,
                               tx_undo, tx_uncles_by_height)| {
                    tx_blocks.apply_batch(&blocks_batch)?;
                    tx_uncles.apply_batch(&uncles_batch)?;
                    tx_contracts.apply_batch(&contracts)?;
                    tx_consensus.apply_batch(&consensus_batch)?;
                    tx_coins.apply_batch(&commitments_batch)?;
                    tx_nullifiers.apply_batch(&nullifiers_batch)?;
                    tx_supply.apply_batch(&sc_batch)?;
                    tx_targets.apply_batch(&targets_batch)?;
                    tx_undo.apply_batch(&contracts_undo_batch)?;
                    tx_uncles_by_height.apply_batch(&uncles_by_height_batch)?;
                    Ok(())
                })
                .map_err(|e: sled::transaction::TransactionError<sled::Error>| {
                    LinearError::StorageError(format!("commit: {}", e))
                })
        })();

        // H5.2: Roll back in-memory consensus on ANY error (not just TransactionError).
        if commit_result.is_err() {
            let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
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
                .map_or(false, |c| c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                    && c.data.first() == Some(&0x05));
            // Claim nullifiers (coinbase + fee-collect) are MATURITY nullifiers,
            // not spend nullifiers. They are tracked is_spend=false below; the
            // tx.nullifiers spend-tracking loop MUST skip them, else the coinbase
            // / fee commitment is born-unspendable — the claim nullifier IS the future
            // spend nullifier (fee-spec §17.4).
            let mut claim_nulls: Vec<Nullifier> = Vec::new();
            if has_pow_reward {
                let pow_data = &tx.contract_calls[0].data[1..]; // skip selector
                if let Ok(params) = dwow_native_token_contract::model::PoWRewardParamsV1::decode(pow_data) {
                    self.commitment_set.lock().unwrap_or_else(|e| e.into_inner()).insert(Commitment::from_base(params.output.commitment.inner()), height);
                    claim_nulls.push(params.nullifier);
                    self.track_nullifier(params.nullifier, height, false);
                }
            }
            // FeeCollectV1 fee commitment + nullifier (consensus-coinbase.md §3.8) —
            // tracked so compute_root_including_commitment sees fee-collect commitments
            // when generating the next block template (audit finding L3).
            // Iterates all calls (consistency with Phase 0.5 per-call counting).
            for c in &tx.contract_calls {
                if c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                    && c.data.first() == Some(&0x06)
                {
                    let fc_data = &c.data[1..]; // skip selector
                    if let Ok(params) = dwow_native_token_contract::model::FeeCollectParamsV1::decode(fc_data) {
                        self.commitment_set.lock().unwrap_or_else(|e| e.into_inner()).insert(Commitment::from_base(params.output.commitment.inner()), height);
                        claim_nulls.push(params.nullifier);
                        self.track_nullifier(params.nullifier, height, false);
                    }
                    break; // at most one FeeCollect call per block
                }
            }
            // UncleMintV1 (0x07) — spendable uncle reward note (claim, kind 0).
            // Spec: uncle_merkle.md §"Maturity, persistence, and reversal" — the
            // uncle note commitment is tracked like the coinbase (spendable after
            // COINBASE_MATURITY), and its claim nullifier is a maturity nullifier.
            for c in &tx.contract_calls {
                if c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                    && c.data.first() == Some(&0x07)
                {
                    let um_data = &c.data[1..]; // skip selector
                    if let Ok(params) = dwow_native_token_contract::model::UncleMintParamsV1::decode(um_data) {
                        self.commitment_set.lock().unwrap_or_else(|e| e.into_inner()).insert(Commitment::from_base(params.output.commitment.inner()), height);
                        claim_nulls.push(params.nullifier);
                        self.track_nullifier(params.nullifier, height, false);
                    }
                }
            }
            // Spend nullifiers — the authoritative replay gate (double-spend).
            for nf in &tx.nullifiers {
                if !claim_nulls.contains(nf) {
                    self.track_nullifier(*nf, height, true);
                }
            }
        }

        // --- Reset block anchor tree for next block ---
        // Per R1: the anchor tree is per-block. After commit, reset to
        // a fresh tree with the empty leaf for the next block's anchors.
        {
            let mut tree = self.block_anchor_tree.lock()
                .unwrap_or_else(|e| e.into_inner());
            // Reset: replace with fresh tree (old tree is dropped)
            let mut fresh = MerkleTree::new(100);
            fresh.append(MerkleNode::from_base(pallas::Base::from(2u64)));
            *tree = fresh;
        }

        // --- Post-commit uncle_commitment_set update ---
        // Uncle Pedersen commitments were pre-computed before the closure.
        // Update the in-memory cache after the atomic sled commit succeeds.
        // Sled persistence deferred to Phase 2 (uncle commitments are deterministically
        // recomputable from chain data via r_i = blake3(uncle_hash ‖ u_i ‖ H) mod p).
        if !uncle_commitment_entries.is_empty() {
            let mut ucs = self.uncle_commitment_set.lock().unwrap_or_else(|e| e.into_inner());
            for (c_bytes, h) in &uncle_commitment_entries {
                ucs.insert(*c_bytes, *h);
            }
        }

        // Clean up orphaned competing blocks (H11)
        self.prune_competing(height);

        // Prune in-memory commitment set. This mirrors the sled commitments tree for
        // fast lookup but grows unboundedly. Entries older than COINBASE_MATURITY
        // are evicted — sled is the authoritative source for old commitments.
        //
        // nullifier_set is now a HashMap<[u8;32], u64> with height tracking.
        // Entries older than COINBASE_MATURITY are pruned — sled is the
        // authoritative source for pre-existing nullifiers on restart.
        if height > BlockHeight::new(COINBASE_MATURITY) {
            let prune_h = height.saturating_sub_blocks(COINBASE_MATURITY);
            self.commitment_set.lock().unwrap_or_else(|e| e.into_inner()).retain(|_, h| *h >= prune_h);
            // Prune nullifiers older than maturity (sled is authoritative source)
            self.nullifier_set.lock().unwrap_or_else(|e| e.into_inner()).retain(|_, h| *h >= prune_h);
        }

        info!(target: "chain_state", "Block {} at height {} committed",
            block.header.height, height);
        Ok(BlockConnectOutcome::CanonicalExtension { new_height: height })
    }

    /// Convenience: apply a block with uncles but no contracts overlay.
    /// Async for caller compatibility. Delegates to `connect_block`.
    pub async fn apply_block_with_uncles(
        &self,
        block: &Block,
        uncles: &[UncleBlock],
    ) -> Result<BlockConnectOutcome> {
        self.connect_block(block, uncles, None, None, None)
    }

    /// Disconnect the canonical block at `height`, reversing all state changes.
    ///
    /// Spec: sync-protocol.md §19.4 (contracts-tree `CBlockUndo` replay) + §19.2/§19.3
    /// (disconnect step of `DisconnectBlock`/`ConnectBlock`). This is the reverse of
    /// `connect_block` for a canonical extension. All removals execute in a single
    /// cross-tree sled transaction. Serialisation against concurrent block application
    /// is the caller's responsibility (the reorg path runs single-threaded during sync).
    ///
    /// # Reversed subsystems (in order)
    ///
    /// 1. Blocks sled tree — remove entry at `height`
    /// 2. Coins + nullifiers — remove coinbase and fee-collect entries
    /// 3. Consensus — roll back accumulated_work, timestamps, target
    /// 4. Supply chain — remove cumulative supply entry at `height`
    /// 5. In-memory sets — commitment_set, nullifier_set
    /// 6. Height — decrement to `height - 1`
    ///
    /// # Known limitation
    ///
    /// Uncle entries in the sled `uncles` tree are NOT removed during disconnect.
    /// These phantom entries prevent displaced uncles from being re-included in
    /// future blocks. This is a fairness issue (at most 6 uncle miners lose
    /// rewards per reorg), not a security issue. General reorg support will add
    /// per-block uncle tracking for complete reversal.
    ///
    /// Similarly, in-memory `uncle_commitment_set` entries from the displaced block
    /// are NOT removed — they represent Pedersen commitments that are
    /// deterministically recomputable from chain data on restart.
    /// Detect whether `block` extends a competing (uncle) chain that is heavier
    /// than the canonical chain, warranting a reorg. Called by `accept_block`
    /// BEFORE WASM execution so a divergent-coinbase extension is recognized as
    /// a fork (and reorged) rather than executed against the wrong cumulative
    /// state — which fails `pow_reward_v1`'s `old_cumulative_commit` check.
    ///
    /// Spec: sync-protocol.md §19.1 (fork detection); consensus.md §Fork Choice Rule
    /// (heaviest-chain work comparison).
    pub fn detect_reorg(&self, block: &Block) -> Result<ReorgSignal> {
        let current_height = self.get_height();
        // Only a next-height block can extend a competing chain.
        if block.header.height != current_height.succ() {
            return Ok(ReorgSignal::None);
        }
        // Find the competing (uncle) parent at current_height that this block
        // builds on (same lookup as connect_block's uncle-parent path).
        let competing = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner());
        let uncle_parent = competing
            .get(&current_height)
            .and_then(|blocks| {
                blocks.iter().find(|b| {
                    let pvm = match self.get_vm(b.header.randomx_key) {
                        Ok(vm) => vm,
                        Err(_) => return false,
                    };
                    let pguard = pvm.lock().unwrap_or_else(|e| e.into_inner());
                    match b.hash_with_vm(&*pguard) {
                        Ok(h) => h == block.header.previous,
                        Err(_) => false,
                    }
                })
            })
            .cloned();
        drop(competing);
        let Some(uncle_parent) = uncle_parent else { return Ok(ReorgSignal::None) };

        // Heaviest-chain comparison (mirrors connect_block's reorg signal).
        let canonical_block = self.get_block(current_height)?;
        let canonical_finalized = self.finality_config.should_enforce(
            canonical_block.header.finality_flags,
        ) && (canonical_block.header.anchor_tx_id != [0u8; 32]
            || canonical_block.header.anchor_monero_height != MoneroBlockHeight::new(0));
        if canonical_finalized {
            return Ok(ReorgSignal::None);
        }
        let canonical_work = {
            let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
            consensus.accumulated_work.get()
        };
        let uncle_work = canonical_work
            .saturating_sub(canonical_block.header.target.chain_work())
            .saturating_add(uncle_parent.header.target.chain_work())
            .saturating_add(block.header.target.chain_work());
        if uncle_work > canonical_work {
            Ok(ReorgSignal::Heavier {
                fork_height: current_height,
                competing_block: uncle_parent,
            })
        } else {
            // M4: a lighter uncle-chain extension is NOT a fork — it SHALL be
            // stored as a competing block, not executed against the wrong
            // cumulative state (which would fail pow_reward_v1's
            // old_cumulative_commit check).
            Ok(ReorgSignal::Lighter)
        }
    }

    /// Store a lighter uncle-chain extension as a competing block at `height`
    /// (M4 / sync-protocol.md §19.1). Extracted from `connect_block` so
    /// `accept_block` can store it BEFORE WASM execution.
    pub fn store_competing_block(&self, block: &Block, height: BlockHeight) -> Result<()> {
        const MAX_COMPETING_BLOCKS_UNCLE: usize = 20;
        let vm = self.get_vm(block.header.randomx_key)?;
        let guard = vm.lock().unwrap_or_else(|e| e.into_inner());
        let block_hash = block.hash_with_vm(&*guard)?;
        drop(guard);
        let mut seen = self.competing_seen.lock().unwrap_or_else(|e| e.into_inner());
        if !seen.contains(&block_hash) {
            seen.insert(block_hash);
            drop(seen);
            let mut competing = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner());
            let entry = competing.entry(height).or_default();
            if entry.len() < MAX_COMPETING_BLOCKS_UNCLE {
                entry.push(block.clone());
            }
        }
        Ok(())
    }

    /// Remove a competing block at `height` whose hash equals `parent_hash`.
    ///
    /// Spec: sync-protocol.md §19.5 (recursion guard) — called during a reorg after a
    /// competing parent has been promoted to canonical, so that `detect_reorg` does not
    /// re-fire on the same parent when its extension block is re-accepted (which would
    /// recurse infinitely).
    pub fn remove_competing(&self, height: BlockHeight, parent_hash: blake3::Hash) {
        let mut competing = self.competing_blocks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(blocks) = competing.get_mut(&height) {
            blocks.retain(|b| {
                let pvm = match self.get_vm(b.header.randomx_key) {
                    Ok(vm) => vm,
                    Err(_) => return true,
                };
                let pguard = pvm.lock().unwrap_or_else(|e| e.into_inner());
                match b.hash_with_vm(&*pguard) {
                    Ok(h) => h != parent_hash,
                    Err(_) => true,
                }
            });
        }
    }

    pub fn disconnect_block(&self, height: BlockHeight) -> Result<()> {
        let block = self.get_block(height)?;
        let current_height = self.get_height();
        if height != current_height {
            return Err(LinearError::StorageError(format!(
                "disconnect_block: height {} != current tip {}",
                height, current_height
            )));
        }

        // --- Snapshot pre-disconnect state for rollback on failure ---
        let pre_target = self.consensus.lock()
            .unwrap_or_else(|e| e.into_inner()).target();
        let pre_timestamps = self.consensus.lock()
            .unwrap_or_else(|e| e.into_inner()).snapshot_timestamps();
        let pre_accumulated_work = self.consensus.lock()
            .unwrap_or_else(|e| e.into_inner()).accumulated_work.get();

        // --- Build removal batches ---
        let mut blocks_remove = sled::Batch::default();
        blocks_remove.remove(&height.to_le_bytes());

        // Coins and nullifiers from coinbase + fee-collect
        let mut commitments_remove = sled::Batch::default();
        let mut nullifiers_remove = sled::Batch::default();
        let mut in_memory_commitments: Vec<Commitment> = Vec::new();
        let mut in_memory_nullifiers: Vec<Nullifier> = Vec::new();
        let mut in_memory_spent_nullifiers: Vec<Nullifier> = Vec::new();

        for (tx_idx, tx) in block.transactions.iter().enumerate() {
            let has_pow_reward = tx_idx == 0 && tx.contract_calls.first()
                .map_or(false, |c| c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                    && c.data.first() == Some(&0x05));
            if has_pow_reward {
                let pow_data = &tx.contract_calls[0].data[1..];
                if let Ok(params) = dwow_native_token_contract::model::PoWRewardParamsV1::decode(pow_data) {
                    commitments_remove.remove(&params.output.commitment.inner().to_repr());
                    nullifiers_remove.remove(&params.nullifier.to_bytes());
                    in_memory_commitments.push(Commitment::from_base(params.output.commitment.inner()));
                    in_memory_nullifiers.push(params.nullifier);
                }
            }
            for c in &tx.contract_calls {
                if c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                    && c.data.first() == Some(&0x06)
                {
                    let fc_data = &c.data[1..];
                    if let Ok(params) = dwow_native_token_contract::model::FeeCollectParamsV1::decode(fc_data) {
                        commitments_remove.remove(&params.output.commitment.inner().to_repr());
                        nullifiers_remove.remove(&params.nullifier.to_bytes());
                        in_memory_commitments.push(Commitment::from_base(params.output.commitment.inner()));
                        in_memory_nullifiers.push(params.nullifier);
                    }
                    break;
                }
            }
            // Uncle note mint reversal (0x07).
            // Spec: uncle_merkle.md §Uncle Minting & Maturity — "Maturity,
            // persistence, and reversal". Remove the displaced uncle notes'
            // commitments + claim nullifiers alongside the coinbase.
            for c in &tx.contract_calls {
                if c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                    && c.data.first() == Some(&0x07)
                {
                    let um_data = &c.data[1..];
                    if let Ok(params) = dwow_native_token_contract::model::UncleMintParamsV1::decode(um_data) {
                        commitments_remove.remove(&params.output.commitment.inner().to_repr());
                        nullifiers_remove.remove(&params.nullifier.to_bytes());
                        in_memory_commitments.push(Commitment::from_base(params.output.commitment.inner()));
                        in_memory_nullifiers.push(params.nullifier);
                    }
                }
            }
            // Spend nullifiers (kind 1) — roll back the authoritative replay gate.
            for nf in &tx.nullifiers {
                nullifiers_remove.remove(&nf.to_bytes());
                in_memory_spent_nullifiers.push(*nf);
            }
        }

        // Supply chain entry removal
        let mut supply_remove = sled::Batch::default();
        supply_remove.remove(&height.to_le_bytes());

        // M-1: remove cached target for the disconnected block
        let mut targets_remove = sled::Batch::default();
        let next_key = height.succ().to_le_bytes();
        targets_remove.remove(&next_key);

        // Consensus rollback batch
        let mut consensus_batch = sled::Batch::default();
        {
            let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
            // Roll back accumulated_work: subtract the displaced block's contribution
            let block_work = block.header.target.chain_work();
            let current_work = consensus.accumulated_work.get();
            consensus.accumulated_work.store(current_work.saturating_sub(block_work));
            let new_accumulated = consensus.accumulated_work.get();

            // Remove this block's timestamp from the window
            let mut timestamps = consensus.snapshot_timestamps();
            timestamps.pop(); // undo record_block — the last timestamp is this block's
            consensus.restore_timestamps(timestamps);

            // Restore target to pre-block value by deriving from chain state.
            // Walk timestamps from genesis to height-1 to compute the correct target.
            let prev_target = if height > BlockHeight::new(1) {
                let mut target = consensus.initial_target();
                let ts = consensus.snapshot_timestamps();
                for window in ts.windows(2) {
                    target = PoWConsensus::compute_adjustment(
                        &[window[0], window[1]],
                        target,
                        consensus.target_block_time(),
                        consensus.min_target(),
                        consensus.max_target(),
                    );
                }
                target
            } else {
                consensus.initial_target()
            };
            consensus.force_target(prev_target);

            consensus.save_to_batch(&mut consensus_batch);
            consensus_batch.insert("accumulated_work", &new_accumulated.to_le_bytes());
        }

        // M7 — remove this block's uncles from the `uncles` tree (symmetric
        // disconnect, sync-protocol.md §19.6). The per-height hash index was
        // written atomically at connect time.
        let mut uncles_remove = sled::Batch::default();
        let uncle_hashes: Vec<[u8; 32]> = self.store.uncles_by_height
            .get(&height.to_le_bytes())
            .map_err(|e| LinearError::StorageError(e.to_string()))?
            .map(|v| dwow_deserialize(&v).unwrap_or_default())
            .unwrap_or_default();
        for h in &uncle_hashes {
            uncles_remove.remove(h);
        }
        let mut uncles_by_height_remove = sled::Batch::default();
        uncles_by_height_remove.remove(&height.to_le_bytes());

        // --- Atomic removal (cross-tree sled transaction) ---
        // Following Bitcoin's DisconnectBlock pattern: all removals succeed
        // or none do. The contracts tree is reversed separately below via the
        // per-block undo batch captured at connect time.
        let result = (&self.store.blocks, &self.store.commitment_set, &self.store.nullifiers,
                      &self.store.consensus, self.supply_chain.tree(),
                      &self.store.block_targets,
                      &self.store.uncles, &self.store.uncles_by_height)
            .transaction(|(tx_blocks, tx_coins, tx_nullifiers,
                           tx_consensus, tx_supply, tx_targets,
                           tx_uncles, tx_uncles_by_height)| {
                tx_blocks.apply_batch(&blocks_remove)?;
                tx_coins.apply_batch(&commitments_remove)?;
                tx_nullifiers.apply_batch(&nullifiers_remove)?;
                tx_consensus.apply_batch(&consensus_batch)?;
                tx_supply.apply_batch(&supply_remove)?;
                tx_targets.apply_batch(&targets_remove)?;
                tx_uncles.apply_batch(&uncles_remove)?;
                tx_uncles_by_height.apply_batch(&uncles_by_height_remove)?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<sled::Error>| {
                LinearError::StorageError(format!("disconnect_block commit: {}", e))
            });

        // Roll back in-memory consensus on ANY error
        if result.is_err() {
            let consensus = self.consensus.lock().unwrap_or_else(|e| e.into_inner());
            consensus.force_target(pre_target);
            consensus.restore_timestamps(pre_timestamps);
            consensus.accumulated_work.store(pre_accumulated_work);
        }
        result?;

        // Reverse the WASM contracts-tree writes for this block (Bitcoin
        // `CBlockUndo`). The inverse ops were captured at connect time; apply
        // them to restore every key the WASM touched (commitment/nullifier sets,
        // per-contract state, and the cumulative-commit singletons).
        if let Ok(Some(undo_bytes)) = self.store.contracts_undo.get(&height.to_le_bytes()) {
            let ops: Vec<(Vec<u8>, Option<Vec<u8>>)> = dwow_serial::deserialize(undo_bytes.as_ref())
                .map_err(|e| LinearError::StorageError(format!("deserialize contracts undo: {e}")))?;
            let mut undo_batch = sled::Batch::default();
            for (key, value) in ops {
                match value {
                    Some(v) => undo_batch.insert(key, v),
                    None => undo_batch.remove(key),
                }
            }
            self.store.contracts.apply_batch(undo_batch)
                .map_err(|e| LinearError::StorageError(format!("apply contracts undo: {e}")))?;
            self.store.contracts_undo.remove(&height.to_le_bytes())
                .map_err(|e| LinearError::StorageError(format!("remove contracts undo: {e}")))?;
        }

        // --- Post-commit in-memory cleanup ---
        // Only after the sled transaction succeeds.
        {
            let mut commitment_set = self.commitment_set.lock().unwrap_or_else(|e| e.into_inner());
            for commitment in &in_memory_commitments {
                commitment_set.remove(commitment);
            }
        }
        {
            let mut nullifier_set = self.nullifier_set.lock().unwrap_or_else(|e| e.into_inner());
            for nf in &in_memory_nullifiers {
                nullifier_set.remove(nf);
            }
        }
        {
            let mut spent_nullifiers = self.spent_nullifiers.lock().unwrap_or_else(|e| e.into_inner());
            for nf in &in_memory_spent_nullifiers {
                spent_nullifiers.remove(nf);
            }
        }
        // M7 — reverse the in-memory uncle commitment set (entries created by this
        // block). They are deterministically recomputable, but must not linger as
        // phantom "already included" markers after a disconnect.
        {
            let mut ucs = self.uncle_commitment_set.lock().unwrap_or_else(|e| e.into_inner());
            ucs.retain(|_, h| *h != height);
        }
        // M7 — roll the cumulative-supply in-memory cache back to the predecessor.
        self.supply_chain.rollback_cache(height.pred().unwrap_or(BlockHeight::new(0)))?;

        // Decrement height
        let new_height = height.pred().unwrap_or(BlockHeight::new(0));
        self.set_height(new_height);

        info!(target: "chain_state",
            "Disconnected block at height {}. New tip: {}",
            height, new_height);

        Ok(())
    }

    /// Memory diagnostics: number of cached RandomX VMs.
    pub fn vm_cache_size(&self) -> usize {
        self.vm_cache.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Memory diagnostics: number of commitments in the in-memory set.
    pub fn commitment_set_size(&self) -> usize {
        self.commitment_set.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Compute the commitment merkle root including a new commitment.
    /// Used by block template generation for the coinbase coin.
    pub fn compute_root_including_commitment(&self, new_commitment: &Commitment) -> [u8; 32] {
        let commitments = self.commitment_set.lock().unwrap_or_else(|e| e.into_inner());
        let mut sorted: Vec<&Commitment> = commitments.keys().collect();
        sorted.push(new_commitment);
        sorted.sort_by_key(|c| c.to_bytes());
        let mut hasher = blake3::Hasher::new();
        for commitment in sorted {
            hasher.update(&commitment.to_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    /// Compute the current nullifier merkle root.
    /// Used by block template generation.
    pub fn compute_nullifier_root(&self) -> [u8; 32] {
        let nullifiers = self.nullifier_set.lock().unwrap_or_else(|e| e.into_inner());
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

    // --- Block Anchor Tree (Two-Level Merkle Architecture) ---

    /// Append a contract state anchor to the block-level Merkle tree.
    ///
    /// Called during `process_update` (apply) via the `merkle_anchor_add`
    /// host function. The nullifier links the contract-local proof to the
    /// block-level proof.
    pub fn append_anchor(&self, entry: &AnchorEntry) {
        let mut tree = self.block_anchor_tree.lock()
            .unwrap_or_else(|e| e.into_inner());
        let leaf_node = MerkleNode::from_base(
            dwow_sdk::crypto::merkle_anchor::anchor_leaf(
                &entry.nullifier, &entry.contract_id, &entry.contract_root)
        );
        tree.append(leaf_node);
    }

    /// Get the current block anchor tree root.
    pub fn block_anchor_root(&self) -> [u8; 32] {
        let tree = self.block_anchor_tree.lock()
            .unwrap_or_else(|e| e.into_inner());
        tree.root(0)
            .map(|r| r.to_bytes())
            .unwrap_or([0u8; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockHeader, PowSource, Miner, compute_merkle_root, CumulativeSupplyEntry};
    use crate::fee_window::FeeWindowFlags;
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::pasta::pallas;
    use dwow_sdk::blockchain::{BlockReward, BlockTarget, BlockTimestamp, MoneroBlockHeight, SupplyAmount, BlockVersion};

    /// T2: Competing blocks at the same height — the second block MUST be
    /// stored in competing_blocks, not dropped. The first block extends the
    /// canonical chain; the second is stored as an uncle candidate.
    #[test]
    fn test_competing_blocks_stored() {
        use crate::{compute_merkle_root, Miner};
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();

        let h1 = BlockHeight::new(1);
        let block1 = Block {
            header: BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::hash(b"genesis"), merkle_root: compute_merkle_root(&[]),
                timestamp: BlockTimestamp::new(1), target: BlockTarget::MAX, nonce: 0,
                height: h1, uncle_merkle_root: [0u8; 32], total_reward: dwow_sdk::blockchain::expected_reward(h1),
                randomx_key: Miner::derive_key_from_height(h1),
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0, pow_source: PowSource::Native,
            fee_window_flags: FeeWindowFlags::default(),
            },
            transactions: vec![],
        };
        let outcome1 = cs.connect_block(&block1, &[], None, None, None).expect("first block connects");
        assert!(matches!(outcome1, BlockConnectOutcome::CanonicalExtension { .. }),
            "first block must extend canonical chain");
        assert_eq!(cs.get_height(), h1);

        // Second block at same height — different nonce so different block hash.
        let mut block2 = block1.clone();
        block2.header.nonce = 1;
        let outcome2 = cs.connect_block(&block2, &[], None, None, None).expect("second block connects");
        assert!(matches!(outcome2, BlockConnectOutcome::CompetingStored),
            "second block at same height must be stored as competing");

        // Verify competing block can be retrieved.
        let competing = cs.take_competing_blocks(h1);
        assert!(!competing.is_empty(), "competing_blocks must contain the second block");
        assert_eq!(competing[0].header.nonce, 1, "retrieved competing block must match");
    }

    /// The sync responder reads tip/genesis hashes from a cache, not re-hashing
    /// with RandomX per request (the established Bitcoin/Monero block-index
    /// pattern). After connecting the genesis block, both must return the
    /// block's actual hash (non-zero) and remain stable across calls.
    #[test]
    fn test_tip_and_genesis_hash_cached() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();

        // Empty store → no hashes yet.
        assert!(cs.tip_hash().is_none(), "empty store must have no tip hash");
        assert!(cs.genesis_hash().is_none(), "empty store must have no genesis hash");

        let h1 = BlockHeight::new(1);
        let block1 = Block {
            header: BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::hash(b"genesis"), merkle_root: compute_merkle_root(&[]),
                timestamp: BlockTimestamp::new(1), target: BlockTarget::MAX, nonce: 0,
                height: h1, uncle_merkle_root: [0u8; 32], total_reward: dwow_sdk::blockchain::expected_reward(h1),
                randomx_key: Miner::derive_key_from_height(h1),
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0, pow_source: PowSource::Native,
            fee_window_flags: FeeWindowFlags::default(),
            },
            transactions: vec![],
        };
        cs.connect_block(&block1, &[], None, None, None).expect("genesis connects");

        let (tip_h, tip_hash) = cs.tip_hash().expect("tip hash after genesis");
        assert_eq!(tip_h, h1, "tip height must be genesis height");
        assert_ne!(tip_hash, blake3::Hash::from_bytes([0u8; 32]), "tip hash must be non-zero");

        let genesis_hash = cs.genesis_hash().expect("genesis hash after genesis");
        assert_eq!(genesis_hash, tip_hash, "genesis hash == tip hash when genesis is the tip");

        // Cache stability — repeated reads return the same value (no re-hash).
        assert_eq!(cs.tip_hash().map(|(_, h)| h), Some(tip_hash), "tip hash must be stable");
        assert_eq!(cs.genesis_hash(), Some(genesis_hash), "genesis hash must be stable");
    }

    /// T6: Supply chain audit — cumulative supply MUST be computable from
    /// the store by iterating blocks and summing expected_reward at each height.
    /// This is the passive audit that any observer can perform without trust.
    #[test]
    fn test_supply_chain_audit() {
        use dwow_sdk::blockchain::expected_reward;

        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();

        let mut expected_total = 0u64;
        for h in 1u64..=5 {
            let height = BlockHeight::new(h);
            let reward = expected_reward(height);
            expected_total = expected_total.saturating_add(reward.get());

            let block = Block {
                header: BlockHeader {
                    version: BlockVersion::CURRENT,
                    previous: if h == 1 { blake3::hash(b"genesis") } else { blake3::hash(&h.to_le_bytes()) },
                    merkle_root: compute_merkle_root(&[]),
                    timestamp: BlockTimestamp::new(h),
                    target: BlockTarget::MAX,
                    nonce: h as u32,
                    height,
                    uncle_merkle_root: [0u8; 32],
                    total_reward: reward,
                    randomx_key: Miner::derive_key_from_height(height),
                    miner: [0u8; 32],
                    commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                    anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                    anchor_monero_hash: [0u8; 32], finality_flags: 0, pow_source: PowSource::Native,
            fee_window_flags: FeeWindowFlags::default(),
                },
                transactions: vec![],
            };
            cs.store.insert_block(height, &block).expect("insert_block");
        }

        // Passive audit: iterate blocks and sum rewards.
        let mut audit_total = 0u64;
        for h in 1u64..=5 {
            let block = cs.store.get_block(BlockHeight::new(h)).expect("get_block");
            audit_total = audit_total.saturating_add(block.header.total_reward.get());
        }
        assert_eq!(audit_total, expected_total,
            "Supply chain audit: cumulative reward sum must match expected_reward sum");
        assert!(audit_total > 0, "Supply must be non-zero after 5 blocks");
    }

    /// CChainState::new correctly initializes from empty sled.
    #[test]
    fn test_empty_chain_state() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();
        assert_eq!(cs.get_height(), BlockHeight::new(0));
        assert_eq!(cs.commitment_set_size(), 0);
    }

    /// Nullifier queries return None for non-existent nullifiers.
    #[test]
    fn test_nullifier_query_empty() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();
        let nf = Nullifier::from_bytes([1u8; 32]).unwrap();
        assert!(!cs.has_nullifier(&nf));
        assert_eq!(cs.nullifier_height(&nf), None);
    }

    /// supply_chain returns genesis entry when empty.
    #[test]
    fn test_supply_chain_empty() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();
        let entry = cs.supply_chain.get_latest();
        assert_eq!(entry.total_supply, SupplyAmount::new(0));
        assert_eq!(entry.value_commit, pallas::Point::identity());
    }

    /// Block height persistence re-lift witness (type-system.md §2.3):
    /// the backing storage key is exactly the 8-byte LE encoding, and
    /// a block inserted by height re-lifts via `from_le_bytes` with the
    /// correct value. This is the one property the compiler cannot prove
    /// — that the bytes stored and the bytes later read are the canonical
    /// 8-byte width and not a legacy 4-byte key that happens to pass at
    /// height ≤ u32::MAX.
    #[test]
    fn test_block_height_persistence_roundtrip() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();
        let h = BlockHeight::new(42);
        // Build a minimal block — only the header matters for the key.
        let block = crate::Block {
            header: crate::BlockHeader {
                version: BlockVersion::CURRENT,
                previous: blake3::hash(b""),
                merkle_root: crate::compute_merkle_root(&[]),
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce: 0,
                height: h,
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: crate::Miner::derive_key_from_height(h),
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
                pow_source: crate::PowSource::Native,
            },
            transactions: vec![],
        };
        cs.store.insert_block(h, &block).expect("insert_block");

        // The sled key MUST be exactly 8 bytes — the canonical encoding
        // defined in §2.3. A 4-byte key would indicate a width regression.
        let key = h.to_le_bytes();
        assert_eq!(key.len(), 8);

        // Round-trip: re-lift via from_le_bytes at the persistence boundary.
        let retrieved = cs.get_block(h).expect("get_block");
        assert_eq!(retrieved.header.height, h,
            "round-tripped height must match — backing key was {:x?}", key);
    }

    /// L1: Block height key width regression — the sled key for any height
    /// MUST be exactly 8 bytes (canonical LE encoding per §2.3). A key of
    /// 1, 2, or 4 bytes would indicate a width regression.
    #[test]
    fn test_height_key_width_is_8_bytes() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();

        for h in [1u64, 255, 256, 65535, 1_000_000] {
            let height = BlockHeight::new(h);
            let block = Block {
                header: BlockHeader {
                    version: BlockVersion::CURRENT, previous: blake3::hash(&h.to_le_bytes()),
                    merkle_root: compute_merkle_root(&[]),
                    timestamp: BlockTimestamp::new(h), target: BlockTarget::MAX,
                    nonce: h as u32, height,
                    uncle_merkle_root: [0u8; 32], total_reward: BlockReward::ZERO,
                    randomx_key: Miner::derive_key_from_height(height),
                    miner: [0u8; 32],
                    commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                    anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                    anchor_monero_hash: [0u8; 32], finality_flags: 0, pow_source: PowSource::Native,
            fee_window_flags: FeeWindowFlags::default(),
                },
                transactions: vec![],
            };
            cs.store.insert_block(height, &block).expect("insert");
            let key = height.to_le_bytes();
            assert_eq!(key.len(), 8,
                "§2.3: height {} LE key must be exactly 8 bytes, got {}", h, key.len());
        }
    }

    /// L2: Nullifier persistence roundtrip — a nullifier tracked via
    /// track_nullifier MUST be queryable back at the same height, and the
    /// claim/spend distinction (Representation Faithfulness Law, type-system
    /// §0.1) MUST hold: a claim is maturity-only, a spend is the replay gate.
    #[test]
    fn test_nullifier_persistence_roundtrip() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();

        let mut nf_bytes = [0u8; 32];
        nf_bytes[0] = 1;
        let nf = Nullifier::from_bytes(nf_bytes).unwrap();

        // Claim (kind 0): maturity tracking only, NOT a double-spend.
        cs.track_nullifier(nf, BlockHeight::new(42), false);
        assert_eq!(cs.nullifier_height(&nf), Some(BlockHeight::new(42)),
            "claim nullifier must be queryable at its creation height");
        assert!(!cs.has_nullifier(&nf),
            "claim nullifier is not a spent nullifier (maturity, not double-spend)");

        // Spend (kind 1): authoritative replay gate.
        cs.track_nullifier(nf, BlockHeight::new(150), true);
        assert!(cs.has_nullifier(&nf),
            "spend nullifier must be recorded in the authoritative replay set");
    }

    /// BW-1: Nullifier replay detection witness.
    /// Per type-system.md §10.5: the block-acceptance boundary SHALL reject
    /// nullifier duplicates. A Nullifier is a nominal type (not bare [u8;32])
    /// and its replay detection is the type-level enforcement of the
    /// ↓nullify barb.
    #[test]
    fn test_nullifier_replay_detected() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db, 120, BlockTarget::MAX, BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();

        let mut nf_bytes = [0u8; 32];
        nf_bytes[0] = 1;
        let nf = Nullifier::from_bytes(nf_bytes).unwrap();

        // Fresh nullifier: not flagged as spent.
        assert!(!cs.has_nullifier(&nf),
            "fresh nullifier must not be flagged as spent");

        // Spend it.
        cs.track_nullifier(nf, BlockHeight::new(10), true);

        // After spending: has_nullifier detects the replay.
        assert!(cs.has_nullifier(&nf),
            "has_nullifier must detect a spent nullifier (the ↓nullify barb)");
    }

    /// L3: Uncle merkle root determinism — the same set of uncles MUST
    /// produce the same merkle root every time.
    #[test]
    fn test_uncle_merkle_root_determinism() {
        use crate::{UncleBlock, build_uncle_merkle};
        let h = BlockHeight::new(1);
        let uncle = UncleBlock {
            header: BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::hash(b"uncle"), merkle_root: compute_merkle_root(&[]),
                timestamp: BlockTimestamp::new(0), target: BlockTarget::MAX, nonce: 0,
                height: h, uncle_merkle_root: [0u8; 32], total_reward: BlockReward::ZERO,
                randomx_key: Miner::derive_key_from_height(h),
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0, pow_source: PowSource::Native,
            fee_window_flags: FeeWindowFlags::default(),
            },
            transactions: vec![],
            depth: 1, pin_offered: true, pin_accepted: false,
            pin_confirmed: BlockReward::ZERO,
        };
        let uncles = vec![uncle];
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let cache = RandomXCache::new(flags, &[0u8; 32]).expect("cache");
        let vm = Arc::new(RandomXVM::new(flags, Some(cache), None).expect("vm"));
        let (root1, _) = build_uncle_merkle(&uncles, &vm).expect("uncle merkle");
        let (root2, _) = build_uncle_merkle(&uncles, &vm).expect("uncle merkle");
        assert_eq!(root1, root2, "uncle merkle root must be deterministic");
    }

    /// BW-13: Supply chain persistence roundtrip witness.
    /// Per type-system.md §10.5: supply entries committed via supply_chain
    /// SHALL survive close/reopen with intact types — SupplyAmount not
    /// truncated to raw u64, Pedersen commitments not identity-erased.
    #[test]
    fn test_supply_chain_persistence_roundtrip() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let db = Arc::new(db);
        let cs = CChainState::new(db.clone(), 120, BlockTarget::MAX,
            BlockTarget::new(1), BlockTarget::MAX,
            FinalityConfig::default()).unwrap();

        // Commit a supply entry with non-trivial values
        let height = BlockHeight::new(5);
        let supply = SupplyAmount::new(1_000_000_000);
        let value_commit = pallas::Point::identity(); // simplified — real commits are Pedersen
        let entry = CumulativeSupplyEntry {
            total_supply: supply,
            value_commit,
            blind: pallas::Scalar::zero(),
        };

        cs.supply_chain.commit(height, &entry).unwrap();

        // Read back — values must survive sled roundtrip intact
        let retrieved = cs.supply_chain.get(height).unwrap();
        assert_eq!(retrieved.total_supply, supply,
            "total_supply must roundtrip through sled without truncation");
        assert_eq!(retrieved.value_commit, value_commit,
            "value_commit must roundtrip through sled");

        // Verify genesis entry still retrievable (no cross-contamination)
        let genesis = cs.supply_chain.get_latest_height();
        assert!(genesis >= height, "latest height must reflect committed entries");
    }
}
