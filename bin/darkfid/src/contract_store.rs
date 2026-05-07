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

//! ContractStore - Bridge LinearStore to darkfi's contract storage interface
//!
//! This module provides a bridge that allows darkfi's Runtime to use LinearStore
//! for contract storage during deployment.
//!
//! Runtime::deploy() uses the blockchain overlay to:
//! 1. Initialize zkas and monotree trees for a contract
//! 2. Store the contract WASM bincode
//!
//! This bridge implements those operations using LinearStore's contract storage.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use darkfi::runtime::vm_runtime::ContractStoreAccess;
use darkfi::Result;
use darkfi_sdk::crypto::ContractId;
use darkfi_linear::LinearStore;

/// Handle type used to identify contract trees
pub type TreeHandle = [u8; 32];

/// Manages contract trees using LinearStore as backing storage
pub struct LinearContractStore {
    store: Arc<LinearStore>,
    /// Maps tree_handle -> tree_name for initialized trees
    tree_names: Mutex<HashMap<TreeHandle, String>>,
}

impl LinearContractStore {
    /// Create a new LinearContractStore wrapping the given LinearStore
    pub fn new(store: Arc<LinearStore>) -> Self {
        Self {
            store,
            tree_names: Mutex::new(HashMap::new()),
        }
    }

    /// Initialize a new tree for a contract
    /// Returns the tree handle that will be used for lookups
    pub fn init(&self, cid: &ContractId, tree_name: &str) -> Result<TreeHandle> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        self.store.set_contract_data(handle_str.as_bytes(), tree_name.as_bytes())
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        self.tree_names.lock().unwrap().insert(handle, tree_name.to_string());
        Ok(handle)
    }

    /// Look up a tree handle for an initialized tree
    pub fn lookup(&self, cid: &ContractId, tree_name: &str) -> Result<TreeHandle> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        let data = self.store.get_contract_data(handle_str.as_bytes())
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(darkfi::Error::ContractStateNotFound)
        }
        Ok(handle)
    }

    /// Insert contract WASM bincode
    pub fn insert_bincode(&self, cid: ContractId, bincode: &[u8]) -> Result<()> {
        self.store.set_contract_data(&cid.to_bytes(), bincode)
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        Ok(())
    }

    /// Get contract WASM bincode
    pub fn get_bincode(&self, cid: &ContractId) -> Result<Vec<u8>> {
        let data = self.store.get_contract_data(&cid.to_bytes())
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(darkfi::Error::ContractStateNotFound)
        }
        Ok(data)
    }

    /// Get a tree handle by name (helper)
    pub fn get_tree_handle(&self, cid: &ContractId, tree_name: &str) -> Result<TreeHandle> {
        self.lookup(cid, tree_name)
    }
}

impl Clone for LinearContractStore {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            tree_names: Mutex::new(self.tree_names.lock().unwrap().clone()),
        }
    }
}

// Implement ContractStoreAccess trait for LinearContractStore
impl ContractStoreAccess for LinearContractStore {
    fn lookup(&self, cid: &ContractId, tree_name: &str) -> darkfi::Result<[u8; 32]> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        let data = self.store.get_contract_data(handle_str.as_bytes())
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(darkfi::Error::ContractStateNotFound)
        }
        Ok(handle)
    }

    fn init(&self, cid: &ContractId, tree_name: &str) -> darkfi::Result<[u8; 32]> {
        let handle = cid.hash_state_id(tree_name);
        let handle_str = format!("{:?}", handle);
        self.store.set_contract_data(handle_str.as_bytes(), tree_name.as_bytes())
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        self.tree_names.lock().unwrap().insert(handle, tree_name.to_string());
        Ok(handle)
    }

    fn insert_bincode(&self, cid: ContractId, bincode: &[u8]) -> darkfi::Result<()> {
        self.store.set_contract_data(&cid.to_bytes(), bincode)
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn get_bincode(&self, cid: &ContractId) -> darkfi::Result<Vec<u8>> {
        let data = self.store.get_contract_data(&cid.to_bytes())
            .map_err(|e| darkfi::Error::Custom(e.to_string()))?;
        if data.is_empty() {
            return Err(darkfi::Error::ContractStateNotFound)
        }
        Ok(data)
    }
}