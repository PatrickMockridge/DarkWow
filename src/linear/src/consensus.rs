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

//! Simple PoW consensus for linear blockchain

use blake3::Hash as Blake3Hash;

use super::{Block, UncleBlock, Result};

/// Ring buffer for storing timestamps or difficulties
/// Using simple vector-based implementation for clarity
struct RingBuffer<T> {
    data: Vec<T>,
    capacity: usize,
    index: usize,
}

impl<T: Clone> RingBuffer<T> {
    fn new(capacity: usize) -> Self {
        Self { data: Vec::with_capacity(capacity), capacity, index: 0 }
    }

    fn push(&mut self, item: T) {
        if self.data.len() < self.capacity {
            self.data.push(item);
        } else {
            self.data[self.index] = item;
            self.index = (self.index + 1) % self.capacity;
        }
    }

    fn is_full(&self) -> bool {
        self.data.len() >= self.capacity
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn get(&self, i: usize) -> Option<&T> {
        self.data.get(i)
    }

    fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }
}

/// Simple PoW consensus with dynamic difficulty adjustment
pub struct PoWConsensus {
    difficulty_target: u32,
    target_block_time: u64,      // seconds between blocks (setup parameter)
    min_difficulty: u32,          // floor difficulty
    max_difficulty: u32,         // ceiling difficulty
    timestamps: RingBuffer<u64>, // recent block timestamps
    difficulties: RingBuffer<u32>,
}

impl PoWConsensus {
    /// Create a new PoW consensus with configurable parameters
    pub fn new(target_block_time: u64, initial_difficulty: u32) -> Self {
        Self {
            difficulty_target: initial_difficulty,
            target_block_time,
            min_difficulty: 1,
            max_difficulty: u32::MAX,
            timestamps: RingBuffer::new(20),
            difficulties: RingBuffer::new(20),
        }
    }

    /// Create with full configuration
    pub fn with_config(
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
            timestamps: RingBuffer::new(20),
            difficulties: RingBuffer::new(20),
        }
    }

    /// Get the difficulty target
    pub fn difficulty_target(&self) -> u32 {
        self.difficulty_target
    }

    /// Get target block time
    pub fn target_block_time(&self) -> u64 {
        self.target_block_time
    }

    /// Record a new block's timestamp and difficulty
    pub fn record_block(&mut self, timestamp: u64, difficulty: u32) {
        self.timestamps.push(timestamp);
        self.difficulties.push(difficulty);
    }

    /// Dynamic difficulty adjustment based on actual vs target block time
    pub fn adjust_difficulty(&mut self) -> u32 {
        // Need at least 2 blocks to calculate interval
        if self.timestamps.len() < 2 {
            return self.difficulty_target;
        }

        // Calculate average block time from last N blocks
        let n = self.timestamps.len().min(10);
        let mut total_interval = 0u64;

        for i in 0..n - 1 {
            let idx1 = self.timestamps.data.len() - 1 - i;
            let idx2 = idx1.saturating_sub(1);
            if let (Some(t1), Some(t2)) = (self.timestamps.get(idx1), self.timestamps.get(idx2)) {
                total_interval += t1.saturating_sub(*t2);
            }
        }

        let avg_interval = if n > 1 {
            total_interval / (n - 1) as u64
        } else {
            self.target_block_time
        };

        // Adjust difficulty: if blocks are coming too fast, increase difficulty
        // If blocks are coming too slow, decrease difficulty
        // Change factor: max +/- 10% per adjustment
        let adjustment = if avg_interval == 0 {
            // Avoid division by zero
            1.1 // Increase difficulty when blocks are instant (likely too easy)
        } else {
            let ratio = self.target_block_time as f64 / avg_interval as f64;
            // Clamp ratio to [0.5, 2.0] range
            ratio.max(0.5).min(2.0)
        };

        // Apply adjustment (only up to 10% change per call)
        let adjustment = if adjustment > 1.0 {
            1.0 + (adjustment - 1.0).min(0.1)
        } else {
            1.0 - (1.0 - adjustment).min(0.1)
        };

        let new_difficulty = (self.difficulty_target as f64 * adjustment) as u32;

        // Clamp to min/max bounds
        self.difficulty_target = new_difficulty.max(self.min_difficulty).min(self.max_difficulty);

        self.difficulty_target
    }

    /// Verify a block meets the difficulty target using RandomX VM.
    /// For a u32 difficulty target, compares the first 4 bytes of the
    /// RandomX hash (interpreted as little-endian u32) against the target.
    /// Lower hash = more work. The u32 target is adequate for testnet
    /// since RandomX output is uniformly random — 32 bits of work per attempt.
    pub fn verify_proof(&self, block: &Block, vm: &randomx::RandomXVM) -> Result<bool> {
        let hash = block.hash(vm);
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        Ok(hash_u32 <= self.difficulty_target)
    }

    /// Check if the hash meets the difficulty target
    pub fn check_difficulty(&self, hash: &Blake3Hash) -> bool {
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        hash_u32 <= self.difficulty_target
    }

    /// Verify an uncle block meets the difficulty target
    pub fn verify_uncle_pow(&self, uncle: &UncleBlock, vm: &randomx::RandomXVM) -> Result<bool> {
        Ok(self.check_difficulty(&uncle.hash(vm)))
    }
}

impl Default for PoWConsensus {
    fn default() -> Self {
        Self {
            difficulty_target: 0x0000_FFFF,
            target_block_time: 120,      // 2 minutes between blocks
            min_difficulty: 1,
            max_difficulty: u32::MAX,
            timestamps: RingBuffer::new(20),
            difficulties: RingBuffer::new(20),
        }
    }
}