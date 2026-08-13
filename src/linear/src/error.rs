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

//! Linear blockchain errors

use dwow_sdk::blockchain::BlockHeight;
use thiserror::Error;

/// All errors the linear blockchain can produce.
///
/// Structured so callers can match on variants and decide recovery
/// strategy (retry, ban peer, reject without ban). No `Custom(String)`
/// variants — every error carries typed context.
#[derive(Error, Debug)]
pub enum LinearError {
    // ---- Storage errors ----
    #[error("Block not found at height {0}")]
    BlockNotFound(BlockHeight),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    // ---- Block validation errors ----
    #[error("block {0} is invalid")]
    BlockIsInvalid(String),

    #[error("invalid proof-of-work for block {0}")]
    InvalidPoW(String),

    #[error("PoW target mismatch at height {height}: declared {declared}, expected {expected}")]
    InvalidTarget { declared: u32, expected: u32, height: BlockHeight },

    #[error("height discontinuity: expected {expected}, got {got}")]
    HeightDiscontinuity { expected: BlockHeight, got: BlockHeight },

    #[error("invalid previous hash for block {0}")]
    InvalidPreviousHash(String),

    #[error("merkle root mismatch for block {0}")]
    MerkleRootMismatch(String),

    #[error("uncle merkle root mismatch for block {0}")]
    UncleMerkleRootMismatch(String),

    #[error("uncle {0} proof verification failed")]
    UncleProofInvalid(String),

    #[error("uncle {uncle_height} too old: current {current}, max depth {max_depth}")]
    UncleTooOld { uncle_height: BlockHeight, current: BlockHeight, max_depth: u8 },

    #[error("duplicate uncle: {0}")]
    DuplicateUncle(String),

    #[error("uncle {0} PoW invalid")]
    UnclePoWInvalid(String),

    #[error("too many uncles: {count} exceeds maximum {max}")]
    TooManyUncles { count: usize, max: usize },

    #[error("block structure invalid: {0}")]
    BlockStructure(String),

    // ---- Consensus / config ----
    #[error("Difficulty target not met")]
    DifficultyNotMet,

    #[error("Cannot replace anchored block")]
    AnchoredBlockConflict,

    #[error("Genesis block already exists")]
    GenesisExists,

    #[error("Invalid timestamp {timestamp}: {reason}")]
    InvalidTimestamp { timestamp: u64, reason: String },

    #[error("Invalid genesis block")]
    InvalidGenesis,

    #[error("RandomX error: {0}")]
    RandomXError(String),

    #[error("Merge mining error: {0}")]
    MoneroMergeMineError(String),

    #[error("Monero hashing error: {0}")]
    MoneroHashingError(String),

    #[error("Monero number of chains is zero")]
    MoneroNumberOfChainZero,

    // ---- Concurrency errors ----
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
}

/// Validation phase during which an error was detected.
/// Maps 1:1 to the 7+1 phases in consensus.md:464-505 and type-system.md §4.1.
/// Each phase implies a specific recovery strategy — callers match on
/// `err.consensus_phase()` instead of string-matching on error barbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusPhase {
    /// Phase 0 — Structural validation (validate_block_structure).
    /// Recovery: reject block.
    Phase0Structural,
    /// Phase 1 — PoW verification.
    /// Recovery: reject block.
    Phase1PoW,
    /// Phase 2 — Chain continuity (height, previous hash).
    /// Recovery: reject block.
    Phase2Continuity,
    /// Phase 3 — Nullifier + ZK proof verification.
    /// Recovery: reject block, ban peer (↓bad-nullifier).
    Phase3Nullifier,
    /// Phase 4 — WASM execution (includes per-transaction validation,
    /// formerly separate Phase 5 in the spec; merged because transaction
    /// validation occurs during WASM execution, not as a separate pass).
    /// Recovery: reject block.
    Phase4Execution,
    /// Phase 7 — Atomic commit (sled write, includes nullifier-set update,
    /// formerly separate Phase 6 in the spec; merged because nullifier updates
    /// are part of the atomic cross-tree commit, not a separate pass).
    /// Recovery: fatal — restart node (↓db-fail).
    Phase7Commit,
}

impl LinearError {
    /// Map each error variant to the validation phase at which it was
    /// detected. Replaces the string-returning `error_barb()` with a
    /// typed enum — callers match on `err.consensus_phase()` for
    /// recovery strategy dispatch without string matching.
    pub fn consensus_phase(&self) -> ConsensusPhase {
        match self {
            // Phase 0 — Structural
            LinearError::BlockStructure(..) => ConsensusPhase::Phase0Structural,

            // Phase 1 — PoW
            LinearError::InvalidPoW(..)
            | LinearError::UnclePoWInvalid(..)
            | LinearError::DifficultyNotMet => ConsensusPhase::Phase1PoW,

            // Phase 2 — Chain continuity
            LinearError::HeightDiscontinuity { .. }
            | LinearError::InvalidPreviousHash(..)
            | LinearError::MerkleRootMismatch(..)
            | LinearError::InvalidTarget { .. }
            | LinearError::UncleMerkleRootMismatch(..)
            | LinearError::UncleProofInvalid(..)
            | LinearError::TooManyUncles { .. }
            | LinearError::UncleTooOld { .. }
            | LinearError::InvalidTimestamp { .. }
            | LinearError::InvalidGenesis => ConsensusPhase::Phase2Continuity,

            // Phase 3 — Nullifier + ZK
            LinearError::DuplicateUncle(..)
            | LinearError::AnchoredBlockConflict => ConsensusPhase::Phase3Nullifier,

            // Phase 4 — WASM execution
            | LinearError::BlockIsInvalid(..) => ConsensusPhase::Phase4Execution,

            // Phase 7 — Atomic commit / storage
            LinearError::BlockNotFound(..)
            | LinearError::TransactionNotFound(..)
            | LinearError::StorageError(..)
            | LinearError::SerializationError(..)
            | LinearError::RandomXError(..)
            | LinearError::LockPoisoned(..)
            | LinearError::MoneroMergeMineError(..)
            | LinearError::MoneroHashingError(..)
            | LinearError::MoneroNumberOfChainZero
            | LinearError::GenesisExists => ConsensusPhase::Phase7Commit,
        }
    }
}

impl From<std::io::Error> for LinearError {
    fn from(e: std::io::Error) -> Self {
        LinearError::SerializationError(e.to_string())
    }
}