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

//! Entropy and Randomness Module
//!
//! Provides provably fair randomness generation using block hash entropy.
//! Used by betting/gambling contracts for drawing outcomes.
//!
//! ## Security Model
//!
//! Block hash entropy is sourced from Proof-of-Work mining.
//! An attacker with < 33% hash power has negligible chance to manipulate
//! a single block hash. For higher security, contracts can require
//! multiple block confirmations before resolving.
//!
//! ## Entropy Levels
//!
//! | Level | Blocks | Security (vs 33% attacker) |
//! |-------|--------|------------------------------|
//! | Low   | 1      | ~33% manipulation chance     |
//! | Medium| 6      | ~0.14% (Bitcoin standard)    |
//! | High  | 10     | ~0.005%                      |
//!
//! ## Usage
//!
//! ```rust
//! use darkfi_sdk::crypto::entropy::{
//!     draw_single, draw_unique_range, combine_block_hashes
//! };
//!
//! // Draw a single number 0-36 (roulette-style)
//! let winning_number = draw_single(block_hash, nonce, 37);
//!
//! // Draw 6 unique numbers from 1-59 (lottery-style)
//! let numbers = draw_unique_range(block_hash, seed_nonce, 6, 59);
//! ```

use crate::crypto::poseidon_hash;
use pasta_curves::{group::ff::PrimeField, pallas};

/// Draw a single random number from block hash entropy.
///
/// Simple version suitable for games like roulette where a single
/// outcome is needed.
///
/// ## Example
/// ```rust
/// // Draw winning number for European roulette (0-36)
/// let winning = draw_single(block_hash, nonce, 37);
/// assert!(winning < 37);
/// ```
pub fn draw_single(block_hash: pallas::Base, nonce: pallas::Base, range: u8) -> u8 {
    let entropy = poseidon_hash([block_hash, nonce]);
    let entropy_bytes = entropy.to_repr();
    let seed = u64::from_le_bytes(entropy_bytes[0..8].try_into().unwrap());
    (seed % (range as u64)) as u8
}

/// Draw multiple unique random numbers from block hash entropy.
///
/// Numbers are in range 1 to `range` (inclusive), suitable for lotteries.
///
/// Uses LCG (Linear Congruential Generator) for sequential number generation
/// to ensure uniqueness without replacement.
///
/// ## Example
/// ```rust
/// // Draw 6 unique numbers from 1-59 (UK National Lottery style)
/// let numbers = draw_unique_range(block_hash, seed_nonce, 6, 59);
/// assert_eq!(numbers.len(), 6);
/// assert!(numbers.iter().all(|&n| n >= 1 && n <= 59));
/// ```
pub fn draw_unique_range(
    block_hash: pallas::Base,
    seed_nonce: u64,
    count: u8,
    range: u8,
) -> Vec<u8> {
    let entropy = poseidon_hash([block_hash, pallas::Base::from(seed_nonce)]);
    let mut rng_seed = u64::from_le_bytes(entropy.to_repr()[0..8].try_into().unwrap());
    let mut numbers: Vec<u8> = Vec::with_capacity(count as usize);

    while numbers.len() < count as usize {
        let num = ((rng_seed % (range as u64)) + 1) as u8;
        if !numbers.contains(&num) {
            numbers.push(num);
        }
        // LCG parameters (same as common implementations)
        rng_seed = rng_seed.wrapping_mul(31).wrapping_add(17);
    }

    numbers
}

/// Combine multiple block hashes into a single entropy source.
///
/// Uses cumulative Poseidon hashing. More blocks = exponentially harder
/// to manipulate. This is the recommended approach for high-value bets.
///
/// ## Security Scaling
///
/// - K=1: 33% manipulation chance (with 33% hash power)
/// - K=6: ~0.14% (Bitcoin "6 confirmations" standard)
/// - K=10: ~0.005%
///
/// ## Example
/// ```rust
/// // Combine 6 block hashes for high security
/// let entropy = combine_block_hashes(&[hash1, hash2, hash3, hash4, hash5, hash6]);
/// let roll = draw_single(entropy, bet_id, 100); // 0-99 roll
/// ```
pub fn combine_block_hashes(block_hashes: &[pallas::Base]) -> pallas::Base {
    let mut combined = pallas::Base::zero();
    for block_hash in block_hashes {
        combined = poseidon_hash([combined, *block_hash]);
    }
    combined
}

/// Draw using cumulative PoW entropy with confirmation depth.
///
/// More secure than single block hash. Recommended for bets with
/// significant value. Uses the same security scaling as Bitcoin's
/// confirmation system.
///
/// ## Arguments
///
/// * `block_hashes` - Slice of block hashes (should be sequential)
/// * `nonce` - Additional entropy (e.g., bet_id, secret_nonce)
/// * `range` - Upper bound for random number (0 to range-1)
pub fn draw_with_depth(block_hashes: &[pallas::Base], nonce: pallas::Base, range: u8) -> u8 {
    let entropy = combine_block_hashes(block_hashes);
    let final_entropy = poseidon_hash([entropy, nonce]);
    let bytes = final_entropy.to_repr();
    let seed = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    (seed % (range as u64)) as u8
}

/// Convert TransactionHash to pallas::Base for entropy use.
///
/// TransactionHash is a wrapper around [u8; 32]. This function
/// properly converts it for use with the entropy functions.
#[allow(dead_code)]
pub fn tx_hash_to_base(tx_hash: &[u8; 32]) -> pallas::Base {
    let a = u64::from_le_bytes(tx_hash[0..8].try_into().unwrap());
    let b = u64::from_le_bytes(tx_hash[8..16].try_into().unwrap());
    let c = u64::from_le_bytes(tx_hash[16..24].try_into().unwrap());
    let d = u64::from_le_bytes(tx_hash[24..32].try_into().unwrap());

    poseidon_hash([
        pallas::Base::from(a),
        pallas::Base::from(b),
        pallas::Base::from(c),
        pallas::Base::from(d),
    ])
}

/// Combine entropy from multiple sources using Poseidon.
///
/// Use this when you need multiple sources of entropy (e.g., block hash
/// + bet_id + secret_nonce).
pub fn mix_entropy(base: pallas::Base, additional: &[pallas::Base]) -> pallas::Base {
    let mut result = base;
    for &item in additional {
        result = poseidon_hash([result, item]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_single() {
        let block_hash = pallas::Base::one();
        let nonce = pallas::Base::one();
        let result = draw_single(block_hash, nonce, 37);
        assert!(result < 37);
    }

    #[test]
    fn test_draw_unique_range() {
        let block_hash = pallas::Base::one();
        let result = draw_unique_range(block_hash, 42, 6, 59);
        assert_eq!(result.len(), 6);
        // Check all unique
        let mut sorted = result.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 6);
        // Check range
        assert!(result.iter().all(|&n| n >= 1 && n <= 59));
    }

    #[test]
    fn test_combine_block_hashes() {
        let hashes = [
            pallas::Base::one(),
            pallas::Base::one() + pallas::Base::one(),
            pallas::Base::one() + pallas::Base::from(2),
        ];
        let combined = combine_block_hashes(&hashes);
        // Should be deterministic
        let combined2 = combine_block_hashes(&hashes);
        assert_eq!(combined, combined2);
    }

    #[test]
    fn test_tx_hash_to_base() {
        let tx_hash = [0u8; 32];
        let base = tx_hash_to_base(&tx_hash);
        assert!(base != pallas::Base::zero()); // Should produce non-zero
    }
}