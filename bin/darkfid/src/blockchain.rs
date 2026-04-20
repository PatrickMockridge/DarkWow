/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
//! This module provides a LinearBlockchain that combines darkfi_linear's
//! LinearStore with darkfi's Runtime and ZK verification for contract execution.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use darkfi::runtime::vm_runtime::{BlockchainAccess, ContractStoreAccess, SimpleDbAccess};
use darkfi::Error;
use darkfi::Result;
use darkfi_linear::{Block, LinearStore, PoWConsensus};
use darkfi_sdk::crypto::ContractId;
use tracing::{error, info};

use crate::zk::ZkVerifier;

/// Linear blockchain with WASM runtime and ZK verification
pub struct LinearBlockchain {
    /// Storage backend (darkfi_linear)
    pub store: Arc<LinearStore>,
    /// Contract store adapter for Runtime
    contract_store: Arc<dyn ContractStoreAccess>,
    /// State db adapter for Runtime
    state_db: Arc<dyn SimpleDbAccess>,
    /// PoW consensus
    pub consensus: PoWConsensus,
    /// ZK verifier
    pub zk_verifier: ZkVerifier,
    /// Current chain height
    height: AtomicU64,
}

impl LinearBlockchain {
    /// Create a new LinearBlockchain with the given sled database
    pub fn new(store: Arc<LinearStore>) -> Self {
        let consensus = PoWConsensus::default();
        let zk_verifier = ZkVerifier;
        let height = AtomicU64::new(store.get_height().unwrap_or(0));

        // Create the adapters for Runtime
        let contract_store = crate::contract_store::LinearContractStore::new(store.clone());
        let state_db = crate::linear_simple_db::LinearSimpleDb::new(store.clone());

        Self {
            store,
            contract_store: Arc::new(contract_store),
            state_db: Arc::new(state_db),
            consensus,
            zk_verifier,
            height,
        }
    }

    /// Get current chain height
    pub fn get_height(&self) -> u64 {
        self.height.load(Ordering::SeqCst)
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

    /// Insert a block into the chain
    pub fn insert_block(&self, block: &Block) -> Result<()> {
        let height = block.header.height;
        self.store.insert_block(height, block).map_err(|e| Error::Custom(e.to_string()))?;
        let current_height = self.height.load(Ordering::SeqCst);
        if height > current_height {
            self.height.store(height, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Verify and apply a block to the chain
    ///
    /// Note: darkfi_linear::Transaction is a UTXO transaction without contract calls.
    /// For smart contract execution, the linear Transaction type would need to be
    /// extended to support contract calls similar to darkfi::Transaction.
    pub async fn apply_block(&self, block: &Block) -> Result<()> {
        let block_hash = block.hash();
        info!(target: "linear_blockchain", "Applying block at height {}", block.header.height);

        // Verify PoW
        match self.consensus.verify_proof(block) {
            Ok(true) => {}
            Ok(false) => {
                error!(target: "linear_blockchain", "Block {} failed PoW verification", block_hash);
                return Err(Error::BlockIsInvalid(block_hash.to_string()))
            }
            Err(e) => {
                error!(target: "linear_blockchain", "Block {} failed PoW: {}", block_hash, e);
                return Err(Error::Custom(e.to_string()))
            }
        }

        // Verify merkle root
        if !block.verify_merkle_root() {
            error!(target: "linear_blockchain", "Block {} failed merkle root verification", block_hash);
            return Err(Error::Custom("MerkleRootMismatch".to_string()))
        }

        // Verify previous hash
        let current_height = self.height.load(Ordering::SeqCst);
        if current_height > 0 {
            let previous = self.store.get_block(current_height).map_err(|e| Error::Custom(e.to_string()))?;
            if block.header.previous != previous.hash() {
                error!(target: "linear_blockchain", "Block {} has invalid previous hash", block_hash);
                return Err(Error::Custom("InvalidPreviousHash".to_string()))
            }
        }

        // Note: Transaction execution via Runtime would go here when
        // darkfi_linear::Transaction is extended to support contract calls.
        // For now, this is a UTXO chain without smart contract execution.

        // Insert block
        self.insert_block(block)?;

        info!(target: "linear_blockchain", "Block {} applied successfully", block_hash);
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
            consensus: PoWConsensus::default(), // consensus is stateless, recreate it
            zk_verifier: ZkVerifier,
            height: AtomicU64::new(self.height.load(Ordering::SeqCst)),
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
        match self.store.get_block(height as u64) {
            Ok(block) => Ok(Some(block.hash().as_bytes().to_vec())),
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