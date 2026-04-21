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

//! Block structures for linear blockchain

use blake3::Hash;
use serde::{Deserialize, Serialize};

use super::Transaction;

/// Block header - contains metadata about a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Block version
    pub version: u8,
    /// Hash of the previous block (only one parent - linear chain)
    pub previous: Hash,
    /// Merkle root of transactions
    pub merkle_root: Hash,
    /// Block timestamp
    pub timestamp: u64,
    /// Difficulty target for PoW
    pub difficulty_target: u32,
    /// Nonce for PoW mining
    pub nonce: u32,
    /// Block height in chain
    pub height: u64,
    /// Merkle root of uncle blocks referenced by this canonical block
    pub uncle_merkle_root: [u8; 32],
    /// Total reward being distributed (canonical + uncle shares)
    pub total_reward: u64,
}

/// Uncle block - a block that was mined but not canonical
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncleBlock {
    /// Header of the uncle block
    pub header: BlockHeader,
    /// Transactions in the uncle block
    pub transactions: Vec<Transaction>,
    /// Depth in the uncle tree (1 = directly referenced, 2 = referenced by depth-1, etc.)
    pub depth: u8,
}

impl UncleBlock {
    /// Calculate the hash of this uncle block's header
    pub fn hash(&self) -> Hash {
        blake3::hash(&serde_json::to_vec(&self.header).unwrap())
    }
}

/// Proof of an uncle for stateless verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncleProof {
    /// Uncle header
    pub header: BlockHeader,
    /// Merkle proof path from uncle to root
    pub merkle_path: Vec<[u8; 32]>,
    /// Uncle's position in merkle tree (leaf index)
    pub position: u32,
    /// Depth (for reward calculation)
    pub depth: u8,
}

/// Block - a single block in the linear chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block header
    pub header: BlockHeader,
    /// Transactions in this block
    pub transactions: Vec<Transaction>,
}

impl Block {
    /// Calculate the hash of this block's header
    pub fn hash(&self) -> Hash {
        blake3::hash(&serde_json::to_vec(&self.header).unwrap())
    }

    /// Verify the block's previous hash matches the expected parent
    pub fn verify_previous_hash(&self, expected_previous: Hash) -> bool {
        self.header.previous == expected_previous
    }

    /// Verify the merkle root matches the transactions
    pub fn verify_merkle_root(&self) -> bool {
        let tx_hashes: Vec<Hash> = self.transactions.iter().map(|tx| tx.hash()).collect();
        let computed_root = if tx_hashes.is_empty() {
            blake3::hash(&[])
        } else {
            // Simple merkle root computation
            let mut layer = tx_hashes.clone();
            while layer.len() > 1 {
                if layer.len() % 2 != 0 {
                    layer.push(layer.last().unwrap().clone());
                }
                layer = layer
                    .chunks(2)
                    .map(|pair| {
                        let mut combined = pair[0].as_bytes().to_vec();
                        combined.extend_from_slice(pair[1].as_bytes());
                        blake3::hash(&combined)
                    })
                    .collect();
            }
            layer[0].clone()
        };
        computed_root == self.header.merkle_root
    }
}

/// Verify an uncle proof against a merkle root
pub fn verify_uncle_proof(uncle: &UncleProof, merkle_root: &[u8; 32]) -> bool {
    // Compute expected root from proof
    let header_bytes = serde_json::to_vec(&uncle.header).unwrap();
    let mut current = blake3::hash(&header_bytes).as_bytes().to_vec();

    for (level, sibling) in uncle.merkle_path.iter().enumerate() {
        // At each level, the position bit tells us left/right
        let bit = (uncle.position >> level) & 1;
        let combined = if bit == 0 {
            // Current is left, sibling is right
            let mut c = current.clone();
            c.extend_from_slice(sibling);
            c
        } else {
            // Sibling is left, current is right
            let mut c = sibling.to_vec();
            c.extend_from_slice(&current);
            c
        };
        current = blake3::hash(&combined).as_bytes().to_vec();
    }
    current.as_slice() == merkle_root
}

/// Build uncle merkle tree from uncle blocks
/// Returns (merkle_root, proofs) for each uncle
pub fn build_uncle_merkle(uncles: &[UncleBlock]) -> ([u8; 32], Vec<UncleProof>) {
    if uncles.is_empty() {
        return ([0u8; 32], vec![]);
    }

    // Build leaves from uncle hashes, pad to even if needed
    let mut leaves: Vec<Hash> = uncles.iter().map(|u| u.hash()).collect();
    if leaves.len() % 2 != 0 {
        leaves.push(leaves.last().unwrap().clone());
    }

    // Build merkle tree bottom-up, storing each layer
    let mut layers: Vec<Vec<Hash>> = vec![leaves];
    while layers.last().unwrap().len() > 1 {
        let current = layers.last().unwrap();
        let mut next = Vec::new();
        for chunk in current.chunks(2) {
            debug_assert_eq!(chunk.len(), 2);
            let mut combined = chunk[0].as_bytes().to_vec();
            combined.extend_from_slice(chunk[1].as_bytes());
            next.push(blake3::hash(&combined));
        }
        layers.push(next);
    }
    let merkle_root: [u8; 32] = *layers.last().unwrap()[0].as_bytes();

    // Build proofs for each uncle
    let proofs: Vec<UncleProof> = (0..uncles.len())
        .map(|i| {
            let mut merkle_path = vec![];
            let mut pos = i;

            // Walk up the tree from leaf to root
            for level in 0..(layers.len() - 1) {
                let is_right = pos % 2 == 1;
                let sibling_pos = if is_right { pos - 1 } else { pos + 1 };
                let current_layer = &layers[level];

                debug_assert!(sibling_pos < current_layer.len());
                merkle_path.push(*current_layer[sibling_pos].as_bytes());

                pos /= 2;
            }

            UncleProof {
                header: uncles[i].header.clone(),
                merkle_path,
                position: i as u32,
                depth: uncles[i].depth,
            }
        })
        .collect();

    (merkle_root, proofs)
}

/// Compute reward distribution for canonical miner and uncles
/// Returns (canonical_reward, uncle_rewards)
pub fn compute_reward(base_reward: u64, uncles: &[UncleBlock]) -> (u64, Vec<u64>) {
    if uncles.is_empty() {
        return (base_reward, vec![]);
    }

    let mut uncle_rewards = Vec::with_capacity(uncles.len());
    let mut canonical_extra = 0u64;

    for uncle in uncles {
        let depth = uncle.depth.min(6) as u32;
        let reward = base_reward / (2_u64.pow(depth));
        uncle_rewards.push(reward);
        canonical_extra += reward;
    }

    (base_reward + canonical_extra, uncle_rewards)
}

/// Maximum uncle depth allowed
pub const MAX_UNCLE_DEPTH: u8 = 6;

/// Create a new block from transactions (no uncles - Phase 1)
pub fn create_block(
    previous: Hash,
    height: u64,
    transactions: Vec<Transaction>,
    difficulty_target: u32,
) -> Block {
    create_block_with_uncles(previous, height, transactions, difficulty_target, &[])
}

/// Create a new block with uncle blocks
pub fn create_block_with_uncles(
    previous: Hash,
    height: u64,
    transactions: Vec<Transaction>,
    difficulty_target: u32,
    uncles: &[UncleBlock],
) -> Block {
    // Calculate merkle root for transactions
    let tx_hashes: Vec<Hash> = transactions.iter().map(|tx| tx.hash()).collect();
    let merkle_root = if tx_hashes.is_empty() {
        blake3::hash(&[])
    } else {
        let mut layer = tx_hashes.clone();
        while layer.len() > 1 {
            if layer.len() % 2 != 0 {
                layer.push(layer.last().unwrap().clone());
            }
            layer = layer
                .chunks(2)
                .map(|pair| {
                    let mut combined = pair[0].as_bytes().to_vec();
                    combined.extend_from_slice(pair[1].as_bytes());
                    blake3::hash(&combined)
                })
                .collect();
        }
        layer[0].clone()
    };

    // Build uncle merkle and compute rewards
    let (uncle_merkle_root, _) = build_uncle_merkle(uncles);
    let base_reward = 100_000_000u64; // TODO: wire up to consensus params
    let (total_reward, _) = compute_reward(base_reward, uncles);

    Block {
        header: BlockHeader {
            version: 1,
            previous,
            merkle_root,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            difficulty_target,
            nonce: 0,
            height,
            uncle_merkle_root,
            total_reward,
        },
        transactions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_uncle_merkle_empty() {
        let (root, proofs) = build_uncle_merkle(&[]);
        assert_eq!(root, [0u8; 32]);
        assert!(proofs.is_empty());
    }

    #[test]
    fn test_build_uncle_merkle_single() {
        let uncle_header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 0,
            difficulty_target: 0x0000_FFFF,
            nonce: 0,
            height: 10,
            uncle_merkle_root: [0u8; 32],
            total_reward: 0,
        };
        let uncle = UncleBlock { header: uncle_header, transactions: vec![], depth: 1 };

        let (root, proofs) = build_uncle_merkle(&[uncle]);
        assert_ne!(root, [0u8; 32]);
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].depth, 1);
        assert_eq!(proofs[0].position, 0);
    }

    #[test]
    fn test_build_uncle_merkle_multiple() {
        let mut uncles = vec![];
        for i in 0..3 {
            let header = BlockHeader {
                version: 1,
                previous: blake3::hash(&[i]),
                merkle_root: blake3::hash(&[i]),
                timestamp: i as u64,
                difficulty_target: 0x0000_FFFF,
                nonce: i as u32,
                height: 10 + i as u64,
                uncle_merkle_root: [0u8; 32],
                total_reward: 0,
            };
            uncles.push(UncleBlock { header, transactions: vec![], depth: 1 });
        }

        let (root, proofs) = build_uncle_merkle(&uncles);
        assert_ne!(root, [0u8; 32]);
        assert_eq!(proofs.len(), 3);
        for (i, proof) in proofs.iter().enumerate() {
            assert_eq!(proof.position, i as u32);
            assert!(verify_uncle_proof(proof, &root));
        }
    }

    #[test]
    fn test_compute_reward_no_uncles() {
        let (canonical, uncles) = compute_reward(100_000_000, &[]);
        assert_eq!(canonical, 100_000_000);
        assert!(uncles.is_empty());
    }

    #[test]
    fn test_compute_reward_with_uncles() {
        let uncle_header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 0,
            difficulty_target: 0x0000_FFFF,
            nonce: 0,
            height: 10,
            uncle_merkle_root: [0u8; 32],
            total_reward: 0,
        };
        let uncle = UncleBlock { header: uncle_header, transactions: vec![], depth: 1 };

        let (canonical, uncle_rewards) = compute_reward(100_000_000, &[uncle]);
        // base 100M + depth-1 uncle = 50M = 150M canonical
        assert_eq!(canonical, 150_000_000);
        assert_eq!(uncle_rewards.len(), 1);
        assert_eq!(uncle_rewards[0], 50_000_000);
    }

    #[test]
    fn test_verify_uncle_proof() {
        let header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 0,
            difficulty_target: 0x0000_FFFF,
            nonce: 42,
            height: 10,
            uncle_merkle_root: [0u8; 32],
            total_reward: 0,
        };
        let uncle = UncleBlock { header: header.clone(), transactions: vec![], depth: 1 };

        let (root, proofs) = build_uncle_merkle(&[uncle]);
        assert!(verify_uncle_proof(&proofs[0], &root));

        // Verify with wrong root fails
        assert!(!verify_uncle_proof(&proofs[0], &[1u8; 32]));
    }

    #[test]
    fn test_create_block_with_uncles() {
        let previous = blake3::hash(b"genesis");
        let block = create_block_with_uncles(
            previous,
            1,
            vec![],
            0x0000_FFFF,
            &[],
        );

        assert_eq!(block.header.previous, previous);
        assert_eq!(block.header.height, 1);
        assert_eq!(block.header.uncle_merkle_root, [0u8; 32]);
        // With no uncles, total_reward = base_reward
        assert_eq!(block.header.total_reward, 100_000_000);
    }
}