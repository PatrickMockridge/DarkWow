/* This file is part of DarkFi (https://dark.fi)
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

//! Blockchain-level constants and reward schedule.
//!
//! # Block Reward Design
//!
//! DarkFi uses a **continuous exponential decay** reward schedule with a
//! permanent tail emission. The reward decreases every block — there are
//! no step-function halvings.
//!
//! ## Parameters
//!
//! | Parameter | Value | Notes |
//! |-----------|-------|-------|
//! | Block time | 120 seconds | 262,980 blocks/year |
//! | Supply cap | 21,000,000 DRK | 2.1 × 10^15 base units |
//! | Half-life (H) | 1,051,920 blocks | ~4 years |
//! | Tail emission | 1% per annum | 210,000 DRK/year |
//! | Initial reward (R₀) | 1,383,800,000 | ~13.838 DRK |
//! | Tail reward (R_tail) | 79,800,000 | ~0.798 DRK |
//!
//! ## Reward Function
//!
//! ```text
//! R(h) = max( R₀ × 2^(-h/H), R_tail )
//!
//! Genesis (h=0) always returns 0.
//! ```
//!
//! ## Supply Convergence
//!
//! The main emission asymptotically approaches 21M DRK through the
//! geometric decay. Tail emission begins when the exponential reward
//! drops below the per-block tail threshold (~16.5 years after launch).

/// Constants for the block reward schedule.
pub mod reward {
    /// Block reward for genesis block.
    pub const GENESIS_REWARD: u64 = 0;

    /// Initial block reward at height 1 (in base units: 1 DRK = 10^8).
    ///
    /// Derived from: R₀ = ⌊total_supply × ln(2) / half_life_blocks⌋
    /// = ⌊2,100,000,000,000,000 × ln(2) / 1,051,920⌋
    /// = 1,383,764,049 base units (~13.837 DRK)
    ///
    /// Rounded down for conservative issuance.
    pub const INITIAL_REWARD: u64 = 1_383_764_049;

    /// Half-life in blocks (~4 years at 2-minute blocks).
    pub const HALF_LIFE_BLOCKS: u32 = 1_051_920;

    /// Tail emission reward per block (in base units).
    ///
    /// 1% per annum of the 21M cap, rounded down:
    /// = ⌊21,000,000 × 0.01 × 10^8 / 262,980⌋
    /// = 79,853,981 base units (~0.7985 DRK)
    pub const TAIL_REWARD: u64 = 79_853_981;

    /// Maximum total supply (in DRK).
    pub const MAX_SUPPLY_DRK: u64 = 21_000_000;

    /// Maximum total supply (in base units).
    pub const MAX_SUPPLY: u64 = MAX_SUPPLY_DRK * 100_000_000; // 2.1 × 10^15

    /// Blocks per year at 2-minute block time (365.25 × 24 × 3600 / 120).
    pub const BLOCKS_PER_YEAR: u32 = 262_980;
}

/// Auxiliary function to calculate provided block height block version.
/// Currently, a single version(1) exists.
pub fn block_version(_height: u32) -> u8 {
    1
}

/// Calculate the expected block reward for a given block height.
///
/// Uses exponential decay: `R(h) = max( R₀ × 2^(-h/H), R_tail )`
///
/// Genesis (height 0) always returns 0.
///
/// The computation uses `f64::powf` which is deterministic per IEEE 754
/// across all supported architectures (x86_64, ARM64).
pub fn expected_reward(height: u32) -> u64 {
    if height == 0 {
        return reward::GENESIS_REWARD;
    }

    let decay = 2.0f64.powf(-(height as f64) / reward::HALF_LIFE_BLOCKS as f64);
    let reward = (reward::INITIAL_REWARD as f64 * decay) as u64;

    // Apply tail emission floor — once the exponential drops below the
    // per-block tail threshold, the tail emission takes over permanently.
    reward.max(reward::TAIL_REWARD)
}

/// Auxiliary function to compute the corresponding fee value
/// for the provided gas.
///
/// Currently we simply divide the gas value by 100.
pub fn compute_fee(gas: &u64) -> u64 {
    gas / 100
}
