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

//! Roulette Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

/// Errors occurring in the Roulette contract
#[derive(Debug, Error)]
pub enum RouletteError {
    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Table not found")]
    TableNotFound,

    #[error("Table not accepting bets")]
    TableNotActive,

    #[error("Table already closed")]
    TableAlreadyClosed,

    #[error("Invalid bet amount")]
    InvalidBetAmount,

    #[error("Bet exceeds table maximum")]
    BetExceedsMaximum,

    #[error("Insufficient table capital")]
    InsufficientCapital,

    #[error("Invalid bet type")]
    InvalidBetType,

    #[error("Invalid numbers for bet type")]
    InvalidNumbers,

    #[error("Numbers already bet on this spin")]
    DuplicateBet,

    #[error("Bet not found")]
    BetNotFound,

    #[error("Bet already settled")]
    BetAlreadySettled,

    #[error("Spin not ready")]
    SpinNotReady,

    #[error("Wheel already spun")]
    WheelAlreadySpun,

    #[error("No bets placed")]
    NoBetsPlaced,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Value mismatch")]
    ValueMismatch,

    #[error("Cross-contract call failed")]
    CrossContractFailed,

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<RouletteError> for ContractError {
    fn from(e: RouletteError) -> Self {
        match e {
            RouletteError::InvalidFunction => Self::Custom(1),
            RouletteError::TableNotFound => Self::Custom(2),
            RouletteError::TableNotActive => Self::Custom(3),
            RouletteError::TableAlreadyClosed => Self::Custom(4),
            RouletteError::InvalidBetAmount => Self::Custom(5),
            RouletteError::BetExceedsMaximum => Self::Custom(6),
            RouletteError::InsufficientCapital => Self::Custom(7),
            RouletteError::InvalidBetType => Self::Custom(8),
            RouletteError::InvalidNumbers => Self::Custom(9),
            RouletteError::DuplicateBet => Self::Custom(10),
            RouletteError::BetNotFound => Self::Custom(11),
            RouletteError::BetAlreadySettled => Self::Custom(12),
            RouletteError::SpinNotReady => Self::Custom(13),
            RouletteError::WheelAlreadySpun => Self::Custom(14),
            RouletteError::NoBetsPlaced => Self::Custom(15),
            RouletteError::UnauthorizedCaller => Self::Custom(16),
            RouletteError::InvalidSignature => Self::Custom(17),
            RouletteError::ValueMismatch => Self::Custom(18),
            RouletteError::CrossContractFailed => Self::Custom(19),
            RouletteError::DatabaseError(_) => Self::Custom(20),
            RouletteError::InternalError(_) => Self::Custom(21),
        }
    }
}
