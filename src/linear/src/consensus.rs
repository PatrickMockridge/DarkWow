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

use std::sync::{atomic::{AtomicU32, Ordering}, Mutex};

use blake3::Hash as Blake3Hash;
use tracing::debug;

use super::{Block, LinearError, UncleBlock, Result};

/// How many recent block timestamps to keep for target adjustment.
const TIMESTAMP_WINDOW: usize = 20;

/// Fixed-point scale factor for difficulty adjustment arithmetic.
/// All ratio and adjustment calculations use integer math at this precision
/// to guarantee deterministic results across CPU architectures.
const SCALE: u64 = 1_000_000;

/// Proof-of-Work consensus engine with dynamic target adjustment.
///
/// `target` is the maximum valid hash value: `u32_le(hash[0..4]) <= target`.
/// Higher target = easier mining. `difficulty()` returns the conventional
/// difficulty measure (higher = harder), derived as `u32::MAX / target`.
///
/// Uses interior mutability (`Cell` + `Mutex`) so that `record_block` and
/// `adjust_target` can take `&self` while `LinearBlockchain` methods use
/// shared references.
pub struct PoWConsensus {
    /// Current target — `hash_u32 <= target` is valid. Higher = easier.
    target: AtomicU32,
    /// Desired seconds between blocks (configuration constant).
    target_block_time: u64,
    /// Floor — target will never drop below this (hardest possible).
    min_target: u32,
    /// Ceiling — target will never rise above this (easiest possible).
    max_target: u32,
    /// Recent block timestamps (newest last). Used by `adjust_target`.
    timestamps: Mutex<Vec<u64>>,
}

impl Clone for PoWConsensus {
    fn clone(&self) -> Self {
        Self {
            target: AtomicU32::new(self.target.load(Ordering::Relaxed)),
            target_block_time: self.target_block_time,
            min_target: self.min_target,
            max_target: self.max_target,
            timestamps: Mutex::new(self.timestamps.lock().unwrap().clone()),
        }
    }
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
            target: AtomicU32::new(initial_target),
            target_block_time,
            min_target,
            max_target,
            timestamps: Mutex::new(Vec::with_capacity(TIMESTAMP_WINDOW)),
        }
    }

    /// Current target — `hash_u32 <= target` is valid. Higher = easier.
    pub fn target(&self) -> u32 {
        self.target.load(Ordering::Relaxed)
    }

    /// Conventional difficulty (higher = harder), derived from target.
    pub fn difficulty(&self) -> u64 {
        let t = self.target.load(Ordering::Relaxed);
        if t == 0 {
            return u64::MAX;
        }
        u32::MAX as u64 / t as u64
    }

    /// Desired block interval in seconds.
    pub fn target_block_time(&self) -> u64 {
        self.target_block_time
    }

    /// Record a block timestamp for target tracking.
    pub fn record_block(&self, timestamp: u64) {
        let mut timestamps = self.timestamps.lock().unwrap();
        if timestamps.len() >= TIMESTAMP_WINDOW {
            timestamps.remove(0);
        }
        timestamps.push(timestamp);
    }

    /// Recalculate target based on recent block intervals.
    ///
    /// Uses a simple proportional controller: if blocks arrive faster than
    /// `target_block_time`, the target decreases (harder); if slower, it
    /// increases (easier). Single-step adjustments are capped at ±10%.
    ///
    /// All arithmetic uses integer fixed-point math (scale = `SCALE`)
    /// to guarantee deterministic results across CPU architectures.
    pub fn adjust_target(&self) -> u32 {
        let timestamps = self.timestamps.lock().unwrap();
        if timestamps.len() < 2 {
            return self.target.load(Ordering::Relaxed);
        }

        // Sum intervals between consecutive timestamps in the window
        let n = timestamps.len().min(10);
        let start = timestamps.len() - n;
        let mut total_interval = 0u64;
        for i in start + 1..timestamps.len() {
            total_interval +=
                timestamps[i].saturating_sub(timestamps[i - 1]);
        }
        let count = (n - 1) as u64;

        let avg_interval = if count > 0 {
            total_interval / count
        } else {
            self.target_block_time
        };

        // Fixed-point ratio: SCALE means "exactly on target".
        // > SCALE means blocks arrive too fast → need harder (lower target).
        // < SCALE means blocks arrive too slow → need easier (higher target).
        let ratio_scaled = if avg_interval == 0 {
            // Blocks are instant — make it 10% harder
            SCALE * 9 / 10
        } else {
            let r = (self.target_block_time * SCALE) / avg_interval;
            r.clamp(SCALE / 2, SCALE * 2)
        };

        // Clamp single-step change to ±10%.
        // SCALE + SCALE/10 = 1.1x (harder); SCALE - SCALE/10 = 0.9x (easier).
        let tenth = SCALE / 10;
        let adjustment = if ratio_scaled > SCALE {
            let excess = (ratio_scaled - SCALE).min(tenth);
            SCALE + excess
        } else if ratio_scaled < SCALE {
            let deficit = (SCALE - ratio_scaled).min(tenth);
            SCALE - deficit
        } else {
            SCALE
        };

        // ratio_scaled > SCALE (blocks fast) → adjustment > SCALE → target decreases (harder)
        // ratio_scaled < SCALE (blocks slow) → adjustment < SCALE → target increases (easier)
        let current = self.target.load(Ordering::Relaxed) as u64;
        let new_target = (current * SCALE / adjustment) as u32;
        let clamped = new_target.clamp(self.min_target, self.max_target);

        debug!(
            target: "consensus",
            "Target adjusted: {} → {} (avg_interval={}s, ratio_scaled={}, adjustment={})",
            current, clamped, avg_interval, ratio_scaled, adjustment
        );

        self.target.store(clamped, Ordering::Relaxed);
        clamped
    }

    /// Persist consensus state to a sled tree so difficulty survives restarts.
    pub fn save(&self, tree: &sled::Tree) -> Result<()> {
        tree.insert(b"target", &self.target.load(Ordering::Relaxed).to_le_bytes())
            .map_err(|e| LinearError::StorageError(e.to_string()))?;
        tree.insert(b"target_block_time", &self.target_block_time.to_le_bytes())
            .map_err(|e| LinearError::StorageError(e.to_string()))?;
        tree.insert(b"min_target", &self.min_target.to_le_bytes())
            .map_err(|e| LinearError::StorageError(e.to_string()))?;
        tree.insert(b"max_target", &self.max_target.to_le_bytes())
            .map_err(|e| LinearError::StorageError(e.to_string()))?;

        let ts = self.timestamps.lock().unwrap();
        if !ts.is_empty() {
            let mut data = Vec::with_capacity(ts.len() * 8);
            for t in ts.iter() {
                data.extend_from_slice(&t.to_le_bytes());
            }
            tree.insert(b"timestamps", data)
                .map_err(|e| LinearError::StorageError(e.to_string()))?;
        }
        Ok(())
    }

    /// Load consensus state from a sled tree.
    /// Values not found in storage keep their current defaults.
    pub fn load(&self, tree: &sled::Tree) -> Result<()> {
        if let Some(bytes) = tree
            .get(b"target")
            .map_err(|e| LinearError::StorageError(e.to_string()))?
        {
            if bytes.len() == 4 {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&bytes);
                self.target.store(u32::from_le_bytes(arr), Ordering::Relaxed);
            }
        }
        if let Some(bytes) = tree
            .get(b"timestamps")
            .map_err(|e| LinearError::StorageError(e.to_string()))?
        {
            let mut timestamps = self.timestamps.lock().unwrap();
            timestamps.clear();
            for chunk in bytes.chunks_exact(8) {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(chunk);
                timestamps.push(u64::from_le_bytes(arr));
            }
        }
        debug!(
            target: "consensus",
            "Loaded consensus state: target={}, {} timestamps",
            self.target.load(Ordering::Relaxed),
            self.timestamps.lock().unwrap().len()
        );
        Ok(())
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
        hash_u32 <= self.target.load(Ordering::Relaxed)
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
            0x0FFF_FFFF,    // initial target (~1 in 16 chance per hash)
            1,              // min target (hardest)
            u32::MAX,       // max target (easiest)
        )
    }
}
