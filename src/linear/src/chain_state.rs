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
use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc, Mutex};

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
const IBD_STALE_TIP_SECS: u64 = 24 * 3600;

/// How many blocks behind peers triggers IBD re-entry.
const IBD_PEER_GAP_BLOCKS: u64 = 10;

/// How old the tip must be (seconds) for the peer-gap IBD rule to apply.
const IBD_PEER_GAP_MIN_AGE: u64 = 2 * 3600;

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

    // --- IBD tracking ---
    /// Best height reported by any peer
    peer_best_height: AtomicU64,
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

        Ok(Arc::new(Self {
            store,
            consensus: Mutex::new(consensus),
            finality_config,
            height: AtomicU64::new(height),
            vm_cache: Mutex::new(vm_cache),
            coin_set: Mutex::new(HashMap::new()),
            nullifier_set: Mutex::new(HashSet::new()),
            peer_best_height: AtomicU64::new(0),
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

    // --- IBD: Derived sync state (replaces sync_complete AtomicBool) ---

    /// Set the best height reported by any peer.
    pub fn set_peer_best_height(&self, h: u64) {
        self.peer_best_height.store(h, Ordering::Relaxed);
    }

    /// Returns `true` while the node is still catching up with the network.
    /// Derived from tip age and peer height comparison — never a latched flag.
    /// Re-evaluated on every miner/stratum/mm_rpc check.
    pub fn is_initial_block_download(&self) -> bool {
        let height = self.get_height();
        if height == 0 {
            return true;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let tip_age = match self.store.get_block(height) {
            Ok(b) => now.saturating_sub(b.header.timestamp),
            Err(_) => return true,
        };

        // Rule 1: tip more than 24 hours old → IBD
        if tip_age >= IBD_STALE_TIP_SECS {
            return true;
        }

        // Rule 2: significantly behind peers with stale tip
        let peer_height = self.peer_best_height.load(Ordering::Relaxed);
        if peer_height.saturating_sub(height) >= IBD_PEER_GAP_BLOCKS
            && tip_age >= IBD_PEER_GAP_MIN_AGE
        {
            return true;
        }

        false
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
        let vm = self.get_vm(block.header.randomx_key);
        let current_height = self.get_height();

        // --- Stage 1 & 2 PoW validation ---
        let expected_target = {
            self.consensus.lock().unwrap()
                .get_next_work_required(block.header.height)
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
        if let Ok(existing) = self.store.get_block(block.header.height) {
            if self.finality_config.should_enforce(existing.header.finality_flags)
                && (existing.header.anchor_tx_id != [0u8; 32]
                    || existing.header.anchor_monero_height != 0)
            {
                return Err(LinearError::AnchoredBlockConflict);
            }
        }

        // --- Build commit batch ---
        let height = block.header.height;
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

        let mut consensus_batch = sled::Batch::default();
        self.consensus.lock().unwrap().save_to_batch(&mut consensus_batch);

        // --- Atomic commit (sled cross-tree transaction) ---
        let contracts = contracts_batch.unwrap_or_default();
        (self.store.blocks_tree(), self.store.uncles_tree(),
         self.store.contracts_tree(), self.store.consensus_tree())
            .transaction(|(tx_blocks, tx_uncles, tx_contracts, tx_consensus)| {
                tx_blocks.apply_batch(&blocks_batch)?;
                tx_uncles.apply_batch(&uncles_batch)?;
                tx_contracts.apply_batch(&contracts)?;
                tx_consensus.apply_batch(&consensus_batch)?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<sled::Error>| {
                LinearError::StorageError(format!("commit: {}", e))
            })?;

        // --- In-memory state (only after commit succeeds) ---
        if height > current_height {
            self.set_height(height);
        }

        // Collect coin commitments from coinbase transactions
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
}
