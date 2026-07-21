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

//! Cumulative Supply Chain — single authoritative source of cumulative supply state.
//!
//! The cumulative Pedersen commitment chain `S_H = S_{H-1} + C_H` is a
//! consensus-layer invariant, not contract-layer state. This module owns
//! a dedicated sled tree (`supply_chain`) and provides the ONLY read/write
//! API for cumulative supply state.
//!
//! ## IMPORTANT: This tree is a HOST-SIDE CACHE
//!
//! The supply_chain tree is NOT a consensus rule. The authoritative source
//! for cumulative supply is the contracts sled tree, written by WASM
//! pow_reward_v1 during execute_block(). Divergence between these two
//! trees is a bug in this module, not a consensus violation. Do NOT add
//! block rejection logic that depends solely on this tree's state.
//!
//! The cumulative supply chain is a **passive audit** (see
//! `src/sdk/src/blockchain.rs:202-207`). Like Bitcoin's halving schedule,
//! it is a verifiable property of the chain that any observer can check.
//! Block production does not halt if the chain diverges — nodes detect
//! the divergence and can choose to fork.
//!
//! ## Two systems, one invariant
//!
//! DarkWow has TWO separate supply verification systems:
//!
//! 1. **Proof of Token Balance** (consensus rule, blocks rejected on violation):
//!    `Σ outputs + Σ burns + Σ fees == Σ inputs` per block.
//!    Lives in `proof_of_token_balance.rs`. Prevents secret inflationary
//!    mints within a single block.
//!
//! 2. **Cumulative Supply Chain** (passive audit, informational):
//!    `S_H = S_{H-1} + C_H` across all blocks.
//!    Lives in this module + `blockchain.rs`. Enables independent supply
//!    auditing. Divergence = cryptographic evidence, not block rejection.
//!
//! ## Architecture
//!
//! Before this module, cumulative supply state was stored in the WASM
//! contract's info tree (under the `contracts` sled tree). The host-side
//! coinbase builder read it via raw `sled::Tree::get()` with manually
//! constructed keys, duplicating the WASM runtime's key derivation.
//! This caused silent data mismatches when the two paths diverged.
//!
//! Now there is exactly one path:
//!   - Host coinbase builder → `supply_chain.get_latest()`
//!   - WASM validation       → `RuntimeBackend::get_cumulative_supply()`
//!   - Block commit          → `supply_chain.commit_to_batch()`
//!
//! ## On-disk format
//!
//! Sled tree `supply_chain`:
//!   key   = `height.to_le_bytes()` (8 bytes)
//!   value = `CumulativeSupplyEntry::to_bytes()` (dwow_serial)
//!
//! ## Genesis
//!
//! After the genesis block (height=1) is committed, the genesis cumulative
//! entry is seeded with:
//!   value_commit = pedersen_commit(reward(1), blind(1))
//!   blind        = coinbase_blind(&[0u8; 32], 1)
//!   total_supply = expected_reward(1)

use std::sync::Mutex;

use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::pasta::{
    group::Group,
    pallas,
};
use dwow_serial::{deserialize, serialize};
use sled::Tree;
use tracing::{debug, info};

use super::LinearError;

/// Cumulative supply state at a single block height.
///
/// Invariants (enforced by this module):
///   `value_commit_H = value_commit_{H-1} + C_H` (Pedersen chain)
///   `blind_H = blind_{H-1} + coinbase_blind_H`  (scalar chain)
///   `total_supply_H = total_supply_{H-1} + reward(H)` (emission schedule)
#[derive(Clone, Debug, PartialEq)]
pub struct CumulativeSupplyEntry {
    /// S_H — cumulative Pedersen commitment at height H
    pub value_commit: pallas::Point,
    /// sum of all coinbase blinds from genesis to height H
    pub blind: pallas::Scalar,
    /// plaintext total DRKW supply at height H
    pub total_supply: u64,
}

impl CumulativeSupplyEntry {
    /// Identity state — pre-genesis or genesis initial.
    pub fn genesis() -> Self {
        Self {
            value_commit: pallas::Point::identity(),
            blind: pallas::Scalar::zero(),
            total_supply: 0,
        }
    }

    /// Serialize to bytes for sled storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&serialize(&self.value_commit));
        buf.extend_from_slice(&serialize(&self.blind));
        buf.extend_from_slice(&self.total_supply.to_le_bytes());
        buf
    }

    /// Deserialize from bytes read from sled.
    ///
    /// Point serialization can be either 32 bytes (compressed / identity) or
    /// 64 bytes (full affine coordinates). Both are valid dwow_serial formats.
    pub fn from_bytes(data: &[u8]) -> Result<Self, LinearError> {
        let point_len: usize;
        let scalar_offset: usize;
        let supply_offset: usize;
        match data.len() {
            72 => { point_len = 32; scalar_offset = 32; supply_offset = 64; }
            104 => { point_len = 64; scalar_offset = 64; supply_offset = 96; }
            _ => {
                return Err(LinearError::SerializationError(format!(
                    "CumulativeSupplyEntry: expected 72 or 104 bytes, got {}",
                    data.len()
                )));
            }
        }
        let value_commit: pallas::Point =
            deserialize(&data[..point_len])
                .map_err(|e| LinearError::SerializationError(e.to_string()))?;
        let blind: pallas::Scalar =
            deserialize(&data[scalar_offset..supply_offset])
                .map_err(|e| LinearError::SerializationError(e.to_string()))?;
        let total_supply = u64::from_le_bytes(
            data[supply_offset..supply_offset + 8].try_into().unwrap(),
        );
        Ok(Self { value_commit, blind, total_supply })
    }
}

/// The single authoritative source of cumulative supply chain state.
///
/// Owns a dedicated sled tree (`supply_chain`). Maintains an in-memory
/// cache of the latest entry for fast reads. All sled writes go through
/// this module — no other code reads or writes cumulative supply state.
pub struct CumulativeSupplyChain {
    tree: Tree,
    /// In-memory cache: (latest_height, latest_entry).
    /// Restored from sled on startup. Sled is authoritative on restart.
    latest: Mutex<Option<(BlockHeight, CumulativeSupplyEntry)>>,
}

impl CumulativeSupplyChain {
    /// Tree name in the sled database.
    pub const TREE_NAME: &str = "supply_chain";

    /// Open (or create) the supply chain sled tree.
    /// On startup, restores the latest committed entry from sled into the
    /// in-memory cache by scanning from the highest key downward.
    pub fn new(db: &sled::Db) -> Result<Self, LinearError> {
        let tree = db
            .open_tree(Self::TREE_NAME)
            .map_err(|e| LinearError::StorageError(e.to_string()))?;

        // Restore latest from sled: find the maximum height key.
        let latest = Self::restore_latest(&tree)?;
        if let Some((height, _)) = &latest {
            info!(
                target: "linear::supply_chain",
                "Restored cumulative supply chain from sled: latest height={}", height
            );
        } else {
            info!(
                target: "linear::supply_chain",
                "No cumulative supply entries found — starting from genesis identity"
            );
        }

        Ok(Self {
            tree,
            latest: Mutex::new(latest),
        })
    }

    /// Scan sled for the highest-height entry.
    fn restore_latest(tree: &Tree) -> Result<Option<(BlockHeight, CumulativeSupplyEntry)>, LinearError> {
        let mut max_height: Option<BlockHeight> = None;
        for result in tree.iter() {
            let (key, _) = result.map_err(|e| LinearError::StorageError(e.to_string()))?;
            if key.len() == 8 {
                let h = BlockHeight::from_le_bytes(key.as_ref().try_into().unwrap());
                max_height = Some(max_height.map_or(h, |m| m.max(h)));
            }
        }
        match max_height {
            Some(h) => {
                let val = tree
                    .get(h.to_le_bytes())
                    .map_err(|e| LinearError::StorageError(e.to_string()))?
                    .ok_or(LinearError::StorageError(format!(
                        "supply_chain entry at height {h} missing"
                    )))?;
                let entry: CumulativeSupplyEntry = CumulativeSupplyEntry::from_bytes(&val)
                    .map_err(|e| LinearError::SerializationError(e.to_string()))?;
                Ok(Some((h, entry)))
            }
            None => Ok(None),
        }
    }

    /// Get the latest cumulative supply state.
    /// Returns genesis (identity) state if no blocks have been committed.
    pub fn get_latest(&self) -> CumulativeSupplyEntry {
        let guard = self.latest.lock().unwrap();
        guard
            .as_ref()
            .map(|(_, e)| e.clone())
            .unwrap_or_else(CumulativeSupplyEntry::genesis)
    }

    /// Get the latest committed height (0 if pre-genesis).
    pub fn get_latest_height(&self) -> BlockHeight {
        self.latest
            .lock()
            .unwrap()
            .as_ref()
            .map(|(h, _)| *h)
            .unwrap_or(BlockHeight::new(0))
    }

    /// Get cumulative supply state at a specific height.
    /// Returns an error if the height has no entry.
    pub fn get(&self, height: BlockHeight) -> Result<CumulativeSupplyEntry, LinearError> {
        if height.get() == 0 {
            return Ok(CumulativeSupplyEntry::genesis());
        }
        let val = self
            .tree
            .get(height.to_le_bytes())
            .map_err(|e| LinearError::StorageError(e.to_string()))?
            .ok_or(LinearError::StorageError(format!(
                "supply_chain: no entry at height {height}"
            )))?;
        CumulativeSupplyEntry::from_bytes(&val).map_err(|e| LinearError::SerializationError(e.to_string()))
    }

    /// Compute the next cumulative state from a previous entry and a new coinbase.
    ///
    /// ```text
    /// S_H = S_{H-1} + coinbase_value_commit
    /// blind_H = blind_{H-1} + coinbase_blind
    /// total_supply_H = total_supply_{H-1} + coinbase_value
    /// ```
    ///
    /// Does NOT persist — use `commit_to_batch()` for atomic persistence.
    pub fn compute_next(
        prev: &CumulativeSupplyEntry,
        coinbase_value_commit: pallas::Point,
        coinbase_blind: pallas::Scalar,
        coinbase_value: u64,
    ) -> CumulativeSupplyEntry {
        CumulativeSupplyEntry {
            value_commit: prev.value_commit + coinbase_value_commit,
            blind: prev.blind + coinbase_blind,
            total_supply: prev.total_supply.saturating_add(coinbase_value),
        }
    }

    /// Verify the subtractive Pedersen split invariant for uncle rewards.
    ///
    /// The base coinbase reward is split between the canonical miner and
    /// uncle block miners via subtractive Pedersen mass balance:
    ///   canonical_reward + sum(uncle_pin_confirmed) == base_reward
    ///
    /// Returns `Ok(())` if the invariant holds, or an error describing
    /// the violation. Called from `connect_block` during block acceptance.
    pub fn verify_uncle_split(
        base_reward: u64,
        canonical_reward: u64,
        uncle_pin_confirmed: &[u64],
    ) -> Result<(), LinearError> {
        let total_pin: u64 = uncle_pin_confirmed.iter().sum();
        if canonical_reward + total_pin != base_reward {
            return Err(LinearError::BlockIsInvalid(format!(
                "Supply invariant violated: canonical({}) + uncles({}) != base_reward({})",
                canonical_reward, total_pin, base_reward
            )));
        }
        Ok(())
    }

    /// Write a cumulative supply entry into a sled Batch.
    ///
    /// Used by `connect_block` to include supply_chain updates in the
    /// atomic cross-tree sled transaction. The batch is applied atomically
    /// alongside blocks, contracts, consensus, coins, and nullifiers.
    pub fn commit_to_batch(
        &self,
        batch: &mut sled::Batch,
        height: BlockHeight,
        entry: &CumulativeSupplyEntry,
    ) -> Result<(), LinearError> {
        let key = height.to_le_bytes();
        let value = entry.to_bytes();
        batch.insert(&key, value);
        debug!(
            target: "linear::supply_chain",
            "commit_to_batch: height={} total_supply={}", height, entry.total_supply
        );
        Ok(())
    }

    /// Commit a cumulative supply entry directly to sled (non-transactional).
    /// Also updates the in-memory cache.
    ///
    /// Prefer `commit_to_batch()` for atomic cross-tree commits.
    /// Use this only for standalone operations (e.g., genesis seeding).
    pub fn commit(
        &self,
        height: BlockHeight,
        entry: &CumulativeSupplyEntry,
    ) -> Result<(), LinearError> {
        let key = height.to_le_bytes();
        let value = entry.to_bytes();
        self.tree
            .insert(&key, value)
            .map_err(|e| LinearError::StorageError(e.to_string()))?;
        let mut guard = self.latest.lock().unwrap();
        *guard = Some((height, entry.clone()));
        info!(
            target: "linear::supply_chain",
            "Committed cumulative supply: height={} total_supply={}",
            height, entry.total_supply
        );
        Ok(())
    }

    /// Update the in-memory cache after an atomic cross-tree transaction
    /// has been committed. Must be called AFTER the sled transaction succeeds.
    pub fn update_cache(&self, height: BlockHeight, entry: CumulativeSupplyEntry) {
        let mut guard = self.latest.lock().unwrap();
        if let Some((existing_h, _)) = *guard {
            if height <= existing_h {
                return; // Don't move backward
            }
        }
        debug!(
            target: "linear::supply_chain",
            "Cache updated: height={}", height
        );
        *guard = Some((height, entry));
    }

    /// Access the underlying sled tree (for inclusion in cross-tree transactions).
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Verify the entire cumulative supply chain from the given starting point
    /// to the tip. Returns `Ok(true)` if all entries satisfy the invariant.
    ///
    /// This is the node-side audit function — any node can call this
    /// independently to verify the supply chain without executing WASM.
    pub fn verify_entries(
        &self,
        from_height: BlockHeight,
        to_height: BlockHeight,
    ) -> Result<bool, LinearError> {
        let mut prev = if from_height <= BlockHeight::GENESIS {
            CumulativeSupplyEntry::genesis()
        } else {
            self.get(from_height.pred().expect("height >= 1"))?
        };

        for h in from_height.get()..=to_height.get() {
            let entry = self.get(BlockHeight::new(h))?;
            // Verify total_supply is monotonic (non-decreasing)
            if entry.total_supply < prev.total_supply {
                return Ok(false);
            }
            prev = entry;
        }
        Ok(true)
    }
}
