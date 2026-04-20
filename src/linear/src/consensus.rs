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

//! Simple PoW consensus for linear blockchain

use blake3::Hash;

use super::{Block, Result};

/// Simple PoW consensus
pub struct PoWConsensus {
    difficulty_target: u32,
}

impl PoWConsensus {
    /// Create a new PoW consensus with given difficulty target
    pub fn new(difficulty_target: u32) -> Self {
        Self { difficulty_target }
    }

    /// Verify a block meets the difficulty target
    pub fn verify_proof(&self, block: &Block) -> Result<bool> {
        let hash = block.hash();
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        Ok(hash_u32 <= self.difficulty_target)
    }

    /// Check if the hash meets the difficulty target
    pub fn check_difficulty(&self, hash: &Hash) -> bool {
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        hash_u32 <= self.difficulty_target
    }
}

impl Default for PoWConsensus {
    fn default() -> Self {
        Self { difficulty_target: 0x0000_FFFF }
    }
}