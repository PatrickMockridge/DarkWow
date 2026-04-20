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

/// Create a new block from transactions
pub fn create_block(
    previous: Hash,
    height: u64,
    transactions: Vec<Transaction>,
    difficulty_target: u32,
) -> Block {
    // Calculate merkle root
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
        },
        transactions,
    }
}