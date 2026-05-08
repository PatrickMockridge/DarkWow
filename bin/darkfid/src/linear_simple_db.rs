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

//! LinearSimpleDb - Bridge LinearStore to SimpleDb interface
//!
//! This module provides a SimpleDb-compatible wrapper around LinearStore
//! for use with darkfi's Runtime during contract execution.
//!
//! LinearStore uses hardcoded trees (blocks, transactions, contracts).
//! This adapter uses the contracts tree to store arbitrary key-value pairs
//! for contract state, using composite keys: tree_name | key -> value

use std::sync::Arc;

use dwow::runtime::vm_runtime::SimpleDbAccess;
use dwow::Result;
use dwow_linear::LinearStore;

/// Wraps LinearStore to implement SimpleDb-compatible interface for Runtime
#[derive(Clone)]
pub struct LinearSimpleDb {
    store: Arc<LinearStore>,
}

#[allow(dead_code)]
impl LinearSimpleDb {
    /// Create a new LinearSimpleDb wrapping the given LinearStore
    pub fn new(store: Arc<LinearStore>) -> Self {
        Self { store }
    }

    /// Insert a key-value pair into the specified tree.
    /// Uses composite key: tree_name || key -> value
    pub fn insert(&self, tree_name: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        self.store.set_contract_data(&composite_key, value)
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        Ok(())
    }

    /// Get a value by tree_name and key.
    /// Uses composite key: tree_name || key -> value
    pub fn get(&self, tree_name: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        let data = self.store.get_contract_data(&composite_key)
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        if data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(data))
        }
    }

    /// Remove a key from a tree.
    pub fn remove(&self, tree_name: &[u8], key: &[u8]) -> Result<()> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        self.store.set_contract_data(&composite_key, &[])
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        Ok(())
    }

    /// Check if a key exists in a tree.
    pub fn contains_key(&self, tree_name: &[u8], key: &[u8]) -> Result<bool> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        let data = self.store.get_contract_data(&composite_key)
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        Ok(!data.is_empty())
    }
}

impl From<Arc<LinearStore>> for LinearSimpleDb {
    fn from(store: Arc<LinearStore>) -> Self {
        Self::new(store)
    }
}

// Implement SimpleDbAccess trait for LinearSimpleDb
impl SimpleDbAccess for LinearSimpleDb {
    fn insert(&self, tree_name: &[u8], key: &[u8], value: &[u8]) -> dwow::Result<()> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        self.store.set_contract_data(&composite_key, value)
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn get(&self, tree_name: &[u8], key: &[u8]) -> dwow::Result<Option<Vec<u8>>> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        let data = self.store.get_contract_data(&composite_key)
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        if data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(data))
        }
    }

    fn remove(&self, tree_name: &[u8], key: &[u8]) -> dwow::Result<()> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        self.store.set_contract_data(&composite_key, &[])
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        Ok(())
    }

    fn contains_key(&self, tree_name: &[u8], key: &[u8]) -> dwow::Result<bool> {
        let mut composite_key = Vec::with_capacity(tree_name.len() + key.len());
        composite_key.extend(tree_name);
        composite_key.extend(key);
        let data = self.store.get_contract_data(&composite_key)
            .map_err(|e| dwow::Error::Custom(e.to_string()))?;
        Ok(!data.is_empty())
    }
}