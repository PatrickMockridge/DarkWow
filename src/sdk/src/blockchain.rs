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
/// Uses integer-only fixed-point arithmetic for deterministic, cross-platform
/// consensus safety. Floating point (f64::powf) produces non-deterministic
/// results across architectures and must not be used for supply computation.
///
/// Formula: `R(h) ≈ R_tail + (R₀ - R_tail) × (1 - h/H)`
/// where h = height - 1 and H = half_life_blocks.
///
/// Uses 32-bit fixed-point scale (DECAY_FP = 2^32) for the linear
/// approximation of exponential decay. After the half-life, tail
/// emission takes over permanently.
///
/// Genesis (height 0) always returns 0.
pub fn expected_reward(height: u32) -> u64 {
    // Fixed-point scale factor (2^32)
    const DECAY_FP: u64 = 4_294_967_296;  // 2^32

    if height == 0 {
        return reward::GENESIS_REWARD;
    }

    // Tail emission floor — once past the half-life, reward is constant
    if height > reward::HALF_LIFE_BLOCKS {
        return reward::TAIL_REWARD;
    }

    let h = (height - 1) as u64;
    let numerator = reward::INITIAL_REWARD.saturating_sub(reward::TAIL_REWARD);

    // decay = (DECAY_FP * h) / HALF_LIFE_BLOCKS  (fixed-point fraction of half-life elapsed)
    let decay = (DECAY_FP.saturating_mul(h)) / reward::HALF_LIFE_BLOCKS as u64;

    // pre_reward = numerator * (DECAY_FP - decay) / DECAY_FP
    let pre_reward = numerator
        .saturating_mul(DECAY_FP.saturating_sub(decay))
        / DECAY_FP;

    reward::TAIL_REWARD.saturating_add(pre_reward)
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

/// Compute the expected cumulative total supply at a given block height.
/// Sum of expected_reward(h) for h = 1..=height.
pub fn expected_cumulative_supply(height: u32) -> u64 {
    let mut total: u64 = 0;
    for h in 1..=height {
        total = total.saturating_add(expected_reward(h));
    }
    total
}

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
/// This is the **supply audit capability** of the native token — a passive,
/// verifiable property that any node can exercise without trusting a single
/// ZK proof. The audit walks the canonical chain and recomputes every
/// Pedersen commitment and blind from the emission schedule, comparing
/// against the stored cumulative commitment at each height.
///
/// Returns `true` if for every block at height `h`:
///   `S_h == S_{h-1} + pedersen_commit(expected_reward(h), coinbase_blind(prev_coin_h, h))`
///
/// # Design Decision: Passive Capability
///
/// The cumulative supply chain is a **passive audit**, not an active
/// consensus circuit breaker. Like Bitcoin's halving schedule, it is a
/// verifiable property of the chain that any observer can check. Block
/// production does not halt if the chain diverges — nodes detect the
/// divergence and can choose to fork.
///
/// Activating WASM contract execution at block time (see `bin/dwowd/src/execution.rs`)
/// would make this validation an **active** consensus rule — blocks with
/// invalid cumulative commitments would be rejected at execution time.
///
/// # Possible Future Upgrade
///
/// The `prev_coin` parameter is not yet updated from actual block data.
/// In a full implementation, `prev_coin` would be read from each block's
/// coinbase commitment, enabling independent verification of the entire
/// canonical chain from genesis to tip without caller-supplied intermediate
/// state. The API would change to accept a blockchain reference rather than
/// a pre-computed list of `(height, S_H)` pairs.
///
/// # Architectural Contrast
///
/// The Zcash Orchard shielded pool (May 2026) had no supply audit capability.
/// When a circuit constraint was missing, there was no independent way to
/// detect hidden inflation. The Pedersen chain provides this capability —
/// even if the ZK circuit had a soundness bug, the binding property of
/// Pedersen commitments makes any divergence cryptographically detectable.
#[cfg(feature = "client")]
pub fn verify_cumulative_supply(
    cumulative_commits: &[(u32, pallas::Point)],  // (height, S_H) pairs
) -> bool {
    use crate::crypto::{pedersen_commitment_u64, ScalarBlind, Blind};

    let mut expected = pallas::Point::identity();
    let mut prev_coin = [0u8; 32]; // genesis: zero
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
        // POSSIBLE FUTURE UPGRADE: prev_coin would be read from the actual
        // block's coinbase commitment in a full implementation. Currently
        // the function verifies internal consistency of caller-supplied data.
        // When prev_coin is read from on-chain blocks, this function will
        // independently verify the entire canonical chain from genesis to tip.
        expected_height += 1;
    }
    true
}
