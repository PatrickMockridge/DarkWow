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

//! Simple deterministic database wrapper for contract storage.
//!
//! This module provides a simple, deterministic key-value store backed by sled.
//! Unlike the complex overlay/diff system, this does exactly one thing:
//! atomic put/get operations with no caching or deferred application.

use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error};

use crate::Result;

#[derive(Error, Debug)]
pub enum SimpleDbError {
    #[error("Sled error: {0}")]
    SledError(#[from] sled::Error),
}

/// Simple deterministic key-value store backed by sled.
/// No caching, no overlay, no diffs - just atomic put/get.
#[derive(Clone)]
pub struct SimpleDb {
    db: Arc<sled::Db>,
}

impl SimpleDb {
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
    }

    /// Insert a key-value pair into the specified tree.
    /// This is a simple, direct operation with no caching.
    pub fn insert(&self, tree_name: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        debug!(target: "simple_db", "insert() called: tree={:?} key_len={} value_len={}",
            tree_name.iter().take(8).collect::<Vec<_>>(), key.len(), value.len());
        let tree = match self.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(e) => {
                error!(target: "simple_db", "insert() failed to open tree: {:?}", e);
                return Err(e.into())
            }
        };
        if let Err(e) = tree.insert(key, value) {
            error!(target: "simple_db", "insert() failed to insert: tree={:?} err={:?}",
                tree_name.iter().take(8).collect::<Vec<_>>(), e);
            return Err(e.into())
        }
        debug!(target: "simple_db", "insert() succeeded");
        Ok(())
    }

    /// Get a value by key from the specified tree.
    pub fn get(&self, tree_name: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>> {
        let tree = self.db.open_tree(tree_name)?;
        Ok(tree.get(key)?.map(|v| v.to_vec()))
    }

    /// Remove a key from the specified tree.
    pub fn remove(&self, tree_name: &[u8], key: &[u8]) -> Result<()> {
        let tree = self.db.open_tree(tree_name)?;
        tree.remove(key)?;
        Ok(())
    }

    /// Check if a key exists in the specified tree.
    pub fn contains_key(&self, tree_name: &[u8], key: &[u8]) -> Result<bool> {
        let tree = self.db.open_tree(tree_name)?;
        Ok(tree.contains_key(key)?)
    }
}