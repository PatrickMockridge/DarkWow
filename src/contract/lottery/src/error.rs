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

//! Lottery Contract Errors

use dwow_sdk::error::ContractError;
use thiserror::Error;

/// Errors occurring in the Lottery contract
#[derive(Debug, Error)]
pub enum LotteryError {
    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Invalid lottery configuration")]
    InvalidConfig,

    #[error("Invalid number of picks")]
    InvalidNumPicks,

    #[error("Invalid number range")]
    InvalidNumberRange,

    #[error("Invalid house edge")]
    InvalidHouseEdge,

    #[error("Number out of range")]
    NumberOutOfRange,

    #[error("Duplicate numbers in selection")]
    DuplicateNumbers,

    #[error("Invalid ticket commitment")]
    InvalidCommitment,

    #[error("Ticket not found")]
    TicketNotFound,

    #[error("Lottery not found")]
    LotteryNotFound,

    #[error("Lottery not accepting tickets")]
    LotteryNotActive,

    #[error("Lottery already drawn")]
    LotteryAlreadyDrawn,

    #[error("Lottery already expired")]
    LotteryAlreadyExpired,

    #[error("Draw not yet available")]
    DrawNotYetAvailable,

    #[error("Claim period expired")]
    ClaimPeriodExpired,

    #[error("Ticket already claimed")]
    TicketAlreadyClaimed,

    #[error("Invalid ticket reveal")]
    InvalidReveal,

    #[error("Insufficient prize tier matches")]
    InsufficientMatches,

    #[error("Claim not found")]
    ClaimNotFound,

    #[error("Prize already claimed")]
    PrizeAlreadyClaimed,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid merkle proof")]
    InvalidMerkleProof,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Value mismatch")]
    ValueMismatch,

    #[error("Token ID mismatch")]
    TokenIdMismatch,

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("ZK circuit verification failed")]
    ZkVerificationFailed,

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Invalid children indexes: expected promissory_note::transfer_v1 calls")]
    InvalidChildrenIndexes,

    #[error("Invalid child call: expected promissory_note::transfer_v1")]
    InvalidChildCall,
}

impl From<LotteryError> for ContractError {
    fn from(e: LotteryError) -> Self {
        match e {
            LotteryError::InvalidFunction => Self::Custom(1),
            LotteryError::InvalidConfig => Self::Custom(2),
            LotteryError::InvalidNumPicks => Self::Custom(3),
            LotteryError::InvalidNumberRange => Self::Custom(4),
            LotteryError::InvalidHouseEdge => Self::Custom(5),
            LotteryError::NumberOutOfRange => Self::Custom(6),
            LotteryError::DuplicateNumbers => Self::Custom(7),
            LotteryError::InvalidCommitment => Self::Custom(8),
            LotteryError::TicketNotFound => Self::Custom(9),
            LotteryError::LotteryNotFound => Self::Custom(10),
            LotteryError::LotteryNotActive => Self::Custom(11),
            LotteryError::LotteryAlreadyDrawn => Self::Custom(12),
            LotteryError::LotteryAlreadyExpired => Self::Custom(13),
            LotteryError::DrawNotYetAvailable => Self::Custom(14),
            LotteryError::ClaimPeriodExpired => Self::Custom(15),
            LotteryError::TicketAlreadyClaimed => Self::Custom(16),
            LotteryError::InvalidReveal => Self::Custom(17),
            LotteryError::InsufficientMatches => Self::Custom(18),
            LotteryError::ClaimNotFound => Self::Custom(19),
            LotteryError::PrizeAlreadyClaimed => Self::Custom(20),
            LotteryError::UnauthorizedCaller => Self::Custom(21),
            LotteryError::InvalidSignature => Self::Custom(22),
            LotteryError::InvalidMerkleProof => Self::Custom(23),
            LotteryError::InvalidNullifier => Self::Custom(24),
            LotteryError::ValueMismatch => Self::Custom(25),
            LotteryError::TokenIdMismatch => Self::Custom(26),
            LotteryError::DatabaseError(_) => Self::Custom(27),
            LotteryError::SerializationError(_) => Self::Custom(28),
            LotteryError::ZkVerificationFailed => Self::Custom(29),
            LotteryError::InternalError(_) => Self::Custom(30),
            LotteryError::InvalidChildrenIndexes => Self::Custom(31),
            LotteryError::InvalidChildCall => Self::Custom(32),
        }
    }
}
