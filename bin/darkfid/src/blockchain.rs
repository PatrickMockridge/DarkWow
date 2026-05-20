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
//! This module provides a LinearBlockchain that combines dwow_linear's
//! LinearStore with darkfi's Runtime and ZK verification for contract execution.

use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use randomx::{RandomXFlags, RandomXVM};
use dwow::runtime::vm_runtime::{BlockchainAccess, ContractStoreAccess, SimpleDbAccess};
use dwow::Error;
use dwow::Result;
use dwow_linear::{build_uncle_merkle, verify_uncle_proof, FinalityConfig, UncleBlock, Block, LinearStore, PoWConsensus};
use dwow_sdk::crypto::ContractId;
use tracing::{error, info};

use crate::zk::ZkVerifier;

/// Configuration for LinearBlockchain PoW
pub struct LinearPoWConfig {
    /// Target block time in seconds
    pub target_block_time: u64,
    /// Initial difficulty
    pub initial_difficulty: u32,
    /// Minimum difficulty
    pub min_difficulty: u32,
    /// Maximum difficulty
    pub max_difficulty: u32,
}

impl Default for LinearPoWConfig {
    fn default() -> Self {
        Self {
            target_block_time: 60,
            initial_difficulty: 0x0000FFFF,
            min_difficulty: 1,
            max_difficulty: u32::MAX,
        }
    }
}

/// Linear blockchain with WASM runtime and ZK verification
pub struct LinearBlockchain {
    /// Storage backend (dwow_linear)
    pub store: Arc<LinearStore>,
    /// Contract store adapter for Runtime
    contract_store: Arc<dyn ContractStoreAccess>,
    /// State db adapter for Runtime
    state_db: Arc<dyn SimpleDbAccess>,
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
    /// Set of coin commitments for double-mint prevention
    coin_set: Mutex<HashSet<[u8; 32]>>,
    /// Set of nullifiers for double-spend prevention
    nullifier_set: Mutex<HashSet<[u8; 32]>>,
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
            config.initial_difficulty,
            config.min_difficulty,
            config.max_difficulty,
        );
        let zk_verifier = ZkVerifier;
        let height = AtomicU64::new(store.get_height().unwrap_or(0));

        // Create the adapters for Runtime
        let contract_store = crate::contract_store::LinearContractStore::new(store.clone());
        let state_db = crate::linear_simple_db::LinearSimpleDb::new(store.clone());

        // Initialize RandomX VM with default key
        let randomx_key = [0u8; 32];
        let vm = Self::create_vm(&randomx_key).expect("Failed to create RandomX VM");

        Self {
            store,
            contract_store: Arc::new(contract_store),
            state_db: Arc::new(state_db),
            consensus: Mutex::new(consensus),
            zk_verifier,
            finality_config,
            height,
            vm: Mutex::new(vm),
            randomx_key: Mutex::new(randomx_key),
            coin_set: Mutex::new(HashSet::new()),
            nullifier_set: Mutex::new(HashSet::new()),
        }
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

    /// Get current RandomX VM (for blockchain access)
    fn get_current_vm(&self) -> Arc<RandomXVM> {
        let key = *self.randomx_key.lock().unwrap();
        self.get_vm(key)
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

    /// Insert a block into the chain, tracking coins and nullifiers from transactions.
    pub fn insert_block(&self, block: &Block) -> Result<()> {
        let height = block.header.height;

        // Finality check: mode-aware anchor enforcement
        if let Ok(existing) = self.store.get_block(height) {
            if self.finality_config.should_enforce(existing.header.finality_flags)
                && existing.header.anchor_tx_id != [0u8; 32]
            {
                info!(
                    target: "linear_blockchain",
                    "Rejected block at height {} — existing block is anchored (tx_id: {:?})",
                    height, existing.header.anchor_tx_id
                );
                return Err(Error::Custom("AnchoredBlockConflict".to_string()));
            }
        }

        // Record timestamp and adjust difficulty
        {
            let mut consensus = self.consensus.lock().unwrap();
            consensus.record_block(block.header.timestamp);
            consensus.adjust_difficulty();
        }

        // Track coins and nullifiers from coinbase transactions
        for tx in &block.transactions {
            if let Some(ref coinbase) = tx.coinbase {
                let mut coin_set = self.coin_set.lock().unwrap();
                let _nullifier_set = self.nullifier_set.lock().unwrap();
                coin_set.insert(coinbase.coin);
                // Mint transactions don't create nullifiers, but check for future use
                // Spends would add nullifiers here
            }
        }

        self.store.insert_block(height, block).map_err(|e| Error::Custom(e.to_string()))?;
        let current_height = self.height.load(Ordering::SeqCst);
        if height > current_height {
            self.height.store(height, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Check if a coin commitment already exists (double-mint prevention)
    pub fn has_coin(&self, coin: &[u8; 32]) -> bool {
        self.coin_set.lock().unwrap().contains(coin)
    }

    /// Add a coin commitment to the tracked set
    pub fn add_coin(&self, coin: [u8; 32]) {
        self.coin_set.lock().unwrap().insert(coin);
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
        let mut sorted: Vec<&[u8; 32]> = coins.iter().collect();
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
        let mut sorted: Vec<&[u8; 32]> = coins.iter().collect();
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
    /// Note: dwow_linear::Transaction is a UTXO transaction without contract calls.
    /// For smart contract execution, the linear Transaction type would need to be
    /// extended to support contract calls similar to dwow::Transaction.
    pub async fn apply_block(&self, block: &Block) -> Result<()> {
        self.apply_block_with_uncles(block, &[]).await
    }

    /// Verify and apply a block with uncle blocks to the chain
    pub async fn apply_block_with_uncles(&self, block: &Block, uncles: &[UncleBlock]) -> Result<()> {
        // Get or create VM for this block's key
        let vm = self.get_vm(block.header.randomx_key);

        let block_hash = block.hash(&vm);
        info!(target: "linear_blockchain", "Applying block at height {}", block.header.height);

        // Verify PoW
        let proof_ok = {
            let consensus = self.consensus.lock().unwrap();
            consensus.verify_proof(block, &vm).map_err(|e| Error::Custom(e.to_string()))?
        };
        if !proof_ok {
            error!(target: "linear_blockchain", "Block {} failed PoW verification", block_hash);
            return Err(Error::BlockIsInvalid(block_hash.to_string()))
        }

        // Verify merkle root
        if !block.verify_merkle_root() {
            error!(target: "linear_blockchain", "Block {} failed merkle root verification", block_hash);
            return Err(Error::Custom("MerkleRootMismatch".to_string()))
        }

        // Verify uncle merkle root matches provided uncles
        let (expected_root, proofs) = build_uncle_merkle(uncles, &vm);
        if block.header.uncle_merkle_root != expected_root {
            error!(target: "linear_blockchain", "Block {} uncle merkle root mismatch", block_hash);
            return Err(Error::Custom("UncleMerkleRootMismatch".to_string()))
        }

        // Verify each uncle's PoW
        let consensus = self.consensus.lock().unwrap();
        for uncle in uncles {
            match consensus.verify_uncle_pow(uncle, &vm) {
                Ok(true) => {}
                Ok(false) => {
                    error!(target: "linear_blockchain", "Uncle {} failed PoW verification", uncle.hash(&vm));
                    return Err(Error::BlockIsInvalid(uncle.hash(&vm).to_string()))
                }
                Err(e) => {
                    error!(target: "linear_blockchain", "Uncle {} failed PoW: {}", uncle.hash(&vm), e);
                    return Err(Error::Custom(e.to_string()))
                }
            }

            // Verify uncle proof
            if !verify_uncle_proof(&proofs[0], &block.header.uncle_merkle_root, &vm, block.header.difficulty_target) {
                error!(target: "linear_blockchain", "Uncle {} failed merkle proof verification", uncle.hash(&vm));
                return Err(Error::Custom("UncleProofVerificationFailed".to_string()))
            }
        }
        drop(consensus);

        // Verify previous hash
        let current_height = self.height.load(Ordering::SeqCst);
        if current_height > 0 {
            let previous = self.store.get_block(current_height).map_err(|e| Error::Custom(e.to_string()))?;
            if block.header.previous != previous.hash(&vm) {
                error!(target: "linear_blockchain", "Block {} has invalid previous hash", block_hash);
                return Err(Error::Custom("InvalidPreviousHash".to_string()))
            }
        }

        // Execute contract calls in transactions using WASM runtime
        let mut calls_executed = 0;
        let mut calls_failed = 0;
        for tx in &block.transactions {
            for (call_idx, call) in tx.contract_calls.iter().enumerate() {
                // Get contract WASM from store
                let contract_id = ContractId::from_bytes(call.contract_id)
                    .map_err(|e| Error::Custom(format!("Invalid contract ID: {}", e)))?;
                let wasm_bytes = self.store.get_contract_data(&call.contract_id)
                    .map_err(|e| Error::Custom(e.to_string()))?;

                if wasm_bytes.is_empty() {
                    error!(target: "linear_blockchain", "Contract not found for call in tx {}", tx.hash());
                    calls_failed += 1;
                    continue;
                }

                // Create runtime for this contract call
                let tx_hash = tx.hash();
                let tx_hash_bytes = dwow_sdk::tx::TransactionHash(*tx_hash.as_bytes());
                let difficulty = {
                    let consensus = self.consensus.lock().unwrap();
                    consensus.difficulty_target()
                };
                let mut runtime = match dwow::runtime::vm_runtime::Runtime::new(
                    &wasm_bytes,
                    self.contract_store.clone(),
                    self.state_db.clone(),
                    Arc::new(self.clone()),
                    contract_id,
                    current_height as u32,
                    difficulty,
                    tx_hash_bytes,
                    call_idx as u8,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        error!(target: "linear_blockchain", "Failed to create runtime for contract {:?}: {}", contract_id, e);
                        calls_failed += 1;
                        continue
                    }
                };

                // Execute metadata phase
                if let Err(e) = runtime.metadata(&call.data) {
                    error!(target: "linear_blockchain", "metadata() failed for contract {:?}: {}", contract_id, e);
                    calls_failed += 1;
                    continue;
                }

                // Execute exec phase (verifies ZK proofs, runs contract logic)
                if let Err(e) = runtime.exec(&call.data) {
                    error!(target: "linear_blockchain", "exec() failed for contract {:?}: {}", contract_id, e);
                    calls_failed += 1;
                    continue;
                }

                // Execute apply phase (commits state changes)
                if let Err(e) = runtime.apply(&[]) {
                    error!(target: "linear_blockchain", "apply() failed for contract {:?}: {}", contract_id, e);
                    calls_failed += 1;
                    continue;
                }

                info!(target: "linear_blockchain", "Contract call executed successfully: contract_id={:?} call_idx={}", contract_id, call_idx);
                calls_executed += 1;
            }
        }

        if calls_failed > 0 {
            error!(target: "linear_blockchain", "Block {} had {} failed contract calls out of {}", block_hash, calls_failed, calls_executed + calls_failed);
        } else if calls_executed > 0 {
            info!(target: "linear_blockchain", "Block {} executed {} contract calls successfully", block_hash, calls_executed);
        }

        // Insert block
        self.insert_block(block)?;

        // Store uncles
        for uncle in uncles {
            self.store.insert_uncle(uncle).map_err(|e| Error::Custom(e.to_string()))?;
        }

        info!(target: "linear_blockchain", "Block {} applied successfully with {} uncles", block_hash, uncles.len());
        Ok(())
    }

    /// Deploy a WASM contract
    pub fn deploy_contract(&self, wasm: &[u8], contract_id: ContractId) -> Result<()> {
        info!(target: "linear_blockchain", "Deploying contract {:?}", contract_id);
        self.store.set_contract_data(&contract_id.to_bytes(), wasm).map_err(|e| Error::Custom(e.to_string()))?;
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

impl Clone for LinearBlockchain {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            contract_store: self.contract_store.clone(),
            state_db: self.state_db.clone(),
            consensus: Mutex::new(PoWConsensus::default()), // consensus is stateless, recreate it
            zk_verifier: ZkVerifier,
            finality_config: self.finality_config.clone(),
            height: AtomicU64::new(self.height.load(Ordering::SeqCst)),
            vm: Mutex::new(self.vm.lock().unwrap().clone()),
            randomx_key: Mutex::new(*self.randomx_key.lock().unwrap()),
            coin_set: Mutex::new(self.coin_set.lock().unwrap().clone()),
            nullifier_set: Mutex::new(self.nullifier_set.lock().unwrap().clone()),
        }
    }
}

// Implement BlockchainAccess for LinearBlockchain
impl BlockchainAccess for LinearBlockchain {
    fn last_block_timestamp(&self) -> Result<Vec<u8>> {
        let height = self.height.load(Ordering::SeqCst);
        if height == 0 {
            return Ok(0u64.to_le_bytes().to_vec())
        }
        let block = self.store.get_block(height).map_err(|e| Error::Custom(e.to_string()))?;
        Ok(block.header.timestamp.to_le_bytes().to_vec())
    }

    fn last_block_height(&self) -> Result<u32> {
        let height = self.height.load(Ordering::SeqCst);
        Ok(height as u32)
    }

    fn get_tx(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        match self.store.get_transaction(hash) {
            Ok(tx) => {
                let data = serde_json::to_vec(&tx).map_err(|e| Error::Custom(e.to_string()))?;
                Ok(Some(data))
            }
            Err(e) => {
                // Transaction not found is not an error - return None
                if e.to_string().contains("TransactionNotFound") {
                    Ok(None)
                } else {
                    Err(Error::Custom(e.to_string()))
                }
            }
        }
    }

    fn get_tx_location(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        // For linear chain, we need to find which block contains this tx
        // LinearStore doesn't have an index, so we scan blocks
        // This is inefficient but works for linear's simple use case
        let height = self.height.load(Ordering::SeqCst);
        for h in 1..=height {
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
        let vm = self.get_current_vm();
        match self.store.get_block(height as u64) {
            Ok(block) => Ok(Some(block.hash(&vm).as_bytes().to_vec())),
            Err(e) => {
                // Block not found is not an error - return None
                if e.to_string().contains("BlockNotFound") {
                    Ok(None)
                } else {
                    Err(Error::Custom(e.to_string()))
                }
            }
        }
    }
}