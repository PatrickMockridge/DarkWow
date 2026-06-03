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

//! Pure block validation functions.
//!
//! Every function in this module is **pure**: it takes data in, returns a
//! `Result` out. No sled, no locks, no async, no side effects.
//!
//! This makes each check independently testable with a standard `#[test]`
//! — construct minimal inputs, call the function, assert the outcome.

use std::collections::HashSet;

use blake3::Hash as Blake3Hash;
use randomx::RandomXVM;

use super::{
    build_uncle_merkle, verify_uncle_proof, Block, LinearError, PowSource, Result, UncleBlock,
};

/// Verify a block header against all consensus rules.
///
/// Two-stage PoW validation (Bitcoin Core pattern):
///   Stage 1: `hash_u32 <= block.header.target` — hash meets header's target.
///   Stage 2: `block.header.target == expected_target` — target matches
///            consensus rules (GetNextWorkRequired). This prevents
///            self-declared-target attacks.
///
/// For genesis (height=1), `get_next_work_required(1)` returns `u32::MAX`,
/// so the declared target of `u32::MAX` passes Stage 2.
///
/// Pure — does NOT execute WASM or touch the database.
pub fn check_block_header(
    block: &Block,
    vm: &RandomXVM,
    expected_target: u32,
    current_height: u64,
    previous_hash: Option<&Blake3Hash>,
) -> Result<()> {
    let block_hash = block.hash_with_vm(&vm);

    // Stage 1: PoW — hash must meet the block header's own target.
    // Monero merge-mined blocks skip native RandomX check.
    if !matches!(block.header.pow_source, PowSource::Monero(_)) {
        let hash_u32 = u32::from_le_bytes(block_hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 > block.header.target {
            return Err(LinearError::InvalidPoW(block_hash.to_string()));
        }
    }

    // Height continuity: must be exactly current + 1.
    // Checked BEFORE previous hash and target — structural errors fail fast.
    if block.header.height != current_height + 1 {
        return Err(LinearError::HeightDiscontinuity {
            expected: current_height + 1,
            got: block.header.height,
        });
    }

    // Previous hash — fork detection MUST come before Stage 2 target.
    // A block from a different fork will have the wrong previous_hash.
    // Failing here with InvalidPreviousHash is the correct diagnostic.
    // Previously this was checked AFTER Stage 2 target, causing fork blocks
    // to fail with misleading "target mismatch" errors.
    if let Some(prev) = previous_hash {
        if block.header.previous != *prev {
            return Err(LinearError::InvalidPreviousHash(block_hash.to_string()));
        }
    }

    // Merkle root
    if !block.verify_merkle_root() {
        return Err(LinearError::MerkleRootMismatch(block_hash.to_string()));
    }

    // Stage 2: The block's declared target must match what consensus rules
    // require for this height. Only reached if the block connects to our
    // canonical chain (previous hash matched above).
    if block.header.target != expected_target {
        return Err(LinearError::InvalidTarget {
            declared: block.header.target,
            expected: expected_target,
            height: block.header.height,
        });
    }

    Ok(())
}

/// Validate block timestamp against consensus rules (CRITICAL-4 fix).
///
/// Bitcoin Core's CheckBlockTimestamp pattern:
/// 1. Timestamp must not be more than MAX_FUTURE (2 hours) ahead of local time.
/// 2. Timestamp must be strictly greater than the median of the last
///    MEDIAN_BLOCK_COUNT (11) block timestamps (time warp protection).
///
/// Pure — does not touch sled. Caller provides the block heights and timestamps.
pub fn check_block_timestamp(
    timestamp: u64,
    height: u64,
    recent_timestamps: &[u64],
) -> Result<()> {
    const MAX_FUTURE: u64 = 2 * 60 * 60; // 2 hours
    const MEDIAN_BLOCK_COUNT: usize = 11;

    // Future timestamp check
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if timestamp > now + MAX_FUTURE {
        return Err(LinearError::InvalidTimestamp {
            timestamp,
            reason: "timestamp too far in the future".to_string(),
        });
    }

    // Median of last N blocks (time warp protection)
    if height > 1 && recent_timestamps.len() >= MEDIAN_BLOCK_COUNT {
        let mut sorted: Vec<u64> = recent_timestamps.to_vec();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        if timestamp <= median {
            return Err(LinearError::InvalidTimestamp {
                timestamp,
                reason: format!("timestamp must be > median of last {} blocks", MEDIAN_BLOCK_COUNT),
            });
        }
    }

    Ok(())
}

/// Verify uncle blocks against all consensus rules.
///
/// Pure — the caller provides the pre-computed uncle merkle root,
/// proofs, and the set of already-stored uncle keys (from the database).
/// This function does not touch sled.
pub fn check_uncles(
    uncles: &[UncleBlock],
    proofs: &[super::UncleProof],
    expected_uncle_root: &[u8; 32],
    current_height: u64,
    vm: &RandomXVM,
    target: u32,
    existing_uncle_keys: &HashSet<[u8; 32]>,
) -> Result<()> {
    // Verify the uncle merkle root matches
    let (computed_root, _) = build_uncle_merkle(uncles, vm);
    if computed_root != *expected_uncle_root {
        return Err(LinearError::UncleMerkleRootMismatch(
            hex::encode(expected_uncle_root),
        ));
    }

    for (i, uncle) in uncles.iter().enumerate() {
        let uncle_hash = uncle.hash_with_vm(&vm);

        // PoW for this uncle
        let hash_u32 = u32::from_le_bytes(uncle_hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 > target {
            return Err(LinearError::UnclePoWInvalid(uncle_hash.to_string()));
        }

        // Merkle proof against the canonical block's uncle_merkle_root
        if !verify_uncle_proof(&proofs[i], expected_uncle_root, vm, target) {
            return Err(LinearError::UncleProofInvalid(uncle_hash.to_string()));
        }

        // Recency: uncle must not be too old
        let min_allowed = current_height.saturating_sub(super::MAX_UNCLE_DEPTH as u64);
        if uncle.header.height <= min_allowed {
            return Err(LinearError::UncleTooOld {
                uncle_height: uncle.header.height,
                current: current_height,
                max_depth: super::MAX_UNCLE_DEPTH,
            });
        }

        // Uniqueness: uncle must not already be in the chain
        let uncle_key: [u8; 32] =
            blake3::hash(&serde_json::to_vec(&uncle.header).unwrap()).into();
        if existing_uncle_keys.contains(&uncle_key) {
            return Err(LinearError::DuplicateUncle(uncle_hash.to_string()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block with correct defaults for empty transactions.
    /// merkle_root for 0 txs is blake3::hash(&[]).
    fn dummy_block() -> Block {
        Block {
            header: super::super::BlockHeader {
                version: 1,
                previous: Blake3Hash::from([0u8; 32]),
                merkle_root: blake3::hash(&[]), // correct for 0 transactions
                timestamp: 0,
                target: u32::MAX,
                nonce: 0,
                height: 1,
                uncle_merkle_root: [0u8; 32],
                total_reward: 0,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: 0,
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: PowSource::Native,
            },
            transactions: vec![],
        }
    }

    /// Create a VM suitable for tests using the recommended flags.
    fn test_vm() -> randomx::RandomXVM {
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &[0u8; 32]).unwrap();
        randomx::RandomXVM::new(flags, Some(cache), None).unwrap()
    }

    #[test]
    fn rejects_height_discontinuity_forward() {
        let mut block = dummy_block();
        block.header.height = 5; // claim 5 when chain is at 0 — expected 1
        let err = check_block_header(
            &block,
            &test_vm(),
            u32::MAX, // expected_target (matches block.header.target = u32::MAX)
            0,         // current_height
            None,      // no previous (genesis-like)
        ).unwrap_err();
        match err {
            LinearError::HeightDiscontinuity { expected, got } => {
                assert_eq!(expected, 1);
                assert_eq!(got, 5);
            }
            e => panic!("wrong error variant: {:?}", e),
        }
    }

    #[test]
    fn rejects_height_discontinuity_backwards() {
        let block = dummy_block();
        let err = check_block_header(
            &block,
            &test_vm(),
            u32::MAX, // expected_target (must match block.header.target = u32::MAX)
            5,         // current_height=5, so expected=6, but block says 1
            None,
        ).unwrap_err();
        match err {
            LinearError::HeightDiscontinuity { expected, got } => {
                assert_eq!(expected, 6);
                assert_eq!(got, 1);
            }
            e => panic!("wrong error variant: {:?}", e),
        }
    }

    /// Stage 2 PoW: a block mined with u32::MAX target at height > 1
    /// must be rejected because the consensus target is lower.
    #[test]
    fn rejects_target_mismatch_above_genesis() {
        let block = dummy_block();
        // Block claims target=u32::MAX but consensus says 0x0FFFFFFF at height 2
        let err = check_block_header(
            &block,
            &test_vm(),
            0x0FFF_FFFF, // expected_target for height > 1
            1,            // current_height=1 (past genesis)
            None,
        ).unwrap_err();
        match err {
            LinearError::InvalidTarget { declared, expected, height } => {
                assert_eq!(declared, u32::MAX);
                assert_eq!(expected, 0x0FFF_FFFF);
                assert_eq!(height, 1); // block header height
            }
            e => panic!("wrong error variant: {:?}", e),
        }
    }

    /// Stage 2 PoW: a block with matching target and u32::MAX (guaranteed
    /// PoW pass) succeeds validation when merkle root is correct.
    #[test]
    fn accepts_matching_target_and_pow() {
        let mut block = dummy_block();
        block.header.target = u32::MAX;
        block.header.height = 2;
        // expected_target = u32::MAX matches header target → stage 2 passes
        // hash_u32 <= u32::MAX → stage 1 always passes
        // merkle_root = blake3::hash(&[]) matches 0 transactions → passes
        let result = check_block_header(
            &block,
            &test_vm(),
            u32::MAX,    // expected_target matches block.header.target
            1,            // current_height=1, expected height=2
            None,
        );
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }
}
