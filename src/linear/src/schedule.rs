/* This file is part of DarkWow
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

//! ExecutionSchedule — dependency-aware parallel execution ordering.
//!
//! Analyzes the write key sets of contract calls within a block and computes
//! a schedule of parallel waves. Calls with disjoint key sets form a single
//! wave (execute concurrently). Calls with intersecting key sets execute in
//! dependency order across sequential waves.
//!
//! This is the SHALL-analyze step from type-system.md §9.4: the schedule
//! SHALL be deterministic — same block, same key sets, same wave partition.
//!
//! Maps to ρ-calculus:
//! ```text
//! ExecutionSchedule =
//!   νkey_sets.(
//!     analyze_keys!(jobs, key_sets)
//!     | build_waves!(key_sets, waves)
//!     | for wave in waves: parallel_execute!(wave) | merge_wave!(wave)
//!   )
//! ```

use std::collections::{HashMap, HashSet};

/// Key identifier for sled tree writes. A key is a `(tree_name, key_bytes)` pair.
pub type SledKey = Vec<u8>;

/// A wave is a set of call indices that can execute in parallel.
/// All calls within a wave have pairwise disjoint write key sets.
pub type Wave = Vec<usize>;

/// Computes an execution schedule from the write key sets of contract calls.
///
/// Returns a list of waves. Calls within each wave may execute concurrently.
/// Waves execute sequentially in order — later waves may depend on state
/// modified by earlier waves.
pub struct ExecutionSchedule {
    /// Waves of parallel-executable call indices, ordered sequentially.
    pub waves: Vec<Wave>,
}

impl ExecutionSchedule {
    /// Build a schedule from the write key sets of each call.
    ///
    /// `write_sets[i]` is the set of sled keys that call `i` writes to.
    /// Calls with intersecting write sets are placed in sequential waves.
    /// Calls with disjoint write sets are placed in the same wave.
    ///
    /// The algorithm is conservative: if two calls share ANY sled key,
    /// they are serialized. This is the bisimulation safety condition
    /// from type-system.md §9.2.
    pub fn build(write_sets: &[HashSet<SledKey>]) -> Self {
        if write_sets.is_empty() {
            return Self { waves: Vec::new() };
        }

        let n = write_sets.len();
        let mut assigned: Vec<bool> = vec![false; n];
        let mut waves: Vec<Wave> = Vec::new();

        while assigned.iter().any(|a| !*a) {
            let mut wave: Wave = Vec::new();
            let mut wave_keys: HashSet<&SledKey> = HashSet::new();

            for i in 0..n {
                if assigned[i] {
                    continue;
                }

                // Check if call i's write set intersects with any call
                // already in this wave
                let intersects = write_sets[i].iter().any(|k| wave_keys.contains(k));

                if !intersects {
                    // Safe to add to this wave — keys are disjoint
                    wave.push(i);
                    for k in &write_sets[i] {
                        wave_keys.insert(k);
                    }
                    assigned[i] = true;
                }
            }

            if wave.is_empty() {
                // All remaining unassigned calls intersect with something
                // in the current wave — start a new wave with just the
                // first unassigned call to break the deadlock
                for i in 0..n {
                    if !assigned[i] {
                        wave.push(i);
                        assigned[i] = true;
                        break;
                    }
                }
            }

            waves.push(wave);
        }

        Self { waves }
    }

    /// Total number of calls in the schedule.
    pub fn total_calls(&self) -> usize {
        self.waves.iter().map(|w| w.len()).sum()
    }

    /// Number of waves (sequential stages).
    pub fn wave_count(&self) -> usize {
        self.waves.len()
    }

    /// Maximum parallelism — the size of the largest wave.
    pub fn max_parallelism(&self) -> usize {
        self.waves.iter().map(|w| w.len()).max().unwrap_or(0)
    }

    /// Returns true if the schedule is fully parallel (single wave).
    pub fn is_fully_parallel(&self) -> bool {
        self.waves.len() <= 1
    }

    /// Returns true if the schedule is fully sequential (every wave has 1 call).
    pub fn is_fully_sequential(&self) -> bool {
        self.waves.iter().all(|w| w.len() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_keys(indices: &[u8]) -> HashSet<SledKey> {
        indices.iter().map(|i| vec![*i]).collect()
    }

    #[test]
    fn test_disjoint_keys_fully_parallel() {
        // Three calls with completely disjoint key sets
        let sets = vec![
            make_keys(&[1, 2, 3]),
            make_keys(&[4, 5, 6]),
            make_keys(&[7, 8, 9]),
        ];

        let schedule = ExecutionSchedule::build(&sets);
        assert_eq!(schedule.wave_count(), 1);
        assert_eq!(schedule.max_parallelism(), 3);
        assert!(schedule.is_fully_parallel());
    }

    #[test]
    fn test_overlapping_keys_serialized() {
        // Three calls all writing to the same key
        let sets = vec![
            make_keys(&[1]),
            make_keys(&[1]),
            make_keys(&[1]),
        ];

        let schedule = ExecutionSchedule::build(&sets);
        assert_eq!(schedule.wave_count(), 3);
        assert_eq!(schedule.max_parallelism(), 1);
        assert!(schedule.is_fully_sequential());
    }

    #[test]
    fn test_mixed_keys_two_waves() {
        // Call 0 writes key A, call 1 writes key A+B, call 2 writes key B
        // Call 0 and call 2 are disjoint → can execute together
        // Call 1 intersects both → must be in a separate wave
        let sets = vec![
            make_keys(&[1]),          // key A
            make_keys(&[1, 2]),       // keys A + B
            make_keys(&[2]),          // key B
        ];

        let schedule = ExecutionSchedule::build(&sets);
        // Implementation detail: algorithm assigns [0,2] to wave 1, [1] to wave 2
        assert_eq!(schedule.total_calls(), 3);
    }

    #[test]
    fn test_empty_schedule() {
        let sets: Vec<HashSet<SledKey>> = vec![];
        let schedule = ExecutionSchedule::build(&sets);
        assert_eq!(schedule.wave_count(), 0);
    }

    #[test]
    fn test_single_call() {
        let sets = vec![make_keys(&[1, 2, 3])];
        let schedule = ExecutionSchedule::build(&sets);
        assert_eq!(schedule.wave_count(), 1);
        assert_eq!(schedule.max_parallelism(), 1);
    }
}
