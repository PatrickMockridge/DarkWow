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

//! DarkBet Exchange Contract Errors

use dwow_sdk::error::ContractError;
use thiserror::Error;

/// Errors occurring in the Darkbet Exchange contract
#[derive(Debug, Error)]
pub enum DarkbetError {
    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Market not found")]
    MarketNotFound,

    #[error("Market not accepting orders")]
    MarketNotOpen,

    #[error("Market already closed")]
    MarketAlreadyClosed,

    #[error("Market already resolved")]
    MarketAlreadyResolved,

    #[error("Invalid odds")]
    InvalidOdds,

    #[error("Invalid duration")]
    InvalidDuration,

    #[error("Invalid commission")]
    InvalidCommission,

    #[error("Insufficient stake")]
    InsufficientStake,

    #[error("Order not found")]
    OrderNotFound,

    #[error("Match not found")]
    MatchNotFound,

    #[error("Order already matched")]
    OrderAlreadyMatched,

    #[error("No matching orders available")]
    NoMatchingOrders,

    #[error("Odds mismatch - spread too wide")]
    OddsMismatch,

    #[error("Invalid outcome")]
    InvalidOutcome,

    #[error("Market not resolved yet")]
    MarketNotResolved,

    #[error("Market already settled")]
    MarketAlreadySettled,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Value mismatch")]
    ValueMismatch,

    #[error("Invalid market type")]
    InvalidMarketType,

    #[error("Invalid fee")]
    InvalidFee,

    #[error("Slippage exceeded - payout below minimum")]
    SlippageExceeded,

    #[error("Position not found")]
    PositionNotFound,

    #[error("LP share not found")]
    LpShareNotFound,

    #[error("Position already claimed")]
    PositionAlreadyClaimed,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Cross-contract call failed")]
    CrossContractFailed,

    #[error("Oracle signature verification failed")]
    InvalidOracleSignature,

    #[error("Market already exists")]
    MarketAlreadyExists,

    #[error("Cross-contract call requires authorized contract")]
    UnauthorizedCrossContract,

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Invalid children_indexes: expected 1 promissory_note::transfer_v1 child call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call: expected promissory_note::transfer_v1 (0x04)")]
    InvalidChildCall,

    #[error("Nullifier already spent")]
    DuplicateNullifier,
}

impl From<DarkbetError> for ContractError {
    fn from(e: DarkbetError) -> Self {
        match e {
            DarkbetError::InvalidFunction => Self::Custom(1),
            DarkbetError::MarketNotFound => Self::Custom(2),
            DarkbetError::MarketNotOpen => Self::Custom(3),
            DarkbetError::MarketAlreadyClosed => Self::Custom(4),
            DarkbetError::MarketAlreadyResolved => Self::Custom(5),
            DarkbetError::InvalidOdds => Self::Custom(6),
            DarkbetError::InvalidDuration => Self::Custom(21),
            DarkbetError::InvalidCommission => Self::Custom(22),
            DarkbetError::InsufficientStake => Self::Custom(7),
            DarkbetError::OrderNotFound => Self::Custom(8),
            DarkbetError::MatchNotFound => Self::Custom(33),
            DarkbetError::OrderAlreadyMatched => Self::Custom(9),
            DarkbetError::NoMatchingOrders => Self::Custom(10),
            DarkbetError::OddsMismatch => Self::Custom(11),
            DarkbetError::InvalidOutcome => Self::Custom(12),
            DarkbetError::MarketNotResolved => Self::Custom(13),
            DarkbetError::MarketAlreadySettled => Self::Custom(14),
            DarkbetError::UnauthorizedCaller => Self::Custom(15),
            DarkbetError::InvalidSignature => Self::Custom(16),
            DarkbetError::ValueMismatch => Self::Custom(17),
            DarkbetError::InvalidMarketType => Self::Custom(23),
            DarkbetError::InvalidFee => Self::Custom(24),
            DarkbetError::SlippageExceeded => Self::Custom(25),
            DarkbetError::PositionNotFound => Self::Custom(26),
            DarkbetError::LpShareNotFound => Self::Custom(27),
            DarkbetError::PositionAlreadyClaimed => Self::Custom(28),
            DarkbetError::ArithmeticOverflow => Self::Custom(29),
            DarkbetError::CrossContractFailed => Self::Custom(18),
            DarkbetError::InvalidOracleSignature => Self::Custom(30),
            DarkbetError::MarketAlreadyExists => Self::Custom(31),
            DarkbetError::UnauthorizedCrossContract => Self::Custom(32),
            DarkbetError::InvalidChildrenIndexes => Self::Custom(34),
            DarkbetError::InvalidChildCall => Self::Custom(35),
            DarkbetError::DuplicateNullifier => Self::Custom(36),
            DarkbetError::DatabaseError(_) => Self::Custom(19),
            DarkbetError::InternalError(_) => Self::Custom(20),
        }
    }
}