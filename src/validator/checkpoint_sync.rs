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

//! Standalone Checkpoint Sync Module
//!
//! This module provides a clean, standalone mechanism for managing blocks
//! with `zkbin_data` used in checkpoint synchronization. It is independent
//! of the sled-backed blockchain storage.
//!
//! ## Architecture
//!
//! When blocks are received via P2P as `ExtendedProposalMessage`, they carry
//! `zkbin_data` which allows stateless ZK proof verification using
//! `sync::verify_block`. This module caches those blocks so they can be
//! served to other nodes during checkpoint sync without requiring sled lookup.
//!
//! ## Flow
//!
//! 1. Node receives `ExtendedProposalMessage` via P2P
//! 2. Block is verified using `sync::verify_block` (stateless, no sled VK)
//! 3. Block is stored in `CheckpointSync` via `add_block()`
//! 4. When another node requests sync, blocks are retrieved from `CheckpointSync`
//!    (not sled) and include `zkbin_data`

use std::collections::HashMap;

use smol::lock::RwLock;

use crate::blockchain::{BlockInfo, HeaderHash};

/// Standalone module for managing checkpoint blocks with zkbin_data.
///
/// This module is independent of the sled-backed blockchain storage
/// and provides blocks for sync responses. It ensures that blocks
/// received via P2P with `zkbin_data` can be served to other nodes
/// without requiring sled lookup (which would lose `zkbin_data`).
#[derive(Debug)]
pub struct CheckpointSync {
    /// Cache of blocks with zkbin_data, keyed by header hash
    blocks: RwLock<HashMap<HeaderHash, BlockInfo>>,
}

impl CheckpointSync {
    /// Create a new empty CheckpointSync
    pub fn new() -> Self {
        Self { blocks: RwLock::new(HashMap::new()) }
    }

    /// Add a block with zkbin_data to the cache
    pub async fn add_block(&self, block: &BlockInfo) {
        self.blocks.write().await.insert(block.hash(), block.clone());
    }

    /// Get a block by hash if available
    pub async fn get_block(&self, hash: &HeaderHash) -> Option<BlockInfo> {
        self.blocks.read().await.get(hash).cloned()
    }

    /// Get multiple blocks by hash
    /// Returns blocks in the same order as the input hashes
    /// Missing blocks are returned as None
    pub async fn get_blocks(&self, hashes: &[HeaderHash]) -> Vec<Option<BlockInfo>> {
        let blocks = self.blocks.read().await;
        hashes.iter().map(|h| blocks.get(h).cloned()).collect()
    }

    /// Check if a block exists in the cache
    pub async fn has_block(&self, hash: &HeaderHash) -> bool {
        self.blocks.read().await.contains_key(hash)
    }

    /// Remove a block from the cache
    pub async fn remove_block(&self, hash: &HeaderHash) {
        self.blocks.write().await.remove(hash);
    }

    /// Get the number of blocks in the cache
    pub async fn len(&self) -> usize {
        self.blocks.read().await.len()
    }

    /// Check if the cache is empty
    pub async fn is_empty(&self) -> bool {
        self.blocks.read().await.is_empty()
    }
}

impl Default for CheckpointSync {
    fn default() -> Self {
        Self::new()
    }
}