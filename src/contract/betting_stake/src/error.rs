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

//! Betting Stake Contract Errors

use dwow_sdk::error::ContractError;
use thiserror::Error;

/// Errors occurring in the Betting Stake contract
#[derive(Debug, Error)]
pub enum BettingStakeError {
    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Stake not found")]
    StakeNotFound,

    #[error("Table not found")]
    TableNotFound,

    #[error("Stake amount too small")]
    StakeTooSmall,

    #[error("Insufficient capital for stake")]
    InsufficientCapital,

    #[error("Stake exceeds maximum ratio")]
    StakeExceedsMaxRatio,

    #[error("No earnings to claim")]
    NoEarnings,

    #[error("Invalid earnings calculation")]
    InvalidEarnings,

    #[error("Stake still locked")]
    StakeLocked,

    #[error("Unlock period not expired")]
    UnlockNotExpired,

    #[error("Risk update would exceed stake")]
    RiskExceedsStake,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Duplicate nullifier")]
    DuplicateNullifier,

    #[error("Value mismatch")]
    ValueMismatch,

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Arithmetic overflow in calculation")]
    ArithmeticOverflow,

    #[error("Invalid children indexes for child call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call")]
    InvalidChildCall,
}

impl From<BettingStakeError> for ContractError {
    fn from(e: BettingStakeError) -> Self {
        match e {
            BettingStakeError::InvalidFunction => Self::Custom(1),
            BettingStakeError::StakeNotFound => Self::Custom(2),
            BettingStakeError::TableNotFound => Self::Custom(3),
            BettingStakeError::StakeTooSmall => Self::Custom(4),
            BettingStakeError::InsufficientCapital => Self::Custom(5),
            BettingStakeError::StakeExceedsMaxRatio => Self::Custom(6),
            BettingStakeError::NoEarnings => Self::Custom(7),
            BettingStakeError::InvalidEarnings => Self::Custom(8),
            BettingStakeError::StakeLocked => Self::Custom(9),
            BettingStakeError::UnlockNotExpired => Self::Custom(10),
            BettingStakeError::RiskExceedsStake => Self::Custom(11),
            BettingStakeError::UnauthorizedCaller => Self::Custom(12),
            BettingStakeError::InvalidSignature => Self::Custom(13),
            BettingStakeError::DuplicateNullifier => Self::Custom(20),
            BettingStakeError::ValueMismatch => Self::Custom(14),
            BettingStakeError::DatabaseError(_) => Self::Custom(15),
            BettingStakeError::InternalError(_) => Self::Custom(16),
            BettingStakeError::ArithmeticOverflow => Self::Custom(17),
            BettingStakeError::InvalidChildrenIndexes => Self::Custom(18),
            BettingStakeError::InvalidChildCall => Self::Custom(19),
        }
    }
}
