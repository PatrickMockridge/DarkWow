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

//! Simple sled storage for linear blockchain

use std::sync::Arc;

use sled::{Db, Tree};

use super::{Block, LinearError, Transaction};

/// Tree names for sled database
const BLOCKS_TREE: &str = "blocks";
const TXS_TREE: &str = "transactions";
const CONTRACTS_TREE: &str = "contracts";

/// Linear store - simple sled-backed blockchain storage
pub struct LinearStore {
    db: Arc<Db>,
    blocks: Tree,
    transactions: Tree,
    contracts: Tree,
}

impl LinearStore {
    /// Open or create a new linear store
    pub fn new(db: Arc<Db>) -> Result<Self, LinearError> {
        let blocks = db.open_tree(BLOCKS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let transactions = db.open_tree(TXS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let contracts = db.open_tree(CONTRACTS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;

        Ok(Self { db, blocks, transactions, contracts })
    }

    /// Insert a block at the given height
    pub fn insert_block(&self, height: u64, block: &Block) -> Result<(), LinearError> {
        let key = height.to_le_bytes();
        let value = serde_json::to_vec(block).map_err(|e| LinearError::SerializationError(e.to_string()))?;
        self.blocks.insert(&key, value.as_slice()).map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Get a block by height
    pub fn get_block(&self, height: u64) -> Result<Block, LinearError> {
        let key = height.to_le_bytes();
        let value = self.blocks.get(&key).map_err(|e| LinearError::StorageError(e.to_string()))?
            .ok_or(LinearError::BlockNotFound(height))?;
        serde_json::from_slice(&value).map_err(|e| LinearError::SerializationError(e.to_string()))
    }

    /// Insert a transaction
    pub fn insert_transaction(&self, tx: &Transaction) -> Result<(), LinearError> {
        let hash = tx.hash();
        let key = hash.as_bytes();
        let value = serde_json::to_vec(tx).map_err(|e| LinearError::SerializationError(e.to_string()))?;
        self.transactions.insert(key, value.as_slice()).map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Get a transaction by hash
    pub fn get_transaction(&self, hash: &[u8]) -> Result<Transaction, LinearError> {
        let value = self.transactions.get(hash).map_err(|e| LinearError::StorageError(e.to_string()))?
            .ok_or_else(|| LinearError::TransactionNotFound(hex::encode(hash)))?;
        serde_json::from_slice(&value).map_err(|e| LinearError::SerializationError(e.to_string()))
    }

    /// Get the current chain height
    pub fn get_height(&self) -> Result<u64, LinearError> {
        let mut max_height: u64 = 0;
        for item in self.blocks.iter() {
            if let Ok((k, _)) = item {
                if k.len() == 8 {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(k.as_ref());
                    let height = u64::from_le_bytes(bytes);
                    if height > max_height {
                        max_height = height;
                    }
                }
            }
        }
        if max_height == 0 {
            return Err(LinearError::BlockNotFound(0))
        }
        Ok(max_height)
    }

    /// Flush the database
    pub fn flush(&self) -> Result<(), LinearError> {
        self.db.flush().map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Set contract data (WASM binary) for a contract ID
    pub fn set_contract_data(&self, contract_id: &[u8], data: &[u8]) -> Result<(), LinearError> {
        self.contracts.insert(contract_id, data).map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Get contract data (WASM binary) for a contract ID
    pub fn get_contract_data(&self, contract_id: &[u8]) -> Result<Vec<u8>, LinearError> {
        match self.contracts.get(contract_id).map_err(|e| LinearError::StorageError(e.to_string()))? {
            Some(v) => Ok(v.to_vec()),
            None => Ok(vec![]),
        }
    }

    /// Check if contract data exists for a contract ID
    pub fn has_contract_data(&self, contract_id: &[u8]) -> Result<bool, LinearError> {
        self.contracts.contains_key(contract_id).map_err(|e| LinearError::StorageError(e.to_string()))
    }
}