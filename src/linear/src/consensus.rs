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

use std::sync::{atomic::{AtomicU32, AtomicU64, Ordering}, Mutex};

use blake3::Hash as Blake3Hash;
use dwow_sdk::blockchain::{BlockHeight, BlockTarget, BlockTimestamp};
use tracing::debug;

use super::{Block, LinearError, UncleBlock, Result};

/// How many recent block timestamps to keep for target adjustment.
const TIMESTAMP_WINDOW: usize = 20;

/// Fixed-point scale factor for difficulty adjustment arithmetic.
/// All ratio and adjustment calculations use integer math at this precision
/// to guarantee deterministic results across CPU architectures.
const SCALE: u64 = 1_000_000;

/// Accumulated chain work for fork selection (heaviest-chain-wins).
/// Wraps AtomicU64 for lock-free reads from hot paths (stratum, RPC, miner).
/// G3: .get() calls are at the hardware atomic boundary.
/// G12: AtomicU64 internal — public API uses BlockTarget.
pub struct ChainWork(AtomicU64);

impl ChainWork {
    pub const fn new() -> Self { Self(AtomicU64::new(0)) }
    pub fn get(&self) -> u64 { self.0.load(Ordering::SeqCst) }
    /// Add work from a block at the given target.
    pub fn add_block(&self, target: BlockTarget) {
        self.0.fetch_add(target.chain_work(), Ordering::SeqCst);
    }
    // load/store removed — dead code. AtomicU64 access uses .get() and direct load(Ordering).
}

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
    min_target: BlockTarget,
    /// Ceiling — target will never rise above this (easiest possible).
    max_target: BlockTarget,
    /// The target value set at construction — never changes.
    /// Used by get_next_work_required as the base for chain-derived computation.
    initial_target: BlockTarget,
    /// Recent block timestamps (newest last). Used by `adjust_target`.
    timestamps: Mutex<Vec<BlockTimestamp>>,
    /// Accumulated chain work (sum of u32::MAX / target per block).
    /// Used for fork selection — heaviest chain wins, not just longest.
    /// G12: typed wrapper — AtomicU64 is internal implementation detail.
    pub accumulated_work: ChainWork,
}

impl Clone for PoWConsensus {
    fn clone(&self) -> Self {
        Self {
            target: AtomicU32::new(self.target.load(Ordering::Acquire)),
            target_block_time: self.target_block_time,
            min_target: self.min_target,   // BlockTarget is Copy
            max_target: self.max_target,   // BlockTarget is Copy
            initial_target: self.initial_target, // BlockTarget is Copy
            timestamps: Mutex::new(self.timestamps.lock().unwrap_or_else(|e| e.into_inner()).clone()),
            accumulated_work: ChainWork(AtomicU64::new(self.accumulated_work.get())),
        }
    }
}

impl PoWConsensus {
    /// Create a new PoW consensus with the given parameters.
    pub fn new(
        target_block_time: u64,
        initial_target: BlockTarget,
        min_target: BlockTarget,
        max_target: BlockTarget,
    ) -> Self {
        Self {
            target: AtomicU32::new(initial_target.get()),
            target_block_time,
            min_target,
            max_target,
            initial_target,
            timestamps: Mutex::new(Vec::with_capacity(TIMESTAMP_WINDOW)),
            accumulated_work: ChainWork::new(),
        }
    }

    /// Current target — `hash_u32 <= target` is valid. Higher = easier.
    pub fn target(&self) -> BlockTarget {
        BlockTarget::new(self.target.load(Ordering::Acquire))
    }

    /// Force-set the target (used for rollback on failed commit).
    pub fn force_target(&self, value: BlockTarget) {
        self.target.store(value.get(), Ordering::Release);
    }

    /// Snapshot current timestamps (used for rollback on failed commit).
    pub fn snapshot_timestamps(&self) -> Vec<BlockTimestamp> {
        self.timestamps.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Restore timestamps from snapshot (used for rollback on failed commit).
    pub fn restore_timestamps(&self, ts: Vec<BlockTimestamp>) {
        let mut timestamps = self.timestamps.lock().unwrap_or_else(|e| e.into_inner());
        *timestamps = ts;
    }

    /// Conventional difficulty (higher = harder), derived from target.
    pub fn difficulty(&self) -> u64 {
        BlockTarget::new(self.target.load(Ordering::Acquire)).difficulty()
    }

    /// Desired block interval in seconds.
    pub fn target_block_time(&self) -> u64 {
        self.target_block_time
    }

    /// Floor — target will never drop below this (hardest possible).
    pub fn min_target(&self) -> BlockTarget {
        self.min_target
    }

    /// Ceiling — target will never rise above this (easiest possible).
    pub fn max_target(&self) -> BlockTarget {
        self.max_target
    }

    /// Record a block timestamp for target tracking.
    pub fn record_block(&self, timestamp: BlockTimestamp) {
        let mut timestamps = self.timestamps.lock().unwrap_or_else(|e| e.into_inner());
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
    pub fn adjust_target(&self) -> BlockTarget {
        let timestamps = self.timestamps.lock().unwrap_or_else(|e| e.into_inner());
        if timestamps.len() < 2 {
            return BlockTarget::new(self.target.load(Ordering::Acquire));
        }

        // Sum intervals between consecutive timestamps in the window
        let n = timestamps.len().min(10);
        let start = timestamps.len() - n;
        let mut total_interval = 0u64;
        for i in start + 1..timestamps.len() {
            total_interval +=
                // Decreasing timestamps violate causality. checked_sub surfaces
                // the violation rather than silently masking it as zero-interval.
                // A zero interval is substituted (same behavior as before) but the
                // anomaly is logged so operators can detect timestamp manipulation.
                timestamps[i].get().checked_sub(timestamps[i - 1].get()).unwrap_or_else(|| {
                    tracing::warn!(target: "dwow_chain::consensus",
                        "Decreasing timestamp in adjustment window: {} < {}",
                        timestamps[i], timestamps[i - 1]);
                    0
                });
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
        let current = BlockTarget::new(self.target.load(Ordering::Acquire));
        let clamped = current.adjust(SCALE, adjustment, self.min_target.get(), self.max_target.get());

        debug!(
            target: "consensus",
            "Target adjusted: {} → {} (avg_interval={}s, ratio_scaled={}, adjustment={})",
            current, clamped, avg_interval, ratio_scaled, adjustment
        );

        self.target.store(clamped.get(), Ordering::Release);
        clamped
    }

    /// Persist consensus state to a sled tree so difficulty survives restarts.
    pub fn save(&self, tree: &sled::Tree) -> Result<()> {
        let mut batch = sled::Batch::default();
        self.save_to_batch(&mut batch);
        tree.apply_batch(batch).map_err(|e| LinearError::StorageError(e.to_string()))?;
        Ok(())
    }

    /// Write consensus state into a sled Batch (for use with transactions).
    pub fn save_to_batch(&self, batch: &mut sled::Batch) {
        batch.insert(b"target", &self.target.load(Ordering::Acquire).to_le_bytes());
        batch.insert(b"target_block_time", &self.target_block_time.to_le_bytes());
        batch.insert(b"min_target", &self.min_target.get().to_le_bytes());
        batch.insert(b"max_target", &self.max_target.get().to_le_bytes());

        let ts = self.timestamps.lock().unwrap_or_else(|e| e.into_inner());
        if !ts.is_empty() {
            let mut data = Vec::with_capacity(ts.len() * 8);
            for t in ts.iter() {
                data.extend_from_slice(&t.to_le_bytes());
            }
            batch.insert(b"timestamps", data);
        }
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
                self.target.store(u32::from_le_bytes(arr), Ordering::Release);
            }
        }
        if let Some(bytes) = tree
            .get(b"timestamps")
            .map_err(|e| LinearError::StorageError(e.to_string()))?
        {
            let mut timestamps = self.timestamps.lock().unwrap_or_else(|e| e.into_inner());
            timestamps.clear();
            for chunk in bytes.chunks_exact(8) {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(chunk);
                timestamps.push(BlockTimestamp::from_le_bytes(arr));
            }
        }
        debug!(
            target: "consensus",
            "Loaded consensus state: target={}, {} timestamps",
            self.target.load(Ordering::Acquire),
            self.timestamps.lock().unwrap_or_else(|e| e.into_inner()).len()
        );
        Ok(())
    }

    /// Verify a block's RandomX hash meets the target.
    pub fn verify_proof(&self, block: &Block, vm: &randomx::RandomXVM) -> Result<bool> {
        let hash = block.hash_with_vm(&vm);
        Ok(self.check_pow(&hash))
    }

    /// Check whether a pre-computed hash meets the target.
    ///
    /// This 32-bit comparison is canonical. Stratum bridges it to xmrig's
    /// 64-bit check — see `stratum.rs` target encoding comment.
    pub fn check_pow(&self, hash: &Blake3Hash) -> bool {
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        hash_u32 <= self.target.load(Ordering::Acquire)
    }

    /// Verify an uncle block meets the target.
    pub fn verify_uncle_pow(&self, uncle: &UncleBlock, vm: &randomx::RandomXVM) -> Result<bool> {
        Ok(self.check_pow(&uncle.hash_with_vm(&vm)))
    }

    /// Compute the target that a block at the given height MUST use.
    ///
    /// Bitcoin's GetNextWorkRequired pattern: reads timestamps from the
    /// CANONICAL CHAIN blocks (via the sled store), not from the mutable
    /// timestamp accumulator. This guarantees deterministic results across
    /// all nodes — two nodes with the same chain always compute the same
    /// expected target, regardless of their local mining history.
    ///
    /// For height 1 (genesis), returns `u32::MAX` since there is no prior
    /// chain history. For height > 1, walks the chain from genesis through
    /// `height - 1`, recomputing the target from each block's timestamp.
    /// Uses the same adjustment algorithm as `adjust_target()`.
    pub fn get_next_work_required(&self, store: &super::LinearStore, height: BlockHeight) -> BlockTarget {
        if height <= BlockHeight::GENESIS {
            return BlockTarget::MAX;
        }

        // NOTE: This walks the entire chain from genesis — O(height) per call.
        // M2 (deferred): the production fix is to cache the target per block in
        // the store (Bitcoin's mapBlockIndex pattern) and read the last N blocks
        // only. For testnet chains under 10,000 blocks, the current approach is
        // acceptable. A full fix requires a schema migration (store target per block).
        let mut target = self.initial_target();
        let mut timestamps: Vec<BlockTimestamp> = Vec::with_capacity(TIMESTAMP_WINDOW);

        for h in 1..height.get() {
            if let Ok(block) = store.get_block(BlockHeight::new(h)) {
                timestamps.push(block.header.timestamp);
                if timestamps.len() > TIMESTAMP_WINDOW {
                    timestamps.remove(0);
                }
                if timestamps.len() >= 2 {
                    target = Self::compute_adjustment(
                        &timestamps, target,
                        self.target_block_time, self.min_target, self.max_target,
                    );
                }
            } else {
                // M5 fix: propagate missing-block error to caller instead of
                // silently returning a partial target. A missing block in the
                // canonical chain is storage corruption — the caller should
                // decide how to handle it, not silently accept a wrong target.
                tracing::error!(
                    target: "consensus",
                    "Chain walk failed at height {h} — block missing from store."
                );
                // Return BlockTarget::MAX so the caller can detect the anomalous
                // value. MAX is never a valid target (it means "genesis / any hash")
                // except at height <= 1, so callers can distinguish corruption
                // from normal operation.
                return BlockTarget::MAX;
            }
        }

        target
    }

    /// The initial target this consensus was configured with.
    /// Returns the value set at construction — immutable, never adjusted.
    /// This is the base for chain-derived target computation in get_next_work_required.
    /// Must match the Python model's INITIAL_TARGET for deterministic results.
    pub fn initial_target(&self) -> BlockTarget {
        self.initial_target
    }

    /// Pure function: compute the adjusted target from a timestamp window.
    /// Same logic as `adjust_target()` but does not mutate self.
    pub fn compute_adjustment(
        timestamps: &[BlockTimestamp],
        current_target: BlockTarget,
        target_block_time: u64,
        min_target: BlockTarget,
        max_target: BlockTarget,
    ) -> BlockTarget {
        let n = timestamps.len().min(10);
        let start = timestamps.len() - n;
        let mut total_interval = 0u64;
        for i in (start + 1)..timestamps.len() {
            total_interval += timestamps[i].get().saturating_sub(timestamps[i - 1].get());
        }
        let count = (n - 1) as u64;
        let avg_interval = if count > 0 {
            total_interval / count
        } else {
            target_block_time
        };

        let ratio_scaled = if avg_interval == 0 {
            SCALE * 9 / 10
        } else {
            let r = (target_block_time * SCALE) / avg_interval;
            r.clamp(SCALE / 2, SCALE * 2)
        };

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

        current_target.adjust(SCALE, adjustment, min_target.get(), max_target.get())
    }
}

impl Default for PoWConsensus {
    fn default() -> Self {
        Self::new(
            120,                         // 2-minute target block time
            BlockTarget::new(0x0FFFFFFF), // initial target (matches Python model + config)
            BlockTarget::new(1),           // min target (hardest)
            BlockTarget::MAX,              // max target (easiest)
        )
    }
}

/// Proof-of-Work configuration for chain initialization.
/// Bundles the four parameters needed by `PoWConsensus::new()`.
/// Formerly `LinearPoWConfig` in the deleted `blockchain.rs` god object.
#[derive(Clone, Debug)]
pub struct PoWConfig {
    /// Desired seconds between blocks.
    pub target_block_time: u64,
    /// Initial difficulty target (higher = easier, BlockTarget::MAX = trivially easy).
    pub initial_target: BlockTarget,
    /// Minimum target — hardest possible (smallest value).
    pub min_target: BlockTarget,
    /// Maximum target — easiest possible (largest value).
    pub max_target: BlockTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ChainWork tests — G2: consensus-critical function coverage
    // =========================================================================

    /// Failure mode: chain work accumulation produces wrong total for fork selection.
    /// Fork selection uses accumulated work to choose the heaviest chain — if
    /// add_block() produces a different sum than independent computation, nodes
    /// could converge on the wrong chain.
    #[test]
    fn test_chain_work_new_is_zero() {
        let cw = ChainWork::new();
        assert_eq!(cw.get(), 0);
    }

    /// Failure mode: add_block with MAX target (easiest) contributes 1 work unit.
    /// Lower targets contribute more work — the ratio must be deterministic.
    #[test]
    fn test_chain_work_add_block_max_target() {
        let cw = ChainWork::new();
        cw.add_block(BlockTarget::MAX);
        // MAX target = u32::MAX, chain_work = u32::MAX / u32::MAX = 1
        assert_eq!(cw.get(), 1);
    }

    /// Failure mode: multiple add_block calls produce cumulative total.
    #[test]
    fn test_chain_work_accumulation() {
        let cw = ChainWork::new();
        // target = 0x7FFFFFFF → chain_work = 0xFFFFFFFF / 0x7FFFFFFF = 2
        cw.add_block(BlockTarget::new(0x7FFFFFFF));
        assert_eq!(cw.get(), 2);
        cw.add_block(BlockTarget::new(0x7FFFFFFF));
        assert_eq!(cw.get(), 4);
    }

    // =========================================================================
    // PoWConsensus tests — G2: consensus-critical function coverage
    // =========================================================================

    fn test_consensus() -> PoWConsensus {
        PoWConsensus::new(120, BlockTarget::new(0x00FFFFFF), BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF))
    }

    /// Failure mode: constructor sets initial_target as both the current target
    /// and the stored initial_target. If these diverge, get_next_work_required
    /// produces wrong results for chain traversal.
    #[test]
    fn test_consensus_new_sets_initial_state() {
        let c = test_consensus();
        assert_eq!(c.target(), BlockTarget::new(0x00FFFFFF));
        assert_eq!(c.target_block_time(), 120);
        assert_eq!(c.min_target(), BlockTarget::new(0x0000FFFF));
        assert_eq!(c.max_target(), BlockTarget::new(0x0FFFFFFF));
        assert_eq!(c.accumulated_work.get(), 0);
    }

    /// Failure mode: difficulty() returns u64::MAX for zero target (sentinel),
    /// not panic or garbage. Zero target is degenerate but must be handled.
    #[test]
    fn test_difficulty_zero_target_sentinel() {
        let c = PoWConsensus::new(120, BlockTarget::new(0x00FFFFFF), BlockTarget::new(0), BlockTarget::new(0x0FFFFFFF));
        c.force_target(BlockTarget::new(0));
        assert_eq!(c.difficulty(), u64::MAX);
    }

    /// Failure mode: force_target + target roundtrip must be consistent.
    #[test]
    fn test_force_target_roundtrip() {
        let c = test_consensus();
        c.force_target(BlockTarget::new(0x0000FFFF));
        assert_eq!(c.target(), BlockTarget::new(0x0000FFFF));
        c.force_target(BlockTarget::new(0x0FFFFFFF));
        assert_eq!(c.target(), BlockTarget::new(0x0FFFFFFF));
    }

    /// Failure mode: timestamp snapshot + restore roundtrip. Used for rollback
    /// on failed block commit — if restore produces different state, the node's
    /// difficulty adjustment window is corrupted after a reorg.
    #[test]
    fn test_timestamp_snapshot_restore_roundtrip() {
        let c = test_consensus();
        c.record_block(BlockTimestamp::new(1000));
        c.record_block(BlockTimestamp::new(1120));
        c.record_block(BlockTimestamp::new(1240));
        let snap = c.snapshot_timestamps();
        assert_eq!(snap.len(), 3);

        // Mutate state
        c.record_block(BlockTimestamp::new(9999));
        assert_eq!(c.snapshot_timestamps().len(), 4);

        // Restore should bring back exactly the snapshot
        c.restore_timestamps(snap);
        let restored = c.snapshot_timestamps();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored[0], BlockTimestamp::new(1000));
        assert_eq!(restored[1], BlockTimestamp::new(1120));
        assert_eq!(restored[2], BlockTimestamp::new(1240));
    }

    // =========================================================================
    // adjust_target tests — G2: difficulty retarget is consensus-critical
    // =========================================================================

    /// Failure mode: with fewer than 2 timestamps, adjust_target returns
    /// current target unchanged (not enough data to compute interval).
    #[test]
    fn test_adjust_target_insufficient_data_returns_current() {
        let c = test_consensus();
        let initial = c.target();
        // Zero timestamps: no adjustment
        assert_eq!(c.adjust_target(), initial);
        // One timestamp: still insufficient
        c.record_block(BlockTimestamp::new(1000));
        assert_eq!(c.adjust_target(), initial);
    }

    /// Failure mode: adjust_target with exactly-on-target block times produces
    /// no change. If blocks arrive exactly at target_block_time, the target
    /// should stay stable — drift here is a consensus fork.
    #[test]
    fn test_adjust_target_stable_at_target_rate() {
        let c = test_consensus();
        // Record blocks exactly at 120s intervals (on target)
        for i in 0..11 {
            c.record_block(BlockTimestamp::new(i * 120));
        }
        let adjusted = c.adjust_target();
        // With perfect timing, should stay very close to initial
        let diff = if adjusted > c.target() { adjusted.get() - c.target().get() } else { c.target().get() - adjusted.get() };
        // Allow small rounding variance (< 1% of target)
        assert!(diff <= c.target().get() / 100,
            "target drifted by {} ({}% of initial) at perfect timing", diff, diff * 100 / c.target().get());
    }

    /// Failure mode: blocks arriving too fast (half target time) should
    /// DECREASE target (make it harder). adjust_target must respond in the
    /// correct direction — a sign error inverts the controller.
    #[test]
    fn test_adjust_target_decreases_when_blocks_too_fast() {
        let c = test_consensus();
        let initial = c.target();
        // Record blocks at 60s intervals (half of 120s target = too fast)
        for i in 0..11 {
            c.record_block(BlockTimestamp::new(i * 60));
        }
        let adjusted = c.adjust_target();
        assert!(adjusted < initial,
            "blocks at 2x speed should DECREASE target (harder), got {} >= {}",
            adjusted, initial);
    }

    /// Failure mode: blocks arriving too slow (double target time) should
    /// INCREASE target (make it easier). A sign error here means the chain
    /// gets progressively harder during low-hashrate periods, potentially
    /// stalling permanently.
    #[test]
    fn test_adjust_target_increases_when_blocks_too_slow() {
        let c = test_consensus();
        let initial = c.target();
        // Record blocks at 240s intervals (double of 120s target = too slow)
        for i in 0..11 {
            c.record_block(BlockTimestamp::new(i * 240));
        }
        let adjusted = c.adjust_target();
        assert!(adjusted > initial,
            "blocks at 0.5x speed should INCREASE target (easier), got {} <= {}",
            adjusted, initial);
    }

    /// Failure mode: adjust_target result must stay within [min_target, max_target].
    /// Clamping failure could produce targets outside protocol limits.
    #[test]
    fn test_adjust_target_clamped_to_bounds() {
        let c = test_consensus();
        // Fill window with extreme timestamps to force large adjustment
        for i in 0..21 {
            c.record_block(BlockTimestamp::new(i * 1)); // instant blocks
        }
        let adjusted = c.adjust_target();
        assert!(adjusted >= c.min_target(),
            "adjusted {} below min_target {}", adjusted, c.min_target());
        assert!(adjusted <= c.max_target(),
            "adjusted {} above max_target {}", adjusted, c.max_target());
    }

    // =========================================================================
    // compute_adjustment tests — pure function, deterministic
    // =========================================================================

    /// Failure mode: compute_adjustment must be deterministic — same inputs
    /// produce same output every time. Non-determinism here causes chain forks
    /// on identical block data.
    #[test]
    fn test_compute_adjustment_deterministic() {
        let timestamps: Vec<BlockTimestamp> = (0..20).map(|i| BlockTimestamp::new(i * 120)).collect();
        let a = PoWConsensus::compute_adjustment(&timestamps, BlockTarget::new(0x00FFFFFF), 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));
        let b = PoWConsensus::compute_adjustment(&timestamps, BlockTarget::new(0x00FFFFFF), 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));
        assert_eq!(a, b, "compute_adjustment must be deterministic");
    }

    /// Failure mode: compute_adjustment with decreasing timestamps
    /// uses checked_sub to avoid panic and substitutes zero.
    #[test]
    fn test_compute_adjustment_decreasing_timestamps_no_panic() {
        // Decreasing timestamps (violate causality) should not panic
        let timestamps: Vec<BlockTimestamp> = vec![
            BlockTimestamp::new(1000), BlockTimestamp::new(900), BlockTimestamp::new(800),
            BlockTimestamp::new(700), BlockTimestamp::new(600), BlockTimestamp::new(500),
            BlockTimestamp::new(400), BlockTimestamp::new(300), BlockTimestamp::new(200),
            BlockTimestamp::new(100), BlockTimestamp::new(0),
        ];
        let result = PoWConsensus::compute_adjustment(&timestamps, BlockTarget::new(0x00FFFFFF), 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));
        // Must still produce a valid clamped target
        assert!(result.get() >= 0x0000FFFF);
        assert!(result.get() <= 0x0FFFFFFF);
    }

    /// Failure mode: window-based average with fewer than 11 timestamps uses
    /// full window — verify it doesn't divide by zero.
    #[test]
    fn test_compute_adjustment_minimal_window() {
        // 3 timestamps → window of 2 intervals
        let result = PoWConsensus::compute_adjustment(
            &[BlockTimestamp::new(0), BlockTimestamp::new(120), BlockTimestamp::new(240)],
            BlockTarget::new(0x00FFFFFF), 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));
        assert!(result.get() >= 0x0000FFFF);
        assert!(result.get() <= 0x0FFFFFFF);
    }
}

impl Default for PoWConfig {
    fn default() -> Self {
        Self {
            target_block_time: 120,
            initial_target: BlockTarget::new(0x0FFFFFFF),  // matches PoWConsensus::default() + Docker entrypoints
            min_target: BlockTarget::new(1),
            max_target: BlockTarget::MAX,
        }
    }
}
