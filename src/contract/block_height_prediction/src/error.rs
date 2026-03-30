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

//! Block Height Prediction Contract Errors

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum BlockHeightPredictionError {
    #[error("Market already exists")]
    MarketAlreadyExists,

    #[error("Position already exists")]
    PositionAlreadyExists,

    #[error("Invalid confirmation depth (must be 1-10)")]
    InvalidConfirmationDepth,

    #[error("Invalid tolerance range")]
    InvalidTolerance,

    #[error("Bet value too small")]
    BetValueTooSmall,

    #[error("Market not active")]
    MarketNotActive,

    #[error("Market already resolved")]
    MarketAlreadyResolved,

    #[error("Invalid state transition")]
    InvalidStateTransition,

    #[error("Target time not reached")]
    TargetTimeNotReached,

    #[error("Invalid position type")]
    InvalidPositionType,

    #[error("Unauthorized access")]
    Unauthorized,

    #[error("Position not found")]
    PositionNotFound,

    #[error("Market not found")]
    MarketNotFound,

    #[error("Winnings already claimed")]
    WinningsClaimed,

    #[error("Resolution blocked by fork race")]
    ResolutionBlocked,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Invalid proof")]
    InvalidProof,

    #[error("Invalid block height")]
    InvalidBlockHeight,

    #[error("Betting closed")]
    BettingClosed,
}

impl From<BlockHeightPredictionError> for ContractError {
    fn from(e: BlockHeightPredictionError) -> Self {
        match e {
            BlockHeightPredictionError::MarketAlreadyExists => Self::Custom(1),
            BlockHeightPredictionError::PositionAlreadyExists => Self::Custom(2),
            BlockHeightPredictionError::InvalidConfirmationDepth => Self::Custom(3),
            BlockHeightPredictionError::InvalidTolerance => Self::Custom(4),
            BlockHeightPredictionError::BetValueTooSmall => Self::Custom(5),
            BlockHeightPredictionError::MarketNotActive => Self::Custom(6),
            BlockHeightPredictionError::MarketAlreadyResolved => Self::Custom(7),
            BlockHeightPredictionError::InvalidStateTransition => Self::Custom(8),
            BlockHeightPredictionError::TargetTimeNotReached => Self::Custom(9),
            BlockHeightPredictionError::InvalidPositionType => Self::Custom(10),
            BlockHeightPredictionError::Unauthorized => Self::Custom(11),
            BlockHeightPredictionError::PositionNotFound => Self::Custom(12),
            BlockHeightPredictionError::MarketNotFound => Self::Custom(13),
            BlockHeightPredictionError::WinningsClaimed => Self::Custom(14),
            BlockHeightPredictionError::ResolutionBlocked => Self::Custom(15),
            BlockHeightPredictionError::ArithmeticOverflow => Self::Custom(16),
            BlockHeightPredictionError::InvalidProof => Self::Custom(17),
            BlockHeightPredictionError::InvalidBlockHeight => Self::Custom(18),
            BlockHeightPredictionError::BettingClosed => Self::Custom(19),
        }
    }
}
