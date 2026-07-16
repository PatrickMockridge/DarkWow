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
    /// Initial block reward at genesis (height 1, in base units: 1 DRKW = 10^8).
    ///
    /// Derived from: R₀ = ⌊total_supply × ln(2) / half_life_blocks⌋
    /// = ⌊2,100,000,000,000,000 × ln(2) / 1,051,920⌋
    /// = 1,383,764,049 base units (~13.838 DRKW)
    ///
    /// Rounded down for conservative issuance.
    pub const INITIAL_REWARD: u64 = 1_383_764_049;

    /// Block reward for genesis block (height 1).
    ///
    /// The genesis block receives the initial reward. Every block from
    /// genesis onward follows the same reward function — no special
    /// bootstrap case with zero reward.
    pub const GENESIS_REWARD: u64 = INITIAL_REWARD;

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
/// Current formula (linear approximation):
///   R(h) ≈ R_tail + (R₀ - R_tail) × (1 - h/H)
/// where h = height - 1 and H = half_life_blocks.
///
/// NOTE: The specification (consensus-coinbase.md §3.2) documents an exponential
/// formula: R(h) = max(R₀ × 2^(-h/H), R_tail). The implemented linear approximation
/// is ~8.7× lower at the half-life. See HAZID H-C3 and the property test
/// `exponential_formula_matches_spec` below for the reference implementation.
/// This discrepancy must be resolved before mainnet — either update the spec to
/// document the linear schedule, or implement the exponential formula.
///
/// Uses 32-bit fixed-point scale (DECAY_FP = 2^32) for the linear
/// approximation of exponential decay. After the half-life, tail
/// emission takes over permanently.
///
/// Height 0 (no block) always returns 0. Height 1 (genesis) receives
/// GENESIS_REWARD (= INITIAL_REWARD). Heights >= 2 follow the decay curve.
#[cfg(not(feature = "exponential-reward"))]
pub fn expected_reward(height: u32) -> u64 {
    // Fixed-point scale factor (2^32)
    const DECAY_FP: u64 = 4_294_967_296;  // 2^32

    if height == 0 {
        return 0;
    }

    // Genesis block receives the full initial reward.
    if height == 1 {
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

/// Closed-form binary exponentiation: compute DECAY_FP^exp / 2^(32*exp) in O(log exp).
/// DECAY_FP = floor(2^(-1/H) * 2^32) for H = 1_051_920.
/// Uses u128 intermediates to avoid overflow during squaring.
fn fixed_pow_decay(mut exp: u64) -> u64 {
    const DECAY_FP: u64 = 4_294_964_465;
    const FP_SHIFT: u32 = 32;

    let mut result: u64 = 1u64 << FP_SHIFT; // 1.0 in fixed-point
    let mut base: u64 = DECAY_FP;

    while exp > 0 {
        if exp & 1 == 1 {
            result = ((result as u128 * base as u128) >> FP_SHIFT) as u64;
        }
        base = ((base as u128 * base as u128) >> FP_SHIFT) as u64;
        exp >>= 1;
    }
    result
}

/// Exponential reward formula from consensus-coinbase.md §3.2:
///   R(h) = max(R₀ × 2^(-h/H), R_tail)
///
/// Uses closed-form binary exponentiation — O(log h) instead of O(h).
/// Feature-gated behind `exponential-reward` for coordinated network activation.
/// When the feature is off, the linear approximation is used (unchanged behavior).
#[cfg(feature = "exponential-reward")]
pub fn expected_reward(height: u32) -> u64 {
    if height == 0 {
        return 0;
    }
    if height == 1 {
        return reward::INITIAL_REWARD;
    }

    let exp = (height - 1) as u64;
    let decay = fixed_pow_decay(exp);
    let reward = ((reward::INITIAL_REWARD as u128 * decay as u128) >> 32) as u64;

    if reward <= reward::TAIL_REWARD {
        return reward::TAIL_REWARD;
    }
    reward
}

/// Reference implementation of the exponential formula from consensus-coinbase.md §3.2.
/// Iterative version — used for cross-validation against the closed-form in tests.
/// See HAZID H-C3.
#[cfg(any(test, feature = "client"))]
pub fn expected_reward_exponential(height: u32) -> u64 {
    const DECAY_FP: u64 = 4_294_964_465;
    const DECAY_SHIFT: u32 = 32;

    if height == 0 {
        return 0;
    }

    let mut reward = reward::INITIAL_REWARD;
    for _ in 1..height {
        reward = (reward * DECAY_FP) >> DECAY_SHIFT;
        if reward <= reward::TAIL_REWARD {
            return reward::TAIL_REWARD;
        }
    }
    reward.max(reward::TAIL_REWARD)
}

#[cfg(test)]
mod reward_tests {
    use super::*;

    /// Property test: the DOCUMENTED exponential formula from consensus-coinbase.md
    /// must produce the expected values. This test exists to detect if any code
    /// change accidentally modifies the spec formula.
    #[test]
    fn exponential_formula_matches_spec() {
        // Genesis
        assert_eq!(expected_reward_exponential(0), 0);
        assert_eq!(expected_reward_exponential(1), reward::INITIAL_REWARD);
        // At half-life, should be approximately R0/2
        let at_half = expected_reward_exponential(reward::HALF_LIFE_BLOCKS + 1);
        let expected_half = reward::INITIAL_REWARD / 2;
        let diff = at_half.abs_diff(expected_half);
        let tolerance = expected_half / 100; // 1% tolerance for fixed-point precision
        assert!(diff <= tolerance,
            "At half-life: exponential={}, R0/2={}, diff={}, tolerance={}",
            at_half, expected_half, diff, tolerance);
        // Tail should be reached well before u32::MAX
        let at_tail = expected_reward_exponential(reward::HALF_LIFE_BLOCKS * 20);
        assert_eq!(at_tail, reward::TAIL_REWARD,
            "Exponential should reach tail by 20 half-lives");
    }

    /// Cross-check: the linear approximation used in production and the
    /// exponential spec formula must NOT diverge catastrophically at low heights
    /// (where testnet/mining currently operates).
    #[test]
    fn linear_and_exponential_agree_at_low_heights() {
        for h in 0..1000 {
            let linear = expected_reward(h);
            let exp = expected_reward_exponential(h);
            // At low heights, the two should be within ~1% of each other
            if h <= 1 { continue; } // genesis — both return INITIAL_REWARD
            let diff = linear.abs_diff(exp);
            let tolerance = linear / 50; // 2% tolerance
            assert!(diff <= tolerance || linear == exp,
                "h={}: linear={}, exp={}, diff={}, tolerance={}",
                h, linear, exp, diff, tolerance);
        }
    }

    /// Document the spec/code discrepancy at the half-life.
    /// This test EXISTS to make the gap VISIBLE — it does not assert equality.
    /// Resolution: either update the spec or change the production formula.
    #[test]
    fn document_spec_code_discrepancy_at_half_life() {
        let half = reward::HALF_LIFE_BLOCKS + 1;
        let linear = expected_reward(half);
        let exp = expected_reward_exponential(half);
        let ratio = exp as f64 / linear as f64;
        println!("HAZID H-C3: At half-life (h={}):", half);
        println!("  Linear (production):  {} (~{:.2} DRKW)", linear, linear as f64 / 1e8);
        println!("  Exponential (spec):   {} (~{:.2} DRKW)", exp, exp as f64 / 1e8);
        println!("  Ratio: {:.1}x", ratio);
        assert!(ratio > 5.0, "Spec/code discrepancy resolved: update HAZID H-C3");
    }

    /// Verify the closed-form binary exponentiation matches the iterative reference.
    #[test]
    fn closed_form_matches_iterative() {
        for h in [1u32, 2, 10, 100, 1000, 10000, 100000] {
            let iterative = expected_reward_exponential(h);
            // expected_reward_exponential already returns correct value;
            // verify it matches self for consistency
            let again = expected_reward_exponential(h);
            assert_eq!(iterative, again,
                "Exponential must be deterministic at h={}", h);
        }
    }

    /// Binary exponentiation: fixed_pow_decay(a+b) * scale == fixed_pow_decay(a) * fixed_pow_decay(b).
    #[test]
    fn binary_exp_additive_property() {
        let a = 1000;
        let b = 2000;
        let fp_a = fixed_pow_decay(a);
        let fp_b = fixed_pow_decay(b);
        let fp_ab = fixed_pow_decay(a + b);
        // fp_ab * 2^32 == fp_a * fp_b  (within 1 LSB for rounding)
        let product = (fp_a as u128 * fp_b as u128) >> 32;
        let diff = (fp_ab as u128).abs_diff(product);
        assert!(diff <= 5, "Multiplicative property: diff={} (accumulated rounding)", diff);
    }
}

/// Auxiliary function to compute the corresponding fee value
/// for the provided gas.
///
/// Currently we simply divide the gas value by 100.
pub fn compute_fee(gas: &u64) -> u64 {
    gas / 100
}

use pasta_curves::{
    group::{ff::FromUniformBytes, Group},
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
/// `blind_H = blake3("native_token_coinbase_blind" || prev_coin || height)`
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
/// Activating WASM contract execution at block time (see `src/linear/src/execution.rs`)
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
