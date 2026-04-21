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

//! WASM Runtime adapter for Linear blockchain
//!
//! This module provides implementations of the WASM runtime traits
//! (ContractStoreAccess, SimpleDbAccess, BlockchainAccess) for LinearStore
//! and LinearBlockchain, enabling WASM contract execution on linear-testnet.

use std::sync::Arc;

use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{deserialize, serialize};
use darkfi::runtime::vm_runtime::{BlockchainAccess, ContractStoreAccess, SimpleDbAccess};
use darkfi::Result;

use darkfi_linear::{LinearBlockchain, LinearStore};

/// Prefix for contract state trees in LinearStore
const CONTRACT_STATE_PREFIX: &str = "_contract_state_";

/// Implement ContractStoreAccess for Arc<LinearStore>
/// This allows WASM contracts to be deployed and managed.
impl ContractStoreAccess for Arc<LinearStore> {
    fn lookup(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        let tree_key = format!("{}{}_{}", CONTRACT_STATE_PREFIX, cid.to_hex(), tree_name);
        let _tree = self.db.open_tree(&tree_key).map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        let hash = blake3::hash(tree_key.as_bytes());
        let mut handle = [0u8; 32];
        handle.copy_from_slice(hash.as_bytes());
        Ok(handle)
    }

    fn init(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        let tree_key = format!("{}{}_{}", CONTRACT_STATE_PREFIX, cid.to_hex(), tree_name);
        self.db.open_tree(&tree_key).map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        let hash = blake3::hash(tree_key.as_bytes());
        let mut handle = [0u8; 32];
        handle.copy_from_slice(hash.as_bytes());
        Ok(handle)
    }

    fn insert_bincode(&self, cid: ContractId, bincode: &[u8]) -> Result<()> {
        let key = serialize(&cid);
        self.contracts.insert(&key, bincode).map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn get_bincode(&self, cid: &ContractId) -> Result<Vec<u8>> {
        let key = serialize(cid);
        match self.contracts.get(&key).map_err(|e| darkfi::Error::Custom(e.to_string()))? {
            Some(v) => Ok(v.to_vec()),
            None => Err(darkfi::Error::Custom("Contract not found".to_string())),
        }
    }
}

/// Implement SimpleDbAccess for Arc<LinearStore>
/// This allows WASM contracts to read/write state.
impl SimpleDbAccess for Arc<LinearStore> {
    fn insert(&self, tree: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        // tree is a handle (blake3 hash), we use hex encoding to namespace keys
        let tree_key = hex::encode(tree);
        let full_key = format!("{}:{}", tree_key, hex::encode(key));
        self.contracts
            .insert(full_key.as_bytes(), value)
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn get(&self, tree: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>> {
        let tree_key = hex::encode(tree);
        let full_key = format!("{}:{}", tree_key, hex::encode(key));
        match self.contracts.get(full_key.as_bytes()).map_err(|e| darkfi::Error::Custom(e.to_string()))? {
            Some(v) => Ok(Some(v.to_vec())),
            None => Ok(None),
        }
    }

    fn remove(&self, tree: &[u8], key: &[u8]) -> Result<()> {
        let tree_key = hex::encode(tree);
        let full_key = format!("{}:{}", tree_key, hex::encode(key));
        self.contracts.remove(full_key.as_bytes()).map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn contains_key(&self, tree: &[u8], key: &[u8]) -> Result<bool> {
        let tree_key = hex::encode(tree);
        let full_key = format!("{}:{}", tree_key, hex::encode(key));
        self.contracts.contains_key(full_key.as_bytes()).map_err(|e| darkfi::Error::Custom(e.to_string()))
    }
}

/// Implement BlockchainAccess for Arc<LinearBlockchain>
/// This allows WASM contracts to query blockchain state.
impl BlockchainAccess for Arc<LinearBlockchain> {
    fn last_block_timestamp(&self) -> Result<Vec<u8>> {
        let block = self.get_latest_block().map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        Ok(serialize(&block.header.timestamp))
    }

    fn last_block_height(&self) -> Result<u32> {
        Ok(self.height as u32)
    }

    fn get_tx(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        match self.store.get_transaction(hash) {
            Ok(tx) => Ok(Some(serialize(&tx))),
            Err(darkfi_linear::LinearError::TransactionNotFound(_)) => Ok(None),
            Err(e) => Err(darkfi::Error::Custom(e.to_string())),
        }
    }

    fn get_tx_location(&self, _hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        // Linear blockchain doesn't have transaction location tracking
        Ok(None)
    }

    fn get_block_hash_by_height(&self, height: u32) -> Result<Option<Vec<u8>>> {
        match self.store.get_block(height as u64) {
            Ok(block) => Ok(Some(serialize(&block.hash()))),
            Err(darkfi_linear::LinearError::BlockNotFound(_)) => Ok(None),
            Err(e) => Err(darkfi::Error::Custom(e.to_string())),
        }
    }
}
