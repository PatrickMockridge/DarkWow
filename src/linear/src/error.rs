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

//! Linear blockchain errors

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LinearError {
    #[error("Block not found at height {0}")]
    BlockNotFound(u64),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Invalid block hash")]
    InvalidBlockHash,

    #[error("Invalid previous block hash")]
    InvalidPreviousHash,

    #[error("Merkle root mismatch")]
    MerkleRootMismatch,

    #[error("Difficulty target not met")]
    DifficultyNotMet,

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Genesis block already exists")]
    GenesisExists,

    #[error("Invalid genesis block")]
    InvalidGenesis,

    #[error("RandomX error: {0}")]
    RandomXError(String),
}