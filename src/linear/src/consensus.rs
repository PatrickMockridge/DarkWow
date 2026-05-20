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
//! A simple, inspectable difficulty-adjustment engine. Every block carries a
//! compact 227-byte mining blob (see `BlockHeader::to_mining_blob`) that is
//! hashed with RandomX. The first 4 bytes of the hash, interpreted as a
//! little-endian u32, must be `<= difficulty_target` for the block to be valid.
//!
//! Difficulty adjusts each time a block is inserted, using a sliding window of
//! the last `TIMESTAMP_WINDOW` block timestamps to estimate the current hash
//! rate. Individual adjustments are capped at ±10% to prevent oscillation.

use blake3::Hash as Blake3Hash;

use super::{Block, UncleBlock, Result};

/// How many recent block timestamps to keep for difficulty adjustment.
const TIMESTAMP_WINDOW: usize = 20;

/// Proof-of-Work consensus engine with dynamic difficulty adjustment.
pub struct PoWConsensus {
    /// Current difficulty target — `hash_u32 <= difficulty_target` is valid.
    difficulty_target: u32,
    /// Desired seconds between blocks (configuration constant).
    target_block_time: u64,
    /// Floor — difficulty will never drop below this.
    min_difficulty: u32,
    /// Ceiling — difficulty will never rise above this.
    max_difficulty: u32,
    /// Recent block timestamps (newest last). Used by `adjust_difficulty`.
    timestamps: Vec<u64>,
}

impl PoWConsensus {
    /// Create a new PoW consensus with the given parameters.
    pub fn new(
        target_block_time: u64,
        initial_difficulty: u32,
        min_difficulty: u32,
        max_difficulty: u32,
    ) -> Self {
        Self {
            difficulty_target: initial_difficulty,
            target_block_time,
            min_difficulty,
            max_difficulty,
            timestamps: Vec::with_capacity(TIMESTAMP_WINDOW),
        }
    }

    /// Current difficulty target.
    pub fn difficulty_target(&self) -> u32 {
        self.difficulty_target
    }

    /// Desired block interval in seconds.
    pub fn target_block_time(&self) -> u64 {
        self.target_block_time
    }

    /// Record a block timestamp for difficulty tracking.
    pub fn record_block(&mut self, timestamp: u64) {
        if self.timestamps.len() >= TIMESTAMP_WINDOW {
            self.timestamps.remove(0);
        }
        self.timestamps.push(timestamp);
    }

    /// Recalculate difficulty based on recent block intervals.
    ///
    /// Uses a simple proportional controller: if blocks arrive faster than
    /// `target_block_time`, difficulty increases; if slower, it decreases.
    /// Single-step adjustments are capped at ±10% to prevent oscillation.
    pub fn adjust_difficulty(&mut self) -> u32 {
        if self.timestamps.len() < 2 {
            return self.difficulty_target;
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
            1.1 // blocks are instant — difficulty is too low
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

        let new_difficulty = (self.difficulty_target as f64 * adjustment) as u32;
        self.difficulty_target = new_difficulty.clamp(self.min_difficulty, self.max_difficulty);

        self.difficulty_target
    }

    /// Verify a block's RandomX hash meets the difficulty target.
    pub fn verify_proof(&self, block: &Block, vm: &randomx::RandomXVM) -> Result<bool> {
        let hash = block.hash(vm);
        Ok(self.check_difficulty(&hash))
    }

    /// Check whether a pre-computed hash meets the difficulty target.
    pub fn check_difficulty(&self, hash: &Blake3Hash) -> bool {
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        hash_u32 <= self.difficulty_target
    }

    /// Verify an uncle block meets the difficulty target.
    pub fn verify_uncle_pow(&self, uncle: &UncleBlock, vm: &randomx::RandomXVM) -> Result<bool> {
        Ok(self.check_difficulty(&uncle.hash(vm)))
    }
}

impl Default for PoWConsensus {
    fn default() -> Self {
        Self::new(
            120,            // 2-minute target block time
            0x0000_FFFF,    // initial difficulty (1 in ~65k chance per hash)
            1,              // min difficulty
            u32::MAX,       // max difficulty
        )
    }
}
