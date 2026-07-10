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

//! Simple sled storage for linear blockchain

use std::sync::Arc;

use sled::{Db, Tree};

use super::{Block, LinearError, Transaction, UncleBlock};

/// Tree names for sled database
const BLOCKS_TREE: &str = "blocks";
const TXS_TREE: &str = "transactions";
const CONTRACTS_TREE: &str = "contracts";
const UNCLES_TREE: &str = "uncles";
const CONSENSUS_TREE: &str = "consensus";
const CHAIN_WORK_TREE: &str = "chain_work";
const COINS_TREE: &str = "coins";
const NULLIFIERS_TREE: &str = "nullifiers";
pub const SUPPLY_CHAIN_TREE: &str = "supply_chain";

/// Linear store - simple sled-backed blockchain storage
#[derive(Clone)]
pub struct LinearStore {
    db: Arc<Db>,
    pub blocks: Tree,
    transactions: Tree,
    pub contracts: Tree,
    pub uncles: Tree,
    pub consensus: Tree,
    /// Accumulated chain work (u32::MAX / target per block)
    pub chain_work: Tree,
    /// Coin commitments → block height (for maturity tracking)
    pub coins: Tree,
    /// Spent nullifiers (empty value = spent)
    pub nullifiers: Tree,
    /// Cumulative supply chain — Pedersen commitment chain S_H = S_{H-1} + C_H
    pub supply_chain: Tree,
}

impl LinearStore {
    /// Open or create a new linear store
    pub fn new(db: Arc<Db>) -> Result<Self, LinearError> {
        let blocks = db.open_tree(BLOCKS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let transactions = db.open_tree(TXS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let contracts = db.open_tree(CONTRACTS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let uncles = db.open_tree(UNCLES_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let consensus = db.open_tree(CONSENSUS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let chain_work = db.open_tree(CHAIN_WORK_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let coins = db.open_tree(COINS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let nullifiers = db.open_tree(NULLIFIERS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let supply_chain = db.open_tree(SUPPLY_CHAIN_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;

        Ok(Self { db, blocks, transactions, contracts, uncles, consensus, chain_work, coins, nullifiers, supply_chain })
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

    /// Get the current chain height.
    /// Uses sled B-tree ordering: keys are height.to_le_bytes(), so
    /// the last entry is the highest height. O(log n) instead of O(n).
    pub fn get_height(&self) -> Result<u64, LinearError> {
        if let Ok(Some((k, _))) = self.blocks.last() {
            if k.len() == 8 {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(k.as_ref());
                return Ok(u64::from_le_bytes(bytes));
            }
        }
        Err(LinearError::BlockNotFound(0))
    }

    /// Flush the database
    pub fn flush(&self) -> Result<(), LinearError> {
        self.db.flush().map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Access the underlying blocks sled tree.
    pub fn blocks_tree(&self) -> &sled::Tree {
        &self.blocks
    }

    /// Access the underlying uncles sled tree.
    pub fn uncles_tree(&self) -> &sled::Tree {
        &self.uncles
    }

    /// Access the underlying contracts sled tree.
    pub fn contracts_tree(&self) -> &sled::Tree {
        &self.contracts
    }

    /// Access the underlying consensus sled tree.
    pub fn consensus_tree(&self) -> &sled::Tree {
        &self.consensus
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

    /// Insert an uncle block (keyed by block hash)
    /// Note: Uses blake3 for storage key since this is just for lookup, not PoW
    pub fn insert_uncle(&self, uncle: &UncleBlock) -> Result<(), LinearError> {
        let hash = blake3::hash(&serde_json::to_vec(&uncle.header)
            .unwrap_or_else(|e| { tracing::error!(target: "dwow_chain::store", "Uncle header serialization failed: {}", e); vec![0u8; 32] }));
        let key = hash.as_bytes();
        let value = serde_json::to_vec(uncle).map_err(|e| LinearError::SerializationError(e.to_string()))?;
        self.uncles.insert(key, value.as_slice()).map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Check if an uncle block with the given hash has already been stored
    pub fn has_uncle(&self, hash: &[u8]) -> Result<bool, LinearError> {
        self.uncles.contains_key(hash).map_err(|e| LinearError::StorageError(e.to_string()))
    }

    /// Get an uncle block by hash
    pub fn get_uncle(&self, hash: &[u8]) -> Result<Option<UncleBlock>, LinearError> {
        match self.uncles.get(hash).map_err(|e| LinearError::StorageError(e.to_string()))? {
            Some(v) => {
                let uncle = serde_json::from_slice(&v).map_err(|e| LinearError::SerializationError(e.to_string()))?;
                Ok(Some(uncle))
            }
            None => Ok(None),
        }
    }
}