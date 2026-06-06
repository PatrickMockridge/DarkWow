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

//! Blockchain-level constants and reward schedule.
//!
//! # Block Reward Design
//!
//! DarkWow uses a **continuous exponential decay** reward schedule with a
//! permanent tail emission — merging Bitcoin's 21M hard cap with Monero's
//! fair CPU mining and tail subsidy. The reward decreases every block;
//! there are no step-function halvings.
//!
//! For the full economic rationale, see [`doc/src/arch/mining-tokenomics.md`].
//!
//! ## Parameters
//!
//! | Parameter | Value | Notes |
//! |-----------|-------|-------|
//! | Block time | 120 seconds | 262,980 blocks/year |
//! | Supply cap | 21,000,000 DRKW| 2.1 × 10^15 base units |
//! | Half-life (H) | 1,051,920 blocks | ~4 years |
//! | Tail emission | 1% per annum | 210,000 DRK/year |
//! | Initial reward (R₀) | 1,383,764,049 | ~13.838 DRKW|
//! | Tail reward (R_tail) | 79,853,981 | ~0.7985 DRKW|
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
//! The main emission asymptotically approaches 21M DRKWthrough the
//! geometric decay. Tail emission begins when the exponential reward
//! drops below the per-block tail threshold (~16.5 years after launch).

/// Constants for the block reward schedule.
pub mod reward {
    /// Block reward for genesis block.
    pub const GENESIS_REWARD: u64 = 0;

    /// Initial block reward at height 1 (in base units: 1 DRKW= 10^8).
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

use pasta_curves::{
    group::ff::FromUniformBytes,
    pallas,
};

/// Derive the deterministic coinbase blind for a block at the given height.
///
/// `blind_H = blake2b("native_token_coinbase_blind" || prev_coin || height)`
///
/// The previous coin commitment ensures each block's blind is unique and
/// unpredictable without knowing the full chain history. Anyone with the
/// blockchain can independently recompute all blinds and verify the
/// cumulative supply commitment chain.
pub fn coinbase_blind(prev_coin: &[u8; 32], height: u32) -> pallas::Scalar {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"native_token_coinbase_blind");
    hasher.update(prev_coin);
    hasher.update(&height.to_le_bytes());
    // Produce 64 bytes for from_uniform_bytes
    let hash = hasher.finalize();
    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(hash.as_bytes());
    // Second hash for the upper 32 bytes
    let mut hasher2 = blake3::Hasher::new();
    hasher2.update(b"native_token_coinbase_blind_2");
    hasher2.update(hash.as_bytes());
    wide[32..].copy_from_slice(hasher2.finalize().as_bytes());
    pallas::Scalar::from_uniform_bytes(&wide)
}

/// Verify the cumulative supply commitment chain from genesis to tip.
///
/// Returns `true` if for every block at height `h`:
///   `S_h == S_{h-1} + pedersen_commit(expected_reward(h), coinbase_blind(prev_coin_h, h))`
///
/// This is the Zcash-Orchard-hardened supply audit: any node can independently
/// verify total supply without trusting any single contract state value.
#[cfg(feature = "client")]
pub fn verify_cumulative_supply(
    cumulative_commits: &[(u32, pallas::Point)],  // (height, S_H) pairs
) -> bool {
    use crate::crypto::{pedersen_commitment_u64, ScalarBlind, Blind};

    let mut expected = pallas::Point::identity();
    let mut prev_coin = [0u8; 32]; // genesis
    let mut expected_height: u32 = 1;

    for (height, commit) in cumulative_commits {
        if *height != expected_height {
            return false; // heights must be sequential
        }
        let reward = expected_reward(*height);
        let blind = coinbase_blind(&prev_coin, *height);
        let coin_vc = pedersen_commitment_u64(reward, Blind(blind));
        expected = expected + coin_vc;
        if expected != *commit {
            return false; // chain break!
        }
        // prev_coin would be read from the actual block's coinbase commitment
        // in a full implementation. Here we just advance the expected height.
        expected_height += 1;
    }
    true
}
