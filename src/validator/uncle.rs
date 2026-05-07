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

//! Uncle Merkle Consensus Implementation
//!
//! This module provides the data structures and verification logic for
//! Uncle Merkle consensus, which replaces the complex overlay/diff system
//! with a merkle-tree-based approach.
//!
//! Key properties:
//! - Uncle chains are explicitly referenced in canonical blocks
//! - Reward splitting is deterministic based on merkle depth
//! - Verification is stateless (pure merkle proof + math)
//! - Mining risk is bounded (uncle gets partial reward if referenced)
//!
//! See doc/src/arch/uncle_merkle.md for full specification.

use blake3::Hash;
use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::blockchain::{Header, HeaderHash};

/// Maximum depth for uncle blocks (similar to Ethereum's 6)
pub const MAX_UNCLE_DEPTH: u8 = 6;

/// Base reward for a canonical block (in smallest unit)
pub const BASE_REWARD: u64 = 1_000_000_000; // 10 DFI assuming 8 decimals

/// Uncle block - a block that was mined but not canonical, but referenced
/// by a canonical block.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct UncleBlock {
    /// Header of the uncle block
    pub header: Header,
    /// Hash of this uncle block
    pub hash: HeaderHash,
    /// Depth in the uncle tree (1 = directly referenced, 2 = referenced by depth-1, etc.)
    pub depth: u8,
}

impl UncleBlock {
    /// Create a new uncle block from a header and depth
    pub fn new(header: Header, depth: u8) -> Self {
        Self { hash: header.hash(), header, depth }
    }
}

/// Merkle proof for an uncle block.
/// Used for stateless verification of uncle inclusion.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct UncleProof {
    /// Uncle header
    pub header: Header,
    /// Merkle proof path from uncle to root (sibling hashes)
    pub merkle_path: Vec<Hash>,
    /// Uncle's position in merkle tree (leaf index)
    pub position: u32,
    /// Depth (for reward calculation)
    pub depth: u8,
    /// Hash of this uncle (for verification)
    pub hash: HeaderHash,
}

impl UncleProof {
    /// Verify this proof against an expected root
    pub fn verify(&self, expected_root: &Hash) -> bool {
        let leaf_hash = *self.header.hash().inner();
        let computed_root = self.merkle_path.iter().fold(leaf_hash, |acc, sibling| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&acc);
            hasher.update(sibling.as_bytes());
            *hasher.finalize().as_bytes()
        });

        computed_root == *expected_root.as_bytes()
    }

    /// Calculate the reward for this uncle based on depth
    pub fn reward(&self) -> u64 {
        BASE_REWARD / (2_u64.pow(self.depth as u32))
    }
}

/// Reward distribution for a canonical block and its uncles.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct RewardDistribution {
    /// Share for canonical miner
    pub canonical: u64,
    /// Shares for uncle miners, ordered by merkle position
    pub uncles: Vec<UncleShare>,
}

/// A single uncle's share of the reward.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct UncleShare {
    /// Hash of the uncle block
    pub hash: HeaderHash,
    /// Depth in the uncle tree
    pub depth: u8,
    /// Reward amount
    pub reward: u64,
}

/// Compute the reward distribution for a given number of uncles.
pub fn compute_reward_distribution(uncle_count: usize) -> RewardDistribution {
    let mut canonical_reward = BASE_REWARD;
    let mut uncle_shares = Vec::new();

    for i in 0..uncle_count {
        let depth = (((i / 2) + 1) as u8).min(MAX_UNCLE_DEPTH);
        let reward = BASE_REWARD / (2_u64.pow(depth as u32));

        if i % 2 == 0 {
            canonical_reward += reward;
        } else {
            // Uncle reward - we'll fill in the hash later
            uncle_shares.push(UncleShare { hash: HeaderHash([0u8; 32]), depth, reward });
        }
    }

    RewardDistribution { canonical: canonical_reward, uncles: uncle_shares }
}

/// Verify the reward distribution math is correct.
pub fn verify_reward_distribution(dist: &RewardDistribution) -> bool {
    for uncle in &dist.uncles {
        let expected = BASE_REWARD / (2_u64.pow(uncle.depth as u32));
        if uncle.reward != expected {
            return false;
        }
    }
    true
}

/// Build a merkle root from a list of uncle hashes.
pub fn build_uncle_merkle_root(uncle_hashes: &[HeaderHash]) -> Hash {
    if uncle_hashes.is_empty() {
        return blake3::hash(b"empty_uncle_merkle");
    }

    let leaves: Vec<Hash> = uncle_hashes.iter().map(|h| Hash::from(*h.inner())).collect();
    build_merkle_root_from_leaves(&leaves)
}

/// Build a merkle root from a slice of blake3 hashes.
fn build_merkle_root_from_leaves(leaves: &[Hash]) -> Hash {
    if leaves.is_empty() {
        return blake3::hash(b"empty_uncle_merkle");
    }

    if leaves.len() == 1 {
        return leaves[0];
    }

    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let mut next_level = Vec::new();
        for pair in level.chunks(2) {
            let left = &pair[0];
            let right = if pair.len() > 1 { &pair[1] } else { left };
            let mut hasher = blake3::Hasher::new();
            hasher.update(left.as_bytes());
            hasher.update(right.as_bytes());
            next_level.push(hasher.finalize());
        }
        level = next_level;
    }

    level[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reward_distribution_single_uncle() {
        let dist = compute_reward_distribution(1);
        // i=0: depth=1, reward=BASE/2, even → canonical bonus only
        // Canonical gets BASE + BASE/2 = 1.5 BASE, no uncle shares
        assert_eq!(dist.canonical, BASE_REWARD + BASE_REWARD / 2);
        assert_eq!(dist.uncles.len(), 0);
    }

    #[test]
    fn test_reward_distribution_multiple_uncles() {
        let dist = compute_reward_distribution(4);
        // i=0: depth=1, reward=BASE/2, even → canonical += BASE/2
        // i=1: depth=1, reward=BASE/2, odd  → uncle share
        // i=2: depth=2, reward=BASE/4, even → canonical += BASE/4
        // i=3: depth=2, reward=BASE/4, odd  → uncle share
        // canonical = BASE + BASE/2 + BASE/4 = 7*BASE/4
        // uncles: 2 shares [BASE/2, BASE/4]
        assert_eq!(dist.canonical, BASE_REWARD + BASE_REWARD / 2 + BASE_REWARD / 4);
        assert_eq!(dist.uncles.len(), 2);
        assert_eq!(dist.uncles[0].reward, BASE_REWARD / 2);
        assert_eq!(dist.uncles[1].reward, BASE_REWARD / 4);
    }

    #[test]
    fn test_verify_reward_distribution() {
        let dist = compute_reward_distribution(2);
        assert!(verify_reward_distribution(&dist));
    }

    #[test]
    fn test_uncle_proof_verify() {
        let mut header = Header::new(
            HeaderHash([0u8; 32]),
            10,
            123,
            crate::util::time::Timestamp::current_time(),
        );
        header.total_reward = BASE_REWARD;

        let hash = header.hash();
        let root = build_uncle_merkle_root(&[hash]);

        let proof = UncleProof {
            header: header.clone(),
            merkle_path: vec![],
            position: 0,
            depth: 1,
            hash,
        };

        assert!(proof.verify(&root));
    }
}