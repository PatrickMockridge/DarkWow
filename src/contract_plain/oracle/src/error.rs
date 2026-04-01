/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Plain Oracle Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OraclePlainError {
    #[error("Feed not found")]
    FeedNotFound,

    #[error("Feed already exists")]
    FeedAlreadyExists,

    #[error("Data point not found")]
    DataPointNotFound,

    #[error("Staker not found")]
    StakerNotFound,

    #[error("Staker already exists")]
    StakerAlreadyExists,

    #[error("Invalid data value")]
    InvalidDataValue,

    #[error("Insufficient stake")]
    InsufficientStake,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Slashing threshold not met")]
    SlashingThresholdNotMet,

    #[error("Cross-contract call failed")]
    CrossContractFailed,
}

impl From<OraclePlainError> for ContractError {
    fn from(e: OraclePlainError) -> Self {
        match e {
            OraclePlainError::FeedNotFound => Self::Custom(1),
            OraclePlainError::FeedAlreadyExists => Self::Custom(2),
            OraclePlainError::DataPointNotFound => Self::Custom(3),
            OraclePlainError::StakerNotFound => Self::Custom(4),
            OraclePlainError::StakerAlreadyExists => Self::Custom(5),
            OraclePlainError::InvalidDataValue => Self::Custom(6),
            OraclePlainError::InsufficientStake => Self::Custom(7),
            OraclePlainError::UnauthorizedCaller => Self::Custom(8),
            OraclePlainError::InvalidSignature => Self::Custom(9),
            OraclePlainError::ArithmeticOverflow => Self::Custom(10),
            OraclePlainError::DivisionByZero => Self::Custom(11),
            OraclePlainError::InvalidFunction => Self::Custom(12),
            OraclePlainError::SlashingThresholdNotMet => Self::Custom(13),
            OraclePlainError::CrossContractFailed => Self::Custom(14),
        }
    }
}