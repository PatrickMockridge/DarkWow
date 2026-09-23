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
//! Every block carries a 292-byte mining blob (see `BlockHeader::to_mining_blob`;
//! it was 260 until `anchor_owner` was appended, `OBL-C64`)
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
use dwow_sdk::blockchain::{BlockHeight, BlockTarget, BlockTimestamp};
use tracing::debug;

use super::{Block, LinearError, Result};

/// How many recent block timestamps to keep for target adjustment.
const TIMESTAMP_WINDOW: usize = 20;

/// Fixed-point scale factor for difficulty adjustment arithmetic.
/// All ratio and adjustment calculations use integer math at this precision
/// to guarantee deterministic results across CPU architectures.
const SCALE: u64 = 1_000_000;

/// Accumulated chain work for fork selection (heaviest-chain-wins).
/// Closes: M1 (chain work wraps at u64 boundary — 2^64 / u32::MAX ≈ 4.3B
/// blocks, or ~16,350 years at 120s/block, but an accumulator that CAN
/// overflow is a ticking time bomb). Enforces: defense-in-depth.
/// u128 eliminates practical overflow: 2^128 / u32::MAX ≈ 10^29 blocks.
/// Mutex replaces AtomicU64 (no AtomicU128 on stable).
pub struct ChainWork(Mutex<u128>);

impl ChainWork {
    pub fn new() -> Self { Self(Mutex::new(0)) }
    pub fn get(&self) -> u128 { *self.0.lock().unwrap_or_else(|e| e.into_inner()) }
    /// Add work from a block at the given target.
    pub fn add_block(&self, target: BlockTarget) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) += target.chain_work();
    }
    // P2-9 dead-code sweep: ChainWork::sub_block deleted — zero callers
    // (disconnect_block does `saturating_sub` inline at chain_state.rs).
    pub(crate) fn store(&self, val: u128) { *self.0.lock().unwrap_or_else(|e| e.into_inner()) = val; }
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
            accumulated_work: ChainWork(Mutex::new(self.accumulated_work.get())),
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
            // spec dispensation: type-system.md §2.3 — BlockTarget stored in
            // AtomicU32 for lock-free concurrent read. get() extracts the u32 for
            // AtomicU32 storage. PoW comparisons use hash_is_valid().
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
            let Some(interval) = timestamps[i].get().checked_sub(timestamps[i - 1].get()) else {
                // A decreasing pair violates causality — and substituting `0` for the interval, which
                // this did until 2026-09-22 (`OBL-C37`), feeds the adjustment a value no chain can
                // produce. It is not the substitution that pays the miner, though: the fabricated zero
                // drags `avg_interval` *down*, which pushes `ratio_scaled` **up**, i.e. *harder*. What paid
                // the miner was the separate all-zero-window branch, which answered `SCALE * 9 / 10`,
                // below `SCALE`, and so eased the target ~11% (corrected 2026-09-23; the causal chain
                // written here when this branch landed had the direction of that middle step backwards).
                // Either way the input is impossible, the anomaly was logged and then acted upon, and so
                // the adjustment is **abandoned** instead: the target stays exactly where it was, which is
                // the conservative answer and the only one a miner cannot profit from.
                //
                // Reachable, not merely defensive: `validation::check_block_timestamp` requires a
                // timestamp above the *median* of the last 11, which does not exclude one below its own
                // parent — a parent above the median with a child between the two passes validation.
                tracing::warn!(target: "dwow_chain::consensus",
                    "Decreasing timestamp in adjustment window: {} < {} — leaving the target unchanged",
                    timestamps[i], timestamps[i - 1]);
                return BlockTarget::new(self.target.load(Ordering::Acquire));
            };
            total_interval += interval;
        }
        let count = (n - 1) as u64;

        let avg_interval = if count > 0 {
            total_interval / count
        } else {
            self.target_block_time
        };

        // A window whose *every* interval is zero is not a fast window — it is a window no valid chain
        // produces, so there is nothing to steer toward and the adjustment is **abandoned** (`OBL-C37`).
        //
        // This branch used to answer `ratio_scaled = SCALE * 9 / 10` under the comment "Blocks are
        // instant — make it 10% harder". Both halves were wrong. The direction: `adjust` computes
        // `current * scale / adjustment`, so `ratio_scaled < SCALE` yields `adjustment = 0.9·SCALE` and a
        // target **11% higher** — *easier*, the opposite of the comment, and the trade a miner would
        // engineer a zero interval to get. The premise: `count > 0` here, so `avg_interval == 0` means
        // all `n-1` intervals in the window are zero, i.e. ten consecutive equal timestamps — and
        // `validation::check_block_timestamp` requires each timestamp to exceed the median of the last
        // 11, which admits at most five equal in a row before the median *is* that value. So the state is
        // unreachable from a chain that validated, and a branch that pays 11% for reaching it is a
        // standing invitation to widen one of those two rules later. Abandoning is deterministic,
        // conservative, and the only answer a miner cannot profit from.
        if avg_interval == 0 {
            tracing::warn!(target: "dwow_chain::consensus",
                "All {} intervals in the adjustment window are zero — leaving the target unchanged",
                count);
            return BlockTarget::new(self.target.load(Ordering::Acquire));
        }

        // Fixed-point ratio: SCALE means "exactly on target".
        // > SCALE means blocks arrive too fast → need harder (lower target).
        // < SCALE means blocks arrive too slow → need easier (higher target).
        let ratio_scaled = {
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
    ///
    /// A value that is **absent** keeps its current default — a fresh node has no stored target. A value
    /// that is present but the **wrong length** is corrupt, and is an error: silently keeping the default
    /// (which this did until 2026-09-22, `OBL-C53`) meant a node whose `target` key was damaged started
    /// on the *initial* target, a consensus-critical value quietly wrong with nothing downstream able to
    /// tell. A node that cannot read its own difficulty must not guess at it.
    pub fn load(&self, tree: &sled::Tree) -> Result<()> {
        if let Some(bytes) = tree
            .get(b"target")
            .map_err(|e| LinearError::StorageError(e.to_string()))?
        {
            if bytes.len() != 4 {
                return Err(LinearError::StorageError(format!(
                    "consensus `target` is {} bytes, expected 4 — refusing to start on a default target",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 4];
            arr.copy_from_slice(&bytes);
            self.target.store(u32::from_le_bytes(arr), Ordering::Release);
        }
        if let Some(bytes) = tree
            .get(b"timestamps")
            .map_err(|e| LinearError::StorageError(e.to_string()))?
        {
            // `chunks_exact` silently discards a trailing partial chunk, which would drop a timestamp
            // without a word. The length must be a whole number of 8-byte entries.
            if bytes.len() % 8 != 0 {
                return Err(LinearError::StorageError(format!(
                    "consensus `timestamps` is {} bytes, not a multiple of 8",
                    bytes.len()
                )));
            }
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
    #[cfg(feature = "pow")]
    pub fn verify_proof(&self, block: &Block, vm: &randomx::RandomXVM) -> Result<bool> {
        let hash = block.hash_with_vm(&vm)?;
        Ok(self.check_pow(&hash))
    }

    /// Check whether a pre-computed hash meets the target.
    ///
    /// This 32-bit comparison is canonical. Stratum bridges it to xmrig's
    /// 64-bit check — see `stratum.rs` target encoding comment.
    pub fn check_pow(&self, hash: &Blake3Hash) -> bool {
        let b = hash.as_bytes();
        let hash_u32 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        hash_u32 <= self.target.load(Ordering::Acquire)
    }

    // P2-9 dead-code sweep: verify_uncle_pow deleted — zero callers, and it
    // carried the same wrong-VM-key hazard fixed in check_uncles (P2-9-4):
    // uncle PoW must be verified with the uncle's own randomx_key, which
    // block::verify_uncle_proof already does.

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
    pub fn get_next_work_required(&self, store: &super::LinearStore, height: BlockHeight) -> std::result::Result<BlockTarget, super::LinearError> {
        if height <= BlockHeight::GENESIS {
            return Ok(BlockTarget::MAX);
        }

        // M-1 fix: fast path — read cached target[H-1] from block_targets tree.
        // Only need the last TIMESTAMP_WINDOW timestamps to compute adjustment.
        let prev_height = height.saturating_sub_blocks(1);
        let cache_key = prev_height.to_le_bytes();
        if let Ok(Some(raw)) = store.block_targets.get(&cache_key) {
            if raw.len() == 4 {
                let target_bytes: [u8; 4] = raw[..4].try_into().map_err(|_| {
                    LinearError::StorageError("Corrupt block_targets entry: wrong length".into())
                })?;
                let prev_target = BlockTarget::new(u32::from_le_bytes(target_bytes));
                if prev_target.get() > 0 {
                    let mut target = prev_target;
                    let start_h = if height.get() > TIMESTAMP_WINDOW as u64 {
                        height.get() - TIMESTAMP_WINDOW as u64
                    } else {
                        1
                    };
                    let mut timestamps: Vec<BlockTimestamp> = Vec::with_capacity(TIMESTAMP_WINDOW);
                    for h in start_h..height.get() {
                        if let Ok(block) = store.get_block(BlockHeight::new(h)) {
                            timestamps.push(block.header.timestamp);
                        }
                    }
                    if timestamps.len() >= 2 {
                        target = Self::compute_adjustment(
                            &timestamps, target,
                            self.target_block_time, self.min_target, self.max_target,
                        );
                    }
                    return Ok(target);
                }
            }
        }

        // Slow path: full chain walk from genesis (cache miss or cold start).
        // Kept as fallback — cache is populated on first connect_block.
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
                tracing::error!(
                    target: "consensus",
                    "Chain walk failed at height {h} — block missing from store."
                );
                return Err(super::LinearError::BlockNotFound(BlockHeight::new(h)));
            }
        }

        Ok(target)
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
        // Unifies with `adjust_target`, including both halves of the 2026-09-23 change (`OBL-C37`): a
        // decreasing pair abandons the adjustment rather than contributing a fabricated zero interval,
        // and so does a window whose intervals are *all* zero. This function is the consensus authority —
        // `get_next_work_required` calls it to derive the target a block must declare — so a divergence
        // here is a fork, not a disagreement about caching. Both paths must agree or the same window
        // would produce different targets depending on which one a caller used.
        let start = timestamps.len() - n;
        let mut total_interval = 0u64;
        for i in (start + 1)..timestamps.len() {
            let Some(interval) = timestamps[i].get().checked_sub(timestamps[i - 1].get()) else {
                tracing::warn!(target: "dwow_chain::consensus",
                    "Decreasing timestamp in compute_adjustment: {} < {} — leaving the target unchanged",
                    timestamps[i], timestamps[i - 1]);
                return current_target;
            };
            total_interval += interval;
        }
        let count = (n - 1) as u64;
        let avg_interval = if count > 0 {
            total_interval / count
        } else {
            target_block_time
        };

        // `OBL-C37`, the other half: an all-zero window abandons the adjustment here exactly as it does in
        // `adjust_target`. See that function for why the branch's former `SCALE * 9 / 10` answer was both
        // backwards (11% *easier*) and unreachable — but the two paths must agree, or the same window
        // yields two different expected targets depending on which one a caller asked.
        if avg_interval == 0 {
            tracing::warn!(target: "dwow_chain::consensus",
                "All {} intervals in compute_adjustment's window are zero — leaving the target unchanged",
                count);
            return current_target;
        }

        let ratio_scaled = {
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

#[cfg(all(test, feature = "pow"))]
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
    /// OBL-C53 — `load` refuses corrupt state instead of silently starting on a default.
    ///
    /// Three cases, and the middle one is the defect: an **absent** value is a fresh node and keeps its
    /// default; a **present but wrong-length** value is corruption and must be an error; a **well-formed**
    /// value loads. Without the second case a node whose `target` key was damaged would run on the initial
    /// target — a consensus-critical value quietly wrong, and nothing downstream could tell.
    #[test]
    fn test_load_rejects_corrupt_state_but_allows_absence() {
        let tree = sled::Config::new().temporary(true).open().unwrap().open_tree("consensus").unwrap();

        // Absent: a fresh node. Must keep its default, not error.
        let fresh = test_consensus();
        fresh.load(&tree).expect("an empty tree is a fresh node, not corruption");

        // Corrupt `target`: wrong length.
        tree.insert(b"target", &[0xABu8, 0xCD][..]).unwrap();
        assert!(
            test_consensus().load(&tree).is_err(),
            "OBL-C53: a wrong-length `target` is corrupt state and must be refused, not skipped — \
             skipping it starts the node on the initial target without saying so"
        );
        tree.remove(b"target").unwrap();

        // Corrupt `timestamps`: not a whole number of entries. `chunks_exact` would drop the remainder
        // silently, which is the same class of quiet loss.
        tree.insert(b"timestamps", &[0u8; 12][..]).unwrap();
        assert!(
            test_consensus().load(&tree).is_err(),
            "a `timestamps` value that is not a multiple of 8 must be refused rather than partially loaded"
        );
        tree.remove(b"timestamps").unwrap();

        // Well-formed values load. Positive control: without this, a `load` that always errored would
        // satisfy both assertions above.
        let mut target = Vec::new();
        target.extend_from_slice(&0x0000_FFFFu32.to_le_bytes());
        tree.insert(b"target", target).unwrap();
        let mut ts = Vec::new();
        for t in [1000u64, 1120, 1240] {
            ts.extend_from_slice(&t.to_le_bytes());
        }
        tree.insert(b"timestamps", ts).unwrap();

        let loaded = test_consensus();
        loaded.load(&tree).expect("well-formed state must load");
        assert_eq!(loaded.target(), BlockTarget::new(0x0000_FFFF), "the stored target must be loaded");
        assert_eq!(loaded.snapshot_timestamps().len(), 3, "and the stored timestamps");
    }

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

    /// OBL-C37 — a decreasing timestamp **abandons** the adjustment instead of biasing it.
    ///
    /// A zero interval drags `avg_interval` down and the target **easier**, so substituting zero for an
    /// impossible interval is an invitation to engineer one. The target must come back untouched.
    ///
    /// **This test was strengthened, not written.** It asserted only that the result was a validly clamped
    /// target, which the *defective* behaviour also satisfied — so it passed either way and would have
    /// passed against a rule that made the chain easier on nonsense input. Verified by reverting the fix:
    /// the assertion below then fails, because `0x00FFFFFF` is adjusted upward.
    #[test]
    fn test_compute_adjustment_decreasing_timestamps_leaves_the_target_unchanged() {
        let current = BlockTarget::new(0x00FFFFFF);
        let timestamps: Vec<BlockTimestamp> = vec![
            BlockTimestamp::new(1000), BlockTimestamp::new(900), BlockTimestamp::new(800),
            BlockTimestamp::new(700), BlockTimestamp::new(600), BlockTimestamp::new(500),
            BlockTimestamp::new(400), BlockTimestamp::new(300), BlockTimestamp::new(200),
            BlockTimestamp::new(100), BlockTimestamp::new(0),
        ];
        let result = PoWConsensus::compute_adjustment(
            &timestamps, current, 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));

        assert_eq!(
            result, current,
            "OBL-C37: a window containing a decreasing timestamp must leave the target exactly as it was. \
             Adjusting on it feeds the controller an interval no chain can produce, and the answer a miner \
             can then engineer toward is an easier target."
        );

        // Positive control: a *well-formed* window that is too fast must still move the target, or this
        // rule would be indistinguishable from "never adjust".
        let fast: Vec<BlockTimestamp> = (0..11).map(|i| BlockTimestamp::new(i * 30)).collect();
        let moved = PoWConsensus::compute_adjustment(
            &fast, current, 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));
        assert_ne!(moved, current, "control: 30s blocks against a 120s target must harden the target");
        assert!(moved.get() < current.get(), "and hardening means a *lower* target");
    }

    /// `OBL-C37`, the other half — a window whose intervals are **all** zero abandons the adjustment too.
    ///
    /// The branch this replaces answered `ratio_scaled = SCALE * 9/10` under the comment "make it 10%
    /// harder". Both halves were wrong, and the second is the one that matters: `adjust` divides by the
    /// adjustment, so a ratio *below* `SCALE` produces a target **11% higher** — easier — which is the
    /// exact opposite of the comment and a free easing for whoever reaches it.
    ///
    /// **The negative control is in the assertion, as a number.** `SCALE = 1_000_000` and
    /// `adjustment = 900_000`, so the old branch returned `0x00FFFFFF * 1_000_000 / 900_000 = 18_641_350
    /// = 0x011C71C6` — inside the test's own `[0x0000FFFF, 0x0FFFFFFF]` clamp, so the old behaviour really
    /// did reach the caller rather than being clamped away. Restore the branch and the equality below
    /// fails with that value; verified by reverting.
    #[test]
    fn test_compute_adjustment_all_zero_intervals_leaves_the_target_unchanged() {
        let current = BlockTarget::new(0x00FFFFFF);
        // Ten equal timestamps — `count == 9 > 0` and every interval zero, so `avg_interval == 0`.
        let timestamps: Vec<BlockTimestamp> = vec![BlockTimestamp::new(1000); 10];
        let result = PoWConsensus::compute_adjustment(
            &timestamps, current, 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));

        assert_eq!(
            result, current,
            "OBL-C37: a window with no interval at all must leave the target exactly as it was. The \
             former SCALE * 9 / 10 answer eased it by ~11% (0x00FFFFFF became 0x011C71C6), which is the \
             reverse of the direction its own comment claimed."
        );

        // A *single* zero interval among real ones is a different case and must NOT be abandoned: the
        // window is still informative, and the controller steers on the average.
        let one_zero: Vec<BlockTimestamp> = vec![
            BlockTimestamp::new(0), BlockTimestamp::new(0), BlockTimestamp::new(120),
            BlockTimestamp::new(240), BlockTimestamp::new(360),
        ];
        let steered = PoWConsensus::compute_adjustment(
            &one_zero, current, 120, BlockTarget::new(0x0000FFFF), BlockTarget::new(0x0FFFFFFF));
        assert_ne!(
            steered, current,
            "control: a window with one zero interval and four real ones still has an average to steer on"
        );
    }

    /// The same rule on the tracker path, whose window is fed by `record_block` rather than rebuilt from
    /// the store — the two must agree or a miner's own target and the expected one diverge (`OBL-C37`).
    #[test]
    fn test_adjust_target_all_zero_intervals_leaves_the_target_unchanged() {
        let c = test_consensus();
        let initial = c.target();
        for _ in 0..5 {
            c.record_block(BlockTimestamp::new(1000));
        }
        assert_eq!(
            c.adjust_target(), initial,
            "OBL-C37 (tracker path): equal timestamps must not move the target; the removed branch would \
             have eased it ~11% per call"
        );
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
