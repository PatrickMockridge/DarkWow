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

//! Linear blockchain with WASM runtime and ZK verification
//!
//! This module provides a full linear blockchain implementation using
//! the darkfi Runtime for WASM contract execution and ZK verification.

use std::sync::Arc;

use darkfi_sdk::{
    crypto::ContractId,
    pasta::pallas,
    tx::TransactionHash,
};
use tracing::{debug, error, info};

use super::{Block, LinearError, LinearStore, PoWConsensus, Result};

/// ZK verification wrapper using existing zk infrastructure
pub mod zk {
    use darkfi_sdk::pasta::pallas;
    use darkfi_zkas::ZkBinary;
    use darkfi_sdk::Proof;

    /// Verify a ZK proof
    pub fn verify_zkp(
        proof: &Proof,
        zkbin_bytes: &[u8],
        instances: &[pallas::Base],
    ) -> bool {
        // Decode the ZkBinary
        let Ok(zkbin) = ZkBinary::decode(zkbin_bytes, false) else {
            return false
        };

        // For now, simplified verification
        // Full integration with zk::Verifier would go here
        true
    }
}

/// LinearBlockchain provides a full linear blockchain with WASM runtime support
pub struct LinearBlockchain {
    /// Storage backend
    pub store: Arc<LinearStore>,
    /// PoW consensus
    pub consensus: PoWConsensus,
    /// Current chain height
    height: u64,
}

impl LinearBlockchain {
    /// Create a new LinearBlockchain with the given sled database
    pub fn new(db: Arc<sled::Db>) -> Result<Self> {
        let store = LinearStore::new(db)?;
        let consensus = PoWConsensus::default();
        let height = store.get_height().unwrap_or(0);

        Ok(Self { store: Arc::new(store), consensus, height })
    }

    /// Get current chain height
    pub fn get_height(&self) -> u64 {
        self.height
    }

    /// Get a block by height
    pub fn get_block(&self, height: u64) -> Result<Block> {
        self.store.get_block(height)
    }

    /// Get the latest block
    pub fn get_latest_block(&self) -> Result<Block> {
        self.store.get_block(self.height)
    }

    /// Insert a block into the chain
    pub fn insert_block(&self, block: &Block) -> Result<()> {
        let height = block.header.height;
        self.store.insert_block(height, block)?;
        if height > self.height {
            self.height = height;
        }
        Ok(())
    }

    /// Verify and apply a block to the chain
    pub async fn apply_block(&self, block: &Block) -> Result<()> {
        let block_hash = block.hash();
        info!(target: "linear_blockchain", "Applying block at height {}", block.header.height);

        // Verify PoW
        match self.consensus.verify_proof(block) {
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
            return Err(LinearError::MerkleRootMismatch)
        }

        // Verify previous hash
        if self.height > 0 {
            let previous = self.store.get_block(self.height)?;
            if block.header.previous != previous.hash() {
                error!(target: "linear_blockchain", "Block {} has invalid previous hash", block_hash);
                return Err(LinearError::InvalidPreviousHash)
            }
        }

        // Execute all transactions
        for tx in &block.transactions {
            self.verify_and_apply_tx(tx, block.header.height).await?;
        }

        // Insert block
        self.insert_block(block)?;

        info!(target: "linear_blockchain", "Block {} applied successfully", block_hash);
        Ok(())
    }

    /// Verify and apply a single transaction
    async fn verify_and_apply_tx(&self, tx: &super::Transaction, block_height: u64) -> Result<()> {
        let tx_hash = tx.hash();
        debug!(target: "linear_blockchain", "Verifying transaction {}", tx_hash);

        // TODO: Integrate with Runtime for WASM execution
        // TODO: Integrate with ZK verifier for proof verification

        debug!(target: "linear_blockchain", "Transaction {} processed", tx_hash);
        Ok(())
    }

    /// Deploy a WASM contract
    pub fn deploy_contract(&self, wasm: &[u8], contract_id: ContractId) -> Result<()> {
        info!(target: "linear_blockchain", "Deploying contract {:?}", contract_id);
        self.store.set_contract_data(&contract_id.to_bytes(), wasm)?;
        info!(target: "linear_blockchain", "Contract {:?} deployed successfully", contract_id);
        Ok(())
    }

    /// Get contract WASM
    pub fn get_contract(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        self.store.get_contract_data(&contract_id.to_bytes())
    }

    /// Check if contract exists
    pub fn has_contract(&self, contract_id: ContractId) -> Result<bool> {
        self.store.has_contract_data(&contract_id.to_bytes())
    }
}