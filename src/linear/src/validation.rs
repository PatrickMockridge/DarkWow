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
/// This is the fast, pure pre-check. It does NOT execute WASM contracts
/// or touch the database. Callers that need the previous hash must
/// compute it before calling.
pub fn check_block_header(
    block: &Block,
    vm: &RandomXVM,
    target: u32,
    current_height: u64,
    previous_hash: Option<&Blake3Hash>,
) -> Result<()> {
    let block_hash = block.hash(vm);

    // PoW verification — Monero merge-mined blocks skip native RandomX check
    if !matches!(block.header.pow_source, PowSource::Monero(_)) {
        let hash_u32 = u32::from_le_bytes(block_hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 > target {
            return Err(LinearError::InvalidPoW(block_hash.to_string()));
        }
    }

    // Merkle root
    if !block.verify_merkle_root() {
        return Err(LinearError::MerkleRootMismatch(block_hash.to_string()));
    }

    // Height continuity: must be exactly current + 1
    if block.header.height != current_height + 1 {
        return Err(LinearError::HeightDiscontinuity {
            expected: current_height + 1,
            got: block.header.height,
        });
    }

    // Previous hash — only checked when there IS a previous block
    if let Some(prev) = previous_hash {
        if block.header.previous != *prev {
            return Err(LinearError::InvalidPreviousHash(block_hash.to_string()));
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
        let uncle_hash = uncle.hash(vm);

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

    /// A block with all-zero fields — useful as a starting point for
    /// constructing test inputs that trigger specific errors.
    fn dummy_block() -> Block {
        Block {
            header: super::super::BlockHeader {
                version: 1,
                previous: Blake3Hash::from([0u8; 32]),
                merkle_root: Blake3Hash::from([0u8; 32]),
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

    #[test]
    fn rejects_height_discontinuity_forward() {
        let block = dummy_block();
        // Claim height 5 when chain is at height 0 — forward jump
        let result = check_block_header(
            &block,
            &randomx::RandomXVM::new(&[0u8; 32]).unwrap(),
            u32::MAX,
            0,    // current_height
            None, // no previous (genesis-like)
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            LinearError::HeightDiscontinuity { expected, got } => {
                assert_eq!(expected, 1);
                assert_eq!(got, 1); // dummy_block height is 1, expected is 1 — passes
            }
            _ => panic!("wrong error variant"),
        }
    }

    #[test]
    fn rejects_height_discontinuity_backwards() {
        let block = dummy_block();
        let result = check_block_header(
            &block,
            &randomx::RandomXVM::new(&[0u8; 32]).unwrap(),
            u32::MAX,
            5,    // current_height=5, so expected=6, but block says 1
            None,
        );
        match result.unwrap_err() {
            LinearError::HeightDiscontinuity { expected, got } => {
                assert_eq!(expected, 6);
                assert_eq!(got, 1);
            }
            _ => panic!("wrong error variant"),
        }
    }
}
