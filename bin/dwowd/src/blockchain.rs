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

//! Linear blockchain for localnet
//!
//! This module provides a LinearBlockchain that combines dwow_chain's
//! LinearStore with dwow's Runtime and ZK verification for contract execution.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use smol::lock::Mutex as SmolMutex;

use randomx::{RandomXFlags, RandomXVM};
use dwow_core::runtime::vm_runtime::RuntimeBackend;
use dwow_core::Error;
use dwow_core::Result;
use dwow_chain::{build_uncle_merkle, FinalityConfig, UncleBlock, Block, LinearStore, PoWConsensus};
use sled::Transactional;
use sled_overlay::{SledTreeOverlay, SledTreeOverlayStateDiff};
use dwow_sdk::crypto::{ContractId, DEPLOYOOOR_CONTRACT_ID};
use dwow_sdk::deploy::DeployParamsV1;
use dwow_serial::Decodable;
use tracing::{error, info, warn};

use crate::zk::ZkVerifier;

/// Maximum gas per block (250× per-call GAS_LIMIT of 400M).
/// Blocks exceeding this are rejected during validation. Template generation
/// should stop pulling from the mempool when cumulative gas approaches this.
pub const BLOCK_GAS_LIMIT: u64 = 100_000_000_000;

/// Number of blocks before a coinbase reward becomes spendable.
/// Gives fork resolution time to include competing blocks as uncles
/// before rewards can be moved. Matches Bitcoin's COINBASE_MATURITY.
pub const COINBASE_MATURITY: u64 = 100;

/// A [`RuntimeBackend`] that buffers contract state writes into a
/// [`SledTreeOverlay`] instead of writing directly to sled.
///
/// State mutations are staged in-memory. On block success the overlay is
/// aggregate()'d into a sled::Batch and applied atomically to the contracts
/// tree. On failure the overlay is dropped — nothing reaches sled.
///
/// Chain queries (block height, timestamps, tx lookups) bypass the overlay
/// and read directly from the store. This struct is intentionally minimal —
/// only `store`, `height`, and `vm` are needed. The `LinearBlockchain` monolith
/// (consensus, coin_set, nullifier_set, etc.) is NOT passed to threads.
pub struct TxBackend {
    pub overlay: Mutex<SledTreeOverlay>,
    pub store: Arc<LinearStore>,
    pub height: u64,
    pub vm: Arc<RandomXVM>,
}

impl TxBackend {
    fn composite_key(tree: &[u8], key: &[u8]) -> Vec<u8> {
        let mut ck = Vec::with_capacity(tree.len() + key.len());
        ck.extend_from_slice(tree);
        ck.extend_from_slice(key);
        ck
    }
}

impl RuntimeBackend for TxBackend {
    fn contract_lookup(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        let ov = self.overlay.lock().unwrap();
        match ov.get(handle_str.as_bytes()) {
            Ok(Some(iv)) if !iv.is_empty() => return Ok(handle),
            Ok(Some(_)) => return Err(Error::ContractStateNotFound),
            Ok(None) => {}
            Err(e) => return Err(Error::Custom(e.to_string())),
        }
        drop(ov);
        let data = self.store.get_contract_data(handle_str.as_bytes())
            .map_err(|e| Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(Error::ContractStateNotFound)
        }
        Ok(handle)
    }

    fn contract_init(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        self.overlay.lock().unwrap()
            .insert(handle_str.as_bytes(), tree_name.as_bytes())
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(handle)
    }

    fn contract_insert_bincode(&self, cid: ContractId, bincode: &[u8]) -> Result<()> {
        self.overlay.lock().unwrap()
            .insert(&cid.to_bytes(), bincode)
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn contract_get_bincode(&self, cid: &ContractId) -> Result<Vec<u8>> {
        let ov = self.overlay.lock().unwrap();
        if let Ok(Some(iv)) = ov.get(&cid.to_bytes()) {
            if iv.is_empty() {
                return Err(Error::ContractStateNotFound);
            }
            return Ok(iv.to_vec());
        }
        drop(ov);
        let data = self.store.get_contract_data(&cid.to_bytes())
            .map_err(|e| Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(Error::ContractStateNotFound)
        }
        Ok(data)
    }

    fn db_insert(&self, tree: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        let ck = Self::composite_key(tree, key);
        self.overlay.lock().unwrap()
            .insert(&ck, value)
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn db_get(&self, tree: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>> {
        let ck = Self::composite_key(tree, key);
        let ov = self.overlay.lock().unwrap();
        match ov.get(&ck) {
            Ok(Some(iv)) => {
                if iv.is_empty() { return Ok(None); }
                return Ok(Some(iv.to_vec()));
            }
            Ok(None) => {}
            Err(e) => return Err(Error::Custom(e.to_string())),
        }
        drop(ov);
        let data = self.store.get_contract_data(&ck)
            .map_err(|e| Error::Custom(e.to_string()))?;
        if data.is_empty() { Ok(None) } else { Ok(Some(data)) }
    }

    fn db_remove(&self, tree: &[u8], key: &[u8]) -> Result<()> {
        let ck = Self::composite_key(tree, key);
        self.overlay.lock().unwrap()
            .insert(&ck, &[])
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn db_contains_key(&self, tree: &[u8], key: &[u8]) -> Result<bool> {
        let ck = Self::composite_key(tree, key);
        let ov = self.overlay.lock().unwrap();
        match ov.get(&ck) {
            Ok(Some(iv)) => return Ok(!iv.is_empty()),
            Ok(None) => {}
            Err(e) => return Err(Error::Custom(e.to_string())),
        }
        drop(ov);
        let data = self.store.get_contract_data(&ck)
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(!data.is_empty())
    }

    fn last_block_timestamp(&self) -> Result<Vec<u8>> {
        if self.height == 0 {
            return Ok(0u64.to_le_bytes().to_vec())
        }
        let block = self.store.get_block(self.height).map_err(|e| Error::Custom(e.to_string()))?;
        Ok(block.header.timestamp.to_le_bytes().to_vec())
    }

    fn last_block_height(&self) -> Result<u32> {
        Ok(self.height as u32)
    }

    fn get_tx(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        match self.store.get_transaction(hash) {
            Ok(tx) => {
                let data = serde_json::to_vec(&tx).map_err(|e| Error::Custom(e.to_string()))?;
                Ok(Some(data))
            }
            Err(e) => {
                if e.to_string().contains("TransactionNotFound") {
                    Ok(None)
                } else {
                    Err(Error::Custom(e.to_string()))
                }
            }
        }
    }

    fn get_tx_location(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        for h in 1..=self.height {
            if let Ok(block) = self.store.get_block(h) {
                for tx in &block.transactions {
                    if tx.hash().as_bytes() == hash {
                        return Ok(Some((h as u32).to_le_bytes().to_vec()))
                    }
                }
            }
        }
        Ok(None)
    }

    fn get_block_hash_by_height(&self, height: u32) -> Result<Option<Vec<u8>>> {
        match self.store.get_block(height as u64) {
            Ok(block) => Ok(Some(block.hash_with_vm(&self.vm).as_bytes().to_vec())),
            Err(e) => {
                if e.to_string().contains("BlockNotFound") {
                    Ok(None)
                } else {
                    Err(Error::Custom(e.to_string()))
                }
            }
        }
    }
}

/// Configuration for LinearBlockchain PoW
pub struct LinearPoWConfig {
    /// Target block time in seconds
    pub target_block_time: u64,
    /// Initial difficulty
    pub initial_target: u32,
    /// Minimum difficulty
    pub min_target: u32,
    /// Maximum difficulty
    pub max_target: u32,
}

impl Default for LinearPoWConfig {
    fn default() -> Self {
        Self {
            target_block_time: 60,
            initial_target: 0x0FFFFFFF,
            min_target: 1,
            max_target: u32::MAX,
        }
    }
}

/// Linear blockchain with WASM runtime and ZK verification
pub struct LinearBlockchain {
    /// Storage backend (dwow_chain)
    pub store: Arc<LinearStore>,
    /// PoW consensus (protected by mutex for difficulty updates)
    pub consensus: Mutex<PoWConsensus>,
    /// ZK verifier
    pub zk_verifier: ZkVerifier,
    /// Finality configuration
    pub finality_config: FinalityConfig,
    /// Current chain height
    height: AtomicU64,
    /// RandomX VM for PoW (protected by mutex for interior mutability)
    vm: Mutex<Arc<RandomXVM>>,
    /// Current RandomX key (protected by mutex for interior mutability)
    randomx_key: Mutex<[u8; 32]>,
    /// Map of coin commitments → block height for double-mint prevention
    /// and coinbase maturity enforcement. Height tracks when the coin was
    /// created so the maturity spend lock can be checked.
    coin_set: Mutex<HashMap<[u8; 32], u64>>,
    /// Set of nullifiers for double-spend prevention
    nullifier_set: Mutex<HashSet<[u8; 32]>>,
    /// Serializes block application so only one apply_block_with_uncles
    /// runs at a time across all callers (sync, broadcast, stratum, MM, miner).
    apply_lock: SmolMutex<()>,
}

impl LinearBlockchain {
    /// Create a new LinearBlockchain with the given sled database and default PoW
    pub fn new(store: Arc<LinearStore>) -> Self {
        Self::with_pow_config(store, LinearPoWConfig::default(), FinalityConfig::default())
    }

    /// Create a new LinearBlockchain with custom PoW and finality configuration
    pub fn with_pow_config(store: Arc<LinearStore>, config: LinearPoWConfig, finality_config: FinalityConfig) -> Self {
        let consensus = PoWConsensus::new(
            config.target_block_time,
            config.initial_target,
            config.min_target,
            config.max_target,
        );
        // Restore persisted consensus state (target + timestamps) if available
        consensus.load(store.consensus_tree()).ok();
        let zk_verifier = ZkVerifier;
        let height = AtomicU64::new(store.get_height().unwrap_or(0));

        // Initialize RandomX VM with default key
        let randomx_key = [0u8; 32];
        let vm = Self::create_vm(&randomx_key).expect("Failed to create RandomX VM");

        let blockchain = Self {
            store,
            consensus: Mutex::new(consensus),
            zk_verifier,
            finality_config,
            height,
            vm: Mutex::new(vm),
            randomx_key: Mutex::new(randomx_key),
            coin_set: Mutex::new(HashMap::new()),
            nullifier_set: Mutex::new(HashSet::new()),
            apply_lock: SmolMutex::new(()),
        };
        blockchain.rehydrate_sets();
        blockchain
    }

    /// Create a new RandomX VM with the given key
    fn create_vm(key: &[u8; 32]) -> Result<Arc<RandomXVM>> {
        // Use recommended flags but disable JIT to avoid SIGILL from
        // -DARCH=native misdetecting CPU features in the JIT compiler.
        // HARD_AES, ARGON2_AVX2 etc. are safe — they use CPU intrinsics.
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let cache = randomx::RandomXCache::new(flags, key)
            .map_err(|e| Error::Custom(format!("RandomX cache error: {}", e)))?;
        let vm = RandomXVM::new(flags, Some(cache), None)
            .map_err(|e| Error::Custom(format!("RandomX VM error: {}", e)))?;
        Ok(Arc::new(vm))
    }

    /// Get VM for the given key, creating if necessary
    pub fn get_vm(&self, key: [u8; 32]) -> Arc<RandomXVM> {
        let mut randomx_key = self.randomx_key.lock().unwrap();
        let mut vm = self.vm.lock().unwrap();
        if key != *randomx_key {
            *randomx_key = key;
            *vm = Self::create_vm(&key).expect("Failed to create RandomX VM");
        }
        vm.clone()
    }

    /// Get current chain height
    pub fn get_height(&self) -> u64 {
        self.height.load(Ordering::SeqCst)
    }

    /// Get current RandomX key
    pub fn get_randomx_key(&self) -> [u8; 32] {
        *self.randomx_key.lock().unwrap()
    }

    /// Get a block by height
    pub fn get_block(&self, height: u64) -> Result<Block> {
        self.store.get_block(height).map_err(|e| Error::Custom(e.to_string()))
    }

    /// Get the latest block
    pub fn get_latest_block(&self) -> Result<Block> {
        let height = self.height.load(Ordering::SeqCst);
        self.store.get_block(height).map_err(|e| Error::Custom(e.to_string()))
    }

    /// Rebuild coin_set and nullifier_set from all stored blocks.
    /// Best-effort: log errors but don't fail construction.
    pub fn rehydrate_sets(&self) {
        let height = self.height.load(Ordering::SeqCst);
        let mut coin_set = self.coin_set.lock().unwrap();
        for h in 1..=height {
            if let Ok(block) = self.store.get_block(h) {
                for tx in &block.transactions {
                    if let Some(ref coinbase) = tx.coinbase {
                        coin_set.insert(coinbase.coin, h);
                    }
                }
            }
        }
        if !coin_set.is_empty() {
            info!(target: "linear_blockchain", "Rehydrated {} coins from {} blocks", coin_set.len(), height);
        }
    }

    /// Insert a validated block into the chain. Callers must verify PoW,
    /// merkle roots, and consensus rules before calling this method.
    /// Used for genesis block insertion during init.
    pub fn insert_validated_block(&self, block: &Block) -> Result<()> {
        let (height, _block_bytes, _consensus_batch, new_coins) =
            self.validate_and_record_block(block)?;
        // Persist to sled (genesis path — uses simple writes, no transaction needed)
        self.store.insert_block(block.header.height, block)
            .map_err(|e| Error::Custom(e.to_string()))?;
        let _ = self.consensus.lock().unwrap().save(self.store.consensus_tree());

        // In-memory state — only after sled writes succeed
        if height > self.height.load(Ordering::SeqCst) {
            self.height.store(height, Ordering::SeqCst);
        }
        {
            let mut coin_set = self.coin_set.lock().unwrap();
            for coin in &new_coins {
                coin_set.insert(*coin, height);
            }
        }
        {
            let consensus = self.consensus.lock().unwrap();
            consensus.record_block(block.header.timestamp);
            consensus.adjust_target();
        }
        Ok(())
    }

    /// Validate block metadata and build a consensus batch WITHOUT mutating
    /// any in-memory state. State mutations happen only after the sled
    /// transaction succeeds.
    ///
    /// Returns (height, block_bytes, consensus_batch, new_coins).
    /// Validate block metadata and build a consensus batch for the atomic
    /// sled transaction. Does NOT mutate any in-memory state — that happens
    /// only after the sled commit succeeds (see [`apply_block_with_uncles`]).
    fn validate_and_record_block(&self, block: &Block) -> Result<(u64, Vec<u8>, sled::Batch, Vec<[u8; 32]>)> {
        let height = block.header.height;

        // Finality check: mode-aware anchor enforcement
        if let Ok(existing) = self.store.get_block(height) {
            if self.finality_config.should_enforce(existing.header.finality_flags)
                && (existing.header.anchor_tx_id != [0u8; 32]
                    || existing.header.anchor_monero_height != 0)
            {
                info!(
                    target: "linear_blockchain",
                    "Rejected block at height {} — existing block is anchored (tx_id: {:?})",
                    height, existing.header.anchor_tx_id
                );
                return Err(Error::Custom("AnchoredBlockConflict".to_string()));
            }
        }

        // Build consensus batch WITHOUT mutating in-memory consensus state.
        // The in-memory update (record_block + adjust_target) happens only
        // after the sled transaction succeeds.
        let mut consensus_batch = sled::Batch::default();
        {
            let consensus = self.consensus.lock().unwrap();
            consensus.save_to_batch(&mut consensus_batch);
        }

        // Collect new coin commitments WITHOUT inserting into the live set.
        // The insert happens only after the sled transaction succeeds.
        let new_coins: Vec<[u8; 32]> = block.transactions.iter()
            .filter_map(|tx| tx.coinbase.as_ref().map(|cb| cb.coin))
            .collect();

        let block_bytes = serde_json::to_vec(block)
            .map_err(|e| Error::Custom(format!("Block serialization: {}", e)))?;

        Ok((height, block_bytes, consensus_batch, new_coins))
    }

    /// Check if a coin commitment already exists (double-mint prevention)
    pub fn has_coin(&self, coin: &[u8; 32]) -> bool {
        self.coin_set.lock().unwrap().contains_key(coin)
    }

    /// Check if a coinbase coin has matured (spend lock expired).
    /// Maturity is enforced per COINBASE_MATURITY — coinbase outputs
    /// cannot be spent until N blocks after they were created.
    pub fn is_coin_mature(&self, coin: &[u8; 32], current_height: u64) -> bool {
        match self.coin_set.lock().unwrap().get(coin) {
            Some(&created_height) => current_height.saturating_sub(created_height) >= COINBASE_MATURITY,
            None => true, // not a coinbase coin — always mature
        }
    }

    /// Add a coin commitment to the tracked set at the given block height
    pub fn add_coin(&self, coin: [u8; 32], height: u64) {
        self.coin_set.lock().unwrap().insert(coin, height);
    }

    /// Check if a nullifier has already been used (double-spend prevention)
    pub fn has_nullifier(&self, nullifier: &[u8; 32]) -> bool {
        self.nullifier_set.lock().unwrap().contains(nullifier)
    }

    /// Add a nullifier to the tracked set
    pub fn add_nullifier(&self, nullifier: [u8; 32]) {
        self.nullifier_set.lock().unwrap().insert(nullifier);
    }

    /// Compute the current coin Merkle root from all tracked coins.
    /// Uses a simple incremental hash for now — can be upgraded to a
    /// proper incremental Merkle tree (depth 32, poseidon hash).
    pub fn compute_coin_merkle_root(&self) -> [u8; 32] {
        let coins = self.coin_set.lock().unwrap();
        if coins.is_empty() {
            return [0u8; 32]
        }
        // Simple root: blake3 hash of all sorted coins
        let mut sorted: Vec<&[u8; 32]> = coins.keys().collect();
        sorted.sort();
        let mut hasher = blake3::Hasher::new();
        for coin in sorted {
            hasher.update(coin);
        }
        *hasher.finalize().as_bytes()
    }

    /// Compute coin Merkle root including a new coin (without adding it permanently).
    /// Used during block construction to compute the header before the block is finalized.
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

    /// Compute the current nullifier root from all tracked nullifiers.
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

    /// Verify and apply a block to the chain
    ///
    /// Note: dwow_chain::Transaction is a UTXO transaction without contract calls.
    /// For smart contract execution, the linear Transaction type would need to be
    /// extended to support contract calls similar to dwow_core::Transaction.
    pub async fn apply_block(&self, block: &Block) -> Result<()> {
        self.apply_block_with_uncles(block, &[]).await
    }

    /// Verify and apply a block with uncle blocks to the chain.
    ///
    /// The canonical block is applied first. Uncle processing (validation,
    /// execution, and commit) happens after the canonical block is committed
    /// and is best-effort — uncle failure does not invalidate the canonical
    /// block. This decouples fork resolution from block production.
    pub async fn apply_block_with_uncles(&self, block: &Block, uncles: &[UncleBlock]) -> Result<()> {
        let _apply_guard = self.apply_lock.lock().await;
        self.apply_canonical_block(block).await?;
        if !uncles.is_empty() {
            self.process_uncles(block, uncles).await?;
        }
        Ok(())
    }

    /// Apply just the canonical block. Uncles are processed separately.
    async fn apply_canonical_block(&self, block: &Block) -> Result<()> {
        let vm = self.get_vm(block.header.randomx_key);
        let block_hash = block.hash_with_vm(&vm);
        info!(target: "linear_blockchain", "Applying block at height {}", block.header.height);

        // --- Phase 1: Pure validation ---
        let current_height = self.height.load(Ordering::SeqCst);
        let target = {
            let consensus = self.consensus.lock().unwrap();
            consensus.target()
        };
        let previous_hash = if current_height > 0 {
            let previous = self.store.get_block(current_height)
                .map_err(|e| Error::Custom(e.to_string()))?;
            let previous_vm = self.get_vm(previous.header.randomx_key);
            Some(previous.hash_with_vm(&previous_vm))
        } else {
            None
        };

        dwow_chain::validation::check_block_header(
            block, &vm, target, current_height, previous_hash.as_ref(),
        ).map_err(|e| Error::Custom(e.to_string()))?;

        // --- Phase 2: WASM execution ---
        let difficulty = target;
        let current_height = self.height.load(Ordering::SeqCst);
        let outcome = crate::execution::execute_block(
            self, block, &[], &vm, current_height, difficulty,
        )?;

        // --- Phase 3: Atomic commit ---
        let (height, block_bytes, consensus_batch, new_coins) =
            self.validate_and_record_block(block)?;

        let overlay_batch = outcome.overlay.aggregate();
        let commit_batch = dwow_chain::commit::build_commit_batch(
            block, &[], overlay_batch,
            &self.consensus.lock().unwrap(),
        );

        dwow_chain::commit::commit_atomic(
            self.store.blocks_tree(),
            self.store.uncles_tree(),
            self.store.contracts_tree(),
            self.store.consensus_tree(),
            &commit_batch,
        ).map_err(|e| Error::Custom(format!("Atomic block commit: {}", e)))?;

        // --- In-memory state (ONLY after commit succeeds) ---
        let current_height = self.height.load(Ordering::SeqCst);
        if height > current_height {
            self.height.store(height, Ordering::SeqCst);
        }
        {
            let mut coin_set = self.coin_set.lock().unwrap();
            for coin in &new_coins {
                coin_set.insert(*coin, height);
            }
        }
        {
            let consensus = self.consensus.lock().unwrap();
            consensus.record_block(block.header.timestamp);
            consensus.adjust_target();
        }

        info!(target: "linear_blockchain", "Block {} applied successfully", block_hash);
        Ok(())
    }

    /// Process uncle blocks for a canonical block that has already been
    /// applied. Uncle validation and commit happen after the canonical
    /// block is safe — the canonical chain is never rolled back for
    /// uncle failures. Errors are returned to the caller so it knows
    /// the uncles were invalid.
    async fn process_uncles(&self, block: &Block, uncles: &[UncleBlock]) -> Result<()> {
        let vm = self.get_vm(block.header.randomx_key);
        let current_height = self.height.load(Ordering::SeqCst);
        let target = {
            let consensus = self.consensus.lock().unwrap();
            consensus.target()
        };

        let existing_uncle_keys: std::collections::HashSet<[u8; 32]> = uncles.iter()
            .map(|u| blake3::hash(&serde_json::to_vec(&u.header).unwrap()).into())
            .filter(|k: &[u8; 32]| {
                self.store.has_uncle(k).unwrap_or(false)
            })
            .collect();

        let (_computed_uncle_root, proofs) = build_uncle_merkle(uncles, &vm);

        dwow_chain::validation::check_uncles(
            uncles, &proofs, &block.header.uncle_merkle_root,
            current_height, &vm, target, &existing_uncle_keys,
        ).map_err(|e| Error::Custom(e.to_string()))?;

        let mut uncles_batch = sled::Batch::default();
        for uncle in uncles {
            let uncle_hash = blake3::hash(&serde_json::to_vec(&uncle.header).unwrap());
            let uncle_value = serde_json::to_vec(uncle).unwrap();
            uncles_batch.insert(uncle_hash.as_bytes(), uncle_value);
        }
        self.store.uncles_tree().apply_batch(uncles_batch)
            .map_err(|e| Error::Custom(format!("uncle commit: {}", e)))?;

        info!(target: "linear_blockchain",
            "Processed {} uncles for block {}", uncles.len(), block.hash_with_vm(&vm));
        Ok(())
    }

    /// Deploy a WASM contract
    ///
    /// Stores the WASM bytes and calls `__initialize` on the contract so that
    /// its database trees (coins, nullifiers, merkle, etc.) are created before
    /// any transactions execute against it.
    ///
    /// Executes inline on the calling thread — matches upstream's sequential
    /// WASM execution model. wasmer does not support concurrent instantiation
    /// across threads safely.
    pub fn deploy_contract(&self, wasm: &[u8], contract_id: ContractId, ix: &[u8]) -> Result<()> {
        info!(target: "linear_blockchain", "Deploying contract {:?}", contract_id);

        // Store WASM bytes in LinearStore so apply_block_with_uncles() can find them
        self.store.set_contract_data(&contract_id.to_bytes(), wasm)
            .map_err(|e| Error::Custom(e.to_string()))?;

        let difficulty = {
            let consensus = self.consensus.lock().unwrap();
            consensus.target()
        };
        let height = self.get_height();
        let vm = self.get_vm(self.get_randomx_key());

        let overlay = Mutex::new(SledTreeOverlay::new(self.store.contracts_tree()));
        let backend = Arc::new(TxBackend {
            overlay,
            store: self.store.clone(),
            height,
            vm,
        });
        let mut runtime = dwow_core::runtime::vm_runtime::Runtime::new(
            wasm,
            backend.clone(),
            contract_id,
            height as u32,
            difficulty,
            dwow_sdk::tx::TransactionHash::none(),
            0,
        ).map_err(|e| Error::Custom(format!("deploy runtime: {}", e)))?;
        runtime.deploy(ix).map_err(|e| Error::Custom(format!("deploy exec: {}", e)))?;
        drop(runtime);

        let overlay = Arc::try_unwrap(backend)
            .map_err(|_| Error::Custom("backend still referenced after deploy".to_string()))?
            .overlay.into_inner().unwrap();
        if let Some(batch) = overlay.aggregate() {
            self.store.contracts_tree()
                .apply_batch(batch)
                .map_err(|e| Error::Custom(e.to_string()))?;
        }

        info!(target: "linear_blockchain", "Contract {:?} deployed successfully", contract_id);
        Ok(())
    }

    /// Get contract WASM
    pub fn get_contract(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        self.store.get_contract_data(&contract_id.to_bytes()).map_err(|e| Error::Custom(e.to_string()))
    }

    /// Check if contract exists
    pub fn has_contract(&self, contract_id: ContractId) -> Result<bool> {
        self.store.has_contract_data(&contract_id.to_bytes()).map_err(|e| Error::Custom(e.to_string()))
    }
}

// Clone intentionally removed — LinearBlockchain is always accessed via
// Arc<LinearBlockchain>. Cloning the Arc shares state; cloning the struct
// would silently fork in-memory state (height, consensus, coin_set, etc.)
// producing correctness bugs that are impossible to detect at compile time.

