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

//! Proof-of-Work consensus for the linear blockchain.
//!
//! Every block carries a 227-byte mining blob (see `BlockHeader::to_mining_blob`)
//! that is hashed with RandomX. The first 4 bytes of the hash, interpreted as a
//! little-endian u32, must be `<= target` for the block to be valid.
//! Higher target = easier mining (more hashes pass).
//!
//! The target adjusts each time a block is inserted, using a sliding window of
//! the last `TIMESTAMP_WINDOW` block timestamps to estimate the current hash
//! rate. Individual adjustments are capped at ±10% to prevent oscillation.
//! When blocks come too fast the target decreases (harder); when too slow it
//! increases (easier).

use blake3::Hash as Blake3Hash;

use super::{Block, UncleBlock, Result};

/// How many recent block timestamps to keep for target adjustment.
const TIMESTAMP_WINDOW: usize = 20;

/// Proof-of-Work consensus engine with dynamic target adjustment.
///
/// `target` is the maximum valid hash value: `u32_le(hash[0..4]) <= target`.
/// Higher target = easier mining. `difficulty()` returns the conventional
/// difficulty measure (higher = harder), derived as `u32::MAX / target`.
#[derive(Clone)]
pub struct PoWConsensus {
    /// Current target — `hash_u32 <= target` is valid. Higher = easier.
    target: u32,
    /// Desired seconds between blocks (configuration constant).
    target_block_time: u64,
    /// Floor — target will never drop below this (hardest possible).
    min_target: u32,
    /// Ceiling — target will never rise above this (easiest possible).
    max_target: u32,
    /// Recent block timestamps (newest last). Used by `adjust_target`.
    timestamps: Vec<u64>,
}

impl PoWConsensus {
    /// Create a new PoW consensus with the given parameters.
    pub fn new(
        target_block_time: u64,
        initial_target: u32,
        min_target: u32,
        max_target: u32,
    ) -> Self {
        Self {
            target: initial_target,
            target_block_time,
            min_target,
            max_target,
            timestamps: Vec::with_capacity(TIMESTAMP_WINDOW),
        }
    }

    /// Current target — `hash_u32 <= target` is valid. Higher = easier.
    pub fn target(&self) -> u32 {
        self.target
    }

    /// Conventional difficulty (higher = harder), derived from target.
    pub fn difficulty(&self) -> u64 {
        if self.target == 0 {
            return u64::MAX;
        }
        u32::MAX as u64 / self.target as u64
    }

    /// Desired block interval in seconds.
    pub fn target_block_time(&self) -> u64 {
        self.target_block_time
    }

    /// Record a block timestamp for target tracking.
    pub fn record_block(&mut self, timestamp: u64) {
        if self.timestamps.len() >= TIMESTAMP_WINDOW {
            self.timestamps.remove(0);
        }
        self.timestamps.push(timestamp);
    }

    /// Recalculate target based on recent block intervals.
    ///
    /// Uses a simple proportional controller: if blocks arrive faster than
    /// `target_block_time`, the target decreases (harder); if slower, it
    /// increases (easier). Single-step adjustments are capped at ±10%.
    pub fn adjust_target(&mut self) -> u32 {
        if self.timestamps.len() < 2 {
            return self.target;
        }

        // Sum intervals between consecutive timestamps in the window
        let n = self.timestamps.len().min(10);
        let start = self.timestamps.len() - n;
        let mut total_interval = 0u64;
        for i in start + 1..self.timestamps.len() {
            total_interval +=
                self.timestamps[i].saturating_sub(self.timestamps[i - 1]);
        }
        let count = (n - 1) as u64;

        let avg_interval = if count > 0 {
            total_interval / count
        } else {
            self.target_block_time
        };

        let ratio = if avg_interval == 0 {
            0.9 // blocks are instant — make it harder
        } else {
            let r = self.target_block_time as f64 / avg_interval as f64;
            r.clamp(0.5, 2.0)
        };

        // Clamp single-step change to ±10%
        let adjustment = if ratio > 1.0 {
            1.0 + (ratio - 1.0).min(0.1)
        } else {
            1.0 - (1.0 - ratio).min(0.1)
        };

        // Divide: ratio > 1 (blocks fast) → target decreases (harder)
        //        ratio < 1 (blocks slow) → target increases (easier)
        let new_target = (self.target as f64 / adjustment) as u32;
        self.target = new_target.clamp(self.min_target, self.max_target);

        self.target
    }

    /// Verify a block's RandomX hash meets the target.
    pub fn verify_proof(&self, block: &Block, vm: &randomx::RandomXVM) -> Result<bool> {
        let hash = block.hash(vm);
        Ok(self.check_pow(&hash))
    }

    /// Check whether a pre-computed hash meets the target.
    ///
    /// This 32-bit comparison is canonical. Stratum bridges it to xmrig's
    /// 64-bit check — see `stratum.rs` target encoding comment.
    pub fn check_pow(&self, hash: &Blake3Hash) -> bool {
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        hash_u32 <= self.target
    }

    /// Verify an uncle block meets the target.
    pub fn verify_uncle_pow(&self, uncle: &UncleBlock, vm: &randomx::RandomXVM) -> Result<bool> {
        Ok(self.check_pow(&uncle.hash(vm)))
    }
}

impl Default for PoWConsensus {
    fn default() -> Self {
        Self::new(
            120,            // 2-minute target block time
            0x0000_FFFF,    // initial target (1 in ~65k chance per hash)
            1,              // min target (hardest)
            u32::MAX,       // max target (easiest)
        )
    }
}
