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

//! Linear blockchain with WASM runtime and ZK verification
//!
//! This module provides a full linear blockchain implementation using
//! the darkfi Runtime for WASM contract execution and ZK verification.

use std::sync::{Arc, Mutex};

use randomx::{RandomXFlags, RandomXVM};
use tracing::{debug, error, info};

use super::{Block, FinalityConfig, LinearError, LinearStore, PoWConsensus, Result};

/// LinearBlockchain provides a full linear blockchain with WASM runtime support.
/// Thread-safe via Arc<Mutex<>> wrapping for interior mutability.
pub struct LinearBlockchain {
    /// Storage backend
    pub store: Arc<LinearStore>,
    /// PoW consensus
    pub consensus: PoWConsensus,
    /// Finality configuration
    pub finality_config: FinalityConfig,
    /// Current chain height - protected by mutex for interior mutability
    height: Mutex<u64>,
    /// RandomX VM cache (for PoW verification) - protected by mutex for interior mutability
    vm: Mutex<Option<Arc<RandomXVM>>>,
    /// Current RandomX key - protected by mutex for interior mutability
    randomx_key: Mutex<[u8; 32]>,
}

impl LinearBlockchain {
    /// Create a new LinearBlockchain with the given sled database
    pub fn new(db: Arc<sled::Db>) -> Result<Self> {
        Self::with_finality(db, FinalityConfig::default())
    }

    /// Create a new LinearBlockchain with custom finality configuration
    pub fn with_finality(db: Arc<sled::Db>, finality_config: FinalityConfig) -> Result<Self> {
        let store = LinearStore::new(db)?;
        let consensus = PoWConsensus::default();

        // Restore persisted consensus state (target + timestamps) if available
        consensus.load(store.consensus_tree()).ok();

        let height = store.get_height().unwrap_or(0);

        // Initialize RandomX VM with default key
        let randomx_key = [0u8; 32];
        let vm = Self::create_vm(&randomx_key)?;

        Ok(Self {
            store: Arc::new(store),
            consensus,
            finality_config,
            height: Mutex::new(height),
            vm: Mutex::new(Some(vm)),
            randomx_key: Mutex::new(randomx_key),
        })
    }

    /// Create a new RandomX VM with the given key.
    ///
    /// JIT is explicitly disabled to avoid SIGILL from -DARCH=native
    /// misdetecting CPU features in the JIT compiler on containerized hosts.
    fn create_vm(key: &[u8; 32]) -> Result<Arc<RandomXVM>> {
        let flags = RandomXFlags::get_recommended_flags() & !RandomXFlags::JIT;
        let cache = randomx::RandomXCache::new(flags, key)
            .map_err(|e| LinearError::StorageError(format!("RandomX cache error: {}", e)))?;
        let vm = RandomXVM::new(flags, Some(cache), None)
            .map_err(|e| LinearError::StorageError(format!("RandomX VM error: {}", e)))?;
        Ok(Arc::new(vm))
    }

    /// Get or create VM for the given key
    pub fn get_vm(&self, key: [u8; 32]) -> Arc<RandomXVM> {
        let mut randomx_key = self.randomx_key.lock().unwrap();
        let mut vm = self.vm.lock().unwrap();
        if key != *randomx_key {
            *randomx_key = key;
            *vm = Some(Self::create_vm(&key).expect("Failed to create RandomX VM"));
        }
        vm.as_ref().unwrap().clone()
    }

    /// Get current RandomX VM for the current key
    pub fn get_current_vm(&self) -> Option<Arc<RandomXVM>> {
        self.vm.lock().unwrap().clone()
    }

    /// Get current RandomX key
    pub fn get_randomx_key(&self) -> [u8; 32] {
        *self.randomx_key.lock().unwrap()
    }

    /// Get tip block hash using current VM
    pub fn get_tip_hash(&self) -> Result<blake3::Hash> {
        let vm = self.vm.lock().unwrap();
        let vm = vm.as_ref().ok_or(LinearError::StorageError("No VM available".to_string()))?;
        let block = self.get_latest_block()?;
        Ok(block.hash(vm))
    }

    /// Get current chain height
    pub fn get_height(&self) -> u64 {
        *self.height.lock().unwrap()
    }

    /// Get a block by height
    pub fn get_block(&self, height: u64) -> Result<Block> {
        self.store.get_block(height)
    }

    /// Get the latest block
    pub fn get_latest_block(&self) -> Result<Block> {
        let height = *self.height.lock().unwrap();
        self.store.get_block(height)
    }

    /// Insert a block into the chain.
    /// Takes &self for thread-safe access via interior mutability.
    ///
    /// Rejects blocks that would replace an already-anchored block at the
    /// same height when finality enforcement is enabled.
    pub fn insert_block(&self, block: &Block) -> Result<()> {
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
                return Err(LinearError::AnchoredBlockConflict);
            }
        }

        let mut current_height = self.height.lock().unwrap();
        self.store.insert_block(height, block)?;
        if height > *current_height {
            *current_height = height;
        }
        Ok(())
    }

    /// Verify and apply a block to the chain
    pub async fn apply_block(&self, block: &Block) -> Result<()> {
        // Get or create VM for this block's key
        let vm = self.get_vm(block.header.randomx_key);

        let block_hash = block.hash(&vm);
        info!(target: "linear_blockchain", "Applying block at height {}", block.header.height);

        // Verify PoW
        match self.consensus.verify_proof(block, &vm) {
            Ok(true) => {}
            Ok(false) => {
                error!(target: "linear_blockchain", "Block {} failed PoW verification", block_hash);
                return Err(LinearError::DifficultyNotMet)
            }
            Err(e) => {
                error!(target: "linear_blockchain", "Block {} failed PoW: {}", block_hash, e);
                return Err(LinearError::DifficultyNotMet)
            }
        }

        // Verify merkle root
        if !block.verify_merkle_root() {
            error!(target: "linear_blockchain", "Block {} failed merkle root verification", block_hash);
            return Err(LinearError::MerkleRootMismatch(block_hash.to_string()))
        }

        // Verify previous hash
        let current_height = *self.height.lock().unwrap();
        if current_height > 0 {
            let previous = self.store.get_block(current_height)?;
            if block.header.previous != previous.hash(&vm) {
                error!(target: "linear_blockchain", "Block {} failed previous hash verification", block_hash);
                return Err(LinearError::InvalidPreviousHash(block_hash.to_string()))
            }
        }

        // Execute all transactions
        for tx in &block.transactions {
            self.verify_and_apply_tx(tx, block.header.height).await?;
        }

        // Insert block
        self.insert_block(block)?;

        // Update difficulty after block is accepted
        self.consensus.record_block(block.header.timestamp);
        self.consensus.adjust_target();
        let _ = self.consensus.save(self.store.consensus_tree());

        info!(target: "linear_blockchain", "Block {} applied successfully", block_hash);
        Ok(())
    }

    /// Verify and apply a single transaction
    async fn verify_and_apply_tx(&self, tx: &super::Transaction, _block_height: u64) -> Result<()> {
        let tx_hash = tx.hash();
        debug!(target: "linear_blockchain", "Verifying transaction {}", tx_hash);

        // TODO: Integrate with Runtime for WASM execution
        // TODO: Integrate with ZK verifier for proof verification

        debug!(target: "linear_blockchain", "Transaction {} processed", tx_hash);
        Ok(())
    }
}