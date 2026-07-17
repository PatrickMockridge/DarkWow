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

use dwow_serial::{SerialDecodable, SerialEncodable};

/// Nominal block-height type (type-system.md §2.3, §8.1).
///
/// A block height and a reward amount are both representable as `u64`, but
/// they inhabit different consensus domains and SHALL NOT unify. The newtype
/// makes the compiler enforce the distinction: `expected_reward(reward)` does
/// not compile.
///
/// - dwow-serial encoding is transparent (the inner `u64`) — every structure
///   carrying a height is wire-identical to a bare `u64` field.
/// - serde encoding is a plain JSON number (manual impl below).
/// - Canonical byte encoding is `to_le_bytes() -> [u8; 8]` — used for every
///   hash preimage, key-derivation seed, and sled key that includes a height.
/// - No `Add`/`Sub` operators and no `Step`: height arithmetic uses the named
///   methods (`succ`, `pred`, `checked_sub`, `saturating_sub`) so intent is
///   explicit; range loops iterate `u64` and construct `BlockHeight::new(h)`.
#[repr(transparent)]
#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, SerialEncodable, SerialDecodable,
)]
pub struct BlockHeight(u64);

impl BlockHeight {
    /// Genesis block height. Height 0 is the pre-genesis sentinel ("no block").
    pub const GENESIS: Self = Self(1);

    /// Construct from the raw height domain. Total: all `u64` values are
    /// valid heights (`0` = pre-genesis sentinel, `1` = genesis).
    pub const fn new(height: u64) -> Self {
        Self(height)
    }

    /// The raw height value — for arithmetic at domain edges and display.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next block height (`h + 1`). Overflow is unreachable in the
    /// height domain (2^64 blocks at 120 s ≫ age of the universe).
    pub const fn succ(self) -> Self {
        Self(self.0 + 1)
    }

    /// The previous block height, or `None` at the pre-genesis sentinel.
    pub const fn pred(self) -> Option<Self> {
        match self.0.checked_sub(1) {
            Some(h) => Some(Self(h)),
            None => None,
        }
    }

    /// Depth arithmetic: `self - rhs`, or `None` if `rhs` is above `self`.
    pub const fn checked_sub(self, rhs: Self) -> Option<u64> {
        self.0.checked_sub(rhs.0)
    }

    /// Depth arithmetic clamped at zero (maturity / uncle-depth checks).
    pub const fn saturating_sub(self, rhs: Self) -> u64 {
        self.0.saturating_sub(rhs.0)
    }

    /// Canonical byte encoding (§2.3): 8 bytes little-endian. The ONLY
    /// encoding permitted in hash preimages, derivation seeds, and sled keys.
    pub const fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    /// Persistence-boundary lift (§2.2): reconstruct from the canonical bytes.
    pub const fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self(u64::from_le_bytes(bytes))
    }
}

impl core::fmt::Display for BlockHeight {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Manual serde as a plain number — keeps every JSON shape (RPC, wallet DB
// chain_blocks, header serde tests) identical to the bare-u64 encoding.
impl serde::Serialize for BlockHeight {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for BlockHeight {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(u64::deserialize(d)?))
    }
}

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
    pub const HALF_LIFE_BLOCKS: u64 = 1_051_920;

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
    pub const BLOCKS_PER_YEAR: u64 = 262_980;
}

/// Auxiliary function to calculate provided block height block version.
/// Currently, a single version(1) exists.
pub fn block_version(_height: BlockHeight) -> u8 {
    1
}

/// Calculate the expected block reward for a given block height.
///
/// Implements the exponential decay formula from consensus-coinbase.md §3.2:
///   R(h) = max(R₀ × 2^(-h/H), R_tail)
///
/// Uses closed-form binary exponentiation (O(log h)) with 32-bit fixed-point
/// arithmetic for deterministic, cross-platform consensus safety. Floating point
/// MUST NOT be used for supply computation.
///
/// Height 0 (no block) always returns 0. Height 1 (genesis) receives
/// GENESIS_REWARD (= INITIAL_REWARD). Heights >= 2 follow continuous exponential
/// decay — every block gets a fractionally smaller reward than the previous.
/// No Bitcoin-style step halvings. The tail emission floor activates when the
/// decay curve reaches the tail threshold.
pub fn expected_reward(height: BlockHeight) -> u64 {
    let height = height.get();
    if height == 0 {
        return 0;
    }
    if height == 1 {
        return reward::INITIAL_REWARD;
    }

    let exp = height - 1;
    let decay = fixed_pow_decay(exp);
    let reward = ((reward::INITIAL_REWARD as u128 * decay as u128) >> 32) as u64;

    if reward <= reward::TAIL_REWARD {
        return reward::TAIL_REWARD;
    }
    reward
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

#[cfg(test)]
mod reward_tests {
    use super::*;

    /// The exponential reward formula must produce the expected values
    /// at key points: genesis, half-life, tail.
    #[test]
    fn reward_formula_key_points() {
        assert_eq!(expected_reward(BlockHeight::new(0)), 0);
        assert_eq!(expected_reward(BlockHeight::GENESIS), reward::INITIAL_REWARD);
        // At half-life, should be approximately R0/2
        let at_half = expected_reward(BlockHeight::new(reward::HALF_LIFE_BLOCKS + 1));
        let expected_half = reward::INITIAL_REWARD / 2;
        let diff = at_half.abs_diff(expected_half);
        let tolerance = expected_half / 100;
        assert!(diff <= tolerance,
            "At half-life: reward={}, R0/2={}, diff={}", at_half, expected_half, diff);
        // Tail should be reached well before u32::MAX
        let at_tail = expected_reward(BlockHeight::new(reward::HALF_LIFE_BLOCKS * 20));
        assert_eq!(at_tail, reward::TAIL_REWARD, "Should reach tail by 20 half-lives");
    }

    /// Reward must be monotonically decreasing.
    #[test]
    fn reward_monotonic_decrease() {
        let mut prev = expected_reward(BlockHeight::GENESIS);
        for h in 2..1000 {
            let cur = expected_reward(BlockHeight::new(h));
            assert!(cur <= prev, "h={}: {} > {}", h, cur, prev);
            prev = cur;
        }
    }

    /// Binary exponentiation: multiplicative property holds.
    #[test]
    fn binary_exp_additive_property() {
        let a = 1000;
        let b = 2000;
        let fp_a = fixed_pow_decay(a);
        let fp_b = fixed_pow_decay(b);
        let fp_ab = fixed_pow_decay(a + b);
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
pub fn expected_cumulative_supply(height: BlockHeight) -> u64 {
    let mut total: u64 = 0;
    for h in 1..=height.get() {
        total = total.saturating_add(expected_reward(BlockHeight::new(h)));
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
pub fn coinbase_blind(prev_coin: &[u8; 32], height: BlockHeight) -> pallas::Scalar {
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
    cumulative_commits: &[(BlockHeight, pallas::Point)],  // (height, S_H) pairs
) -> bool {
    use crate::crypto::{pedersen_commitment_u64, ScalarBlind, Blind};

    let mut expected = pallas::Point::identity();
    let mut prev_coin = [0u8; 32]; // genesis: zero
    let mut expected_height = BlockHeight::GENESIS;

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
        expected_height = expected_height.succ();
    }
    true
}
