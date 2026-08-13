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

use dwow_sdk::blockchain::BlockHeight;
use dwow_serial::{deserialize as dwow_deserialize, serialize as dwow_serialize};
use sled::{Db, Tree};

use super::{Block, LinearError, Transaction, UncleBlock};

/// Tree names for sled database
const BLOCKS_TREE: &str = "blocks";
const TXS_TREE: &str = "transactions";
const CONTRACTS_TREE: &str = "contracts";
const UNCLES_TREE: &str = "uncles";
const CONSENSUS_TREE: &str = "consensus";
const COINS_TREE: &str = "coins";
const NULLIFIERS_TREE: &str = "nullifiers";
pub const SUPPLY_CHAIN_TREE: &str = "supply_chain";
const BLOCK_TARGETS_TREE: &str = "block_targets";
const CONTRACT_RISK_TREE: &str = "contract_risk";

/// Linear store - simple sled-backed blockchain storage
#[derive(Clone)]
pub struct LinearStore {
    db: Arc<Db>,
    pub blocks: Tree,
    transactions: Tree,
    pub contracts: Tree,
    pub uncles: Tree,
    pub consensus: Tree,
    /// Coin commitments → block height (for maturity tracking)
    pub coins: Tree,
    /// Nullifiers → height, tagged by kind: value = [kind] ++ height.to_le_bytes()
    /// (9 bytes), kind 0 = claim (coinbase maturity), kind 1 = spend (double-spend).
    pub nullifiers: Tree,
    /// Cumulative supply chain — Pedersen commitment chain S_H = S_{H-1} + C_H
    pub supply_chain: Tree,
    /// Per-block PoW targets — O(1) difficulty lookup (M-1 fix).
    /// Key: height.to_le_bytes(), Value: target.get().to_le_bytes() (4 bytes).
    /// Write-once per height, removed on disconnect.
    pub block_targets: Tree,
    /// Per-contract dynamic risk factors (FI-RISK-3, fee-spec.md §14.7).
    /// Key: contract_id.to_bytes() (32 bytes), Value: risk_factor.get().to_le_bytes() (8 bytes).
    pub contract_risk: Tree,
}

impl LinearStore {
    /// Open or create a new linear store
    pub fn new(db: Arc<Db>) -> Result<Self, LinearError> {
        let blocks = db.open_tree(BLOCKS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let transactions = db.open_tree(TXS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let contracts = db.open_tree(CONTRACTS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let uncles = db.open_tree(UNCLES_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let consensus = db.open_tree(CONSENSUS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let coins = db.open_tree(COINS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let nullifiers = db.open_tree(NULLIFIERS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let supply_chain = db.open_tree(SUPPLY_CHAIN_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let block_targets = db.open_tree(BLOCK_TARGETS_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;
        let contract_risk = db.open_tree(CONTRACT_RISK_TREE).map_err(|e| LinearError::StorageError(e.to_string()))?;

        Ok(Self { db, blocks, transactions, contracts, uncles, consensus, coins, nullifiers, supply_chain, block_targets, contract_risk })
    }

    /// Insert a block at the given height
    pub fn insert_block(&self, height: BlockHeight, block: &Block) -> Result<(), LinearError> {
        let key = height.to_le_bytes();
        let value = dwow_serialize(block);
        self.blocks.insert(&key, value.as_slice()).map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Get a block by height
    pub fn get_block(&self, height: BlockHeight) -> Result<Block, LinearError> {
        let key = height.to_le_bytes();
        let value = self.blocks.get(&key).map_err(|e| LinearError::StorageError(e.to_string()))?
            .ok_or(LinearError::BlockNotFound(height))?;
        dwow_deserialize(&value).map_err(|e| LinearError::SerializationError(e.to_string()))
    }

    /// Insert a transaction
    pub fn insert_transaction(&self, tx: &Transaction) -> Result<(), LinearError> {
        let hash = tx.hash();
        let key = hash.as_bytes();
        let value = dwow_serialize(tx);
        self.transactions.insert(key, value.as_slice()).map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Get a transaction by hash
    pub fn get_transaction(&self, hash: &[u8]) -> Result<Transaction, LinearError> {
        let value = self.transactions.get(hash).map_err(|e| LinearError::StorageError(e.to_string()))?
            .ok_or_else(|| LinearError::TransactionNotFound(hex::encode(hash)))?;
        dwow_deserialize(&value).map_err(|e| LinearError::SerializationError(e.to_string()))
    }

    /// Get the current chain height.
    /// Uses sled B-tree ordering: keys are height.to_le_bytes(), so
    /// the last entry is the highest height. O(log n) instead of O(n).
    pub fn get_height(&self) -> Result<BlockHeight, LinearError> {
        if let Ok(Some((k, _))) = self.blocks.last() {
            if k.len() == 8 {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(k.as_ref());
                return Ok(BlockHeight::from_le_bytes(bytes));
            }
        }
        Err(LinearError::BlockNotFound(BlockHeight::new(0)))
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

    /// Insert an uncle block (keyed by blake3 hash of deterministically-encoded header)
    pub fn insert_uncle(&self, uncle: &UncleBlock) -> Result<(), LinearError> {
        let hash = blake3::hash(&dwow_serialize(&uncle.header));
        let key = hash.as_bytes();
        let value = dwow_serialize(uncle);
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
                let uncle = dwow_deserialize(&v).map_err(|e| LinearError::SerializationError(e.to_string()))?;
                Ok(Some(uncle))
            }
            None => Ok(None),
        }
    }
}