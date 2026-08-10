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

//! Dynamic fee estimator — tracks recent block gas utilization.
//!
//! Bitcoin Core pattern: estimatesmartfee uses recent block confirmation
//! data to recommend fees. DarkWow uses block gas utilization as proxy
//! for congestion (ZK proof verification cost dominates).
//!
//! HAZOP Gap 4 remediation (2026-07-01).

use std::collections::VecDeque;
use smol::lock::Mutex;

use crate::fee_window::compute_fee;

/// Base fee estimate at zero congestion for the rolling-window estimator.
/// This is a SPECULATIVE estimate used for fee estimation UX, NOT for
/// consensus-critical computation.
///
/// Derived from: compute_fee(&[1000], 1, CongestionFactor::default(), ...)
/// FI-GEN-2 TODO: replace with chain-derived baseline from FeeWindowState
/// when genesis ceremony stores economic parameters in sled state.
pub const MIN_FEE_ESTIMATE: u64 = 1_001_000;

/// Block gas limit — re-exported from `src/linear/src/execution.rs`.
pub use crate::execution::BLOCK_GAS_LIMIT;

/// Rolling-window fee estimator.
pub struct FeeEstimator {
    /// Recent block gas utilization values
    gas_history: Mutex<VecDeque<u64>>,
    window_size: usize,
}

impl FeeEstimator {
    /// Create a new estimator with the given window size (default 20 blocks).
    pub fn new(window_size: usize) -> Self {
        Self {
            gas_history: Mutex::new(VecDeque::with_capacity(window_size)),
            window_size,
        }
    }

    /// Record the gas used in a newly accepted block.
    pub async fn record_block(&self, gas_used: u64) {
        let mut history = self.gas_history.lock().await;
        history.push_back(gas_used);
        if history.len() > self.window_size {
            history.pop_front();
        }
    }

    /// Estimate the current fee based on recent block utilization.
    ///
    /// Utilization < 50% → MIN_FEE_ESTIMATE
    /// Utilization 50-80% → MIN_FEE_ESTIMATE × (1.0 + utilization)
    /// Utilization ≥ 80% → MIN_FEE_ESTIMATE × 2.0
    pub async fn estimate(&self) -> u64 {
        let history = self.gas_history.lock().await;
        if history.is_empty() {
            return MIN_FEE_ESTIMATE;
        }

        let total_gas: u64 = history.iter().sum();
        let capacity = (history.len() as u64) * BLOCK_GAS_LIMIT;
        if capacity == 0 {
            return MIN_FEE_ESTIMATE;
        }

        // Utilization as basis points (0-10000 = 0%-100%)
        let utilization_bp = (total_gas * 10000) / capacity;

        if utilization_bp < 5000 {
            MIN_FEE_ESTIMATE
        } else if utilization_bp < 8000 {
            // Linear scaling from 1.0x to 2.0x between 50% and 80%
            let multiplier_bp = 10000 + ((utilization_bp - 5000) * 10000) / 3000;
            (MIN_FEE_ESTIMATE as u128 * multiplier_bp as u128 / 10000) as u64
        } else {
            MIN_FEE_ESTIMATE * 2
        }
    }

    /// Get current utilization ratio (0.0-1.0) for diagnostics.
    pub async fn utilization(&self) -> f64 {
        let history = self.gas_history.lock().await;
        if history.is_empty() {
            return 0.0;
        }
        let total_gas: u64 = history.iter().sum();
        let capacity = (history.len() as u64) * BLOCK_GAS_LIMIT;
        if capacity == 0 {
            return 0.0;
        }
        total_gas as f64 / capacity as f64
    }

    /// Number of blocks in the estimation window.
    pub async fn blocks_sampled(&self) -> usize {
        self.gas_history.lock().await.len()
    }
}

impl Default for FeeEstimator {
    fn default() -> Self {
        Self::new(20)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_returns_min() {
        smol::block_on(async {
            let est = FeeEstimator::new(20);
            assert_eq!(est.estimate().await, MIN_FEE_ESTIMATE);
        });
    }

    #[test]
    fn test_low_utilization_returns_min() {
        smol::block_on(async {
            let est = FeeEstimator::new(20);
            // 10% utilization
            est.record_block(BLOCK_GAS_LIMIT / 10).await;
            assert_eq!(est.estimate().await, MIN_FEE_ESTIMATE);
        });
    }

    #[test]
    fn test_high_utilization_doubles() {
        smol::block_on(async {
            let est = FeeEstimator::new(20);
            // 90% utilization
            est.record_block(BLOCK_GAS_LIMIT * 9 / 10).await;
            assert_eq!(est.estimate().await, MIN_FEE_ESTIMATE * 2);
        });
    }

    #[test]
    fn test_window_rolls_off() {
        smol::block_on(async {
            let est = FeeEstimator::new(3);
            // Fill with high-utilization blocks
            for _ in 0..3 {
                est.record_block(BLOCK_GAS_LIMIT * 9 / 10).await;
            }
            assert_eq!(est.estimate().await, MIN_FEE_ESTIMATE * 2);
            // Add empty blocks
            for _ in 0..3 {
                est.record_block(0).await;
            }
            assert_eq!(est.estimate().await, MIN_FEE_ESTIMATE);
        });
    }
}
