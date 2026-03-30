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

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum PredictionMarketError {
    #[error("Market does not exist")]
    MarketNotFound,

    #[error("Position does not exist")]
    PositionNotFound,

    #[error("Invalid market state transition")]
    InvalidMarketState,

    #[error("Market is not active")]
    MarketNotActive,

    #[error("Market already resolved")]
    MarketAlreadyResolved,

    #[error("Invalid outcome index")]
    InvalidOutcome,

    #[error("Bet value too small")]
    BetValueTooSmall,

    #[error("Insufficient liquidity")]
    InsufficientLiquidity,

    #[error("Liquidity provider share not found")]
    LpShareNotFound,

    #[error("Oracle attestation verification failed")]
    InvalidOracleAttestation,

    #[error("Oracle not authorized")]
    UnauthorizedOracle,

    #[error("Resolution timeout not reached")]
    ResolutionTimeoutNotReached,

    #[error("Already claimed winnings")]
    AlreadyClaimed,

    #[error("No winnings to claim")]
    NoWinnings,

    #[error("Invalid fee percentage")]
    InvalidFee,

    #[error("Protocol fee too high")]
    ProtocolFeeTooHigh,

    #[error("Market question too long")]
    QuestionTooLong,

    #[error("Duplicate market")]
    MarketAlreadyExists,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Zero-knowledge proof verification failed")]
    InvalidProof,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Cross-contract call failed")]
    CrossContractFailed,

    #[error("Value commitment mismatch")]
    ValueCommitmentMismatch,

    #[error("Position already exists")]
    PositionAlreadyExists,

    #[error("Market is frozen")]
    MarketFrozen,

    #[error("Market not resolved")]
    MarketNotResolved,

    #[error("Oracle timeout exceeded")]
    OracleTimeoutExceeded,

    #[error("Insufficient contract balance")]
    InsufficientBalance,

    #[error("Claim does not exist")]
    ClaimNotFound,

    #[error("Arithmetic overflow in calculation")]
    ArithmeticOverflow,
}

impl From<PredictionMarketError> for ContractError {
    fn from(e: PredictionMarketError) -> Self {
        match e {
            PredictionMarketError::MarketNotFound => Self::Custom(1),
            PredictionMarketError::PositionNotFound => Self::Custom(2),
            PredictionMarketError::InvalidMarketState => Self::Custom(3),
            PredictionMarketError::MarketNotActive => Self::Custom(4),
            PredictionMarketError::MarketAlreadyResolved => Self::Custom(5),
            PredictionMarketError::InvalidOutcome => Self::Custom(6),
            PredictionMarketError::BetValueTooSmall => Self::Custom(7),
            PredictionMarketError::InsufficientLiquidity => Self::Custom(8),
            PredictionMarketError::LpShareNotFound => Self::Custom(9),
            PredictionMarketError::InvalidOracleAttestation => Self::Custom(10),
            PredictionMarketError::UnauthorizedOracle => Self::Custom(11),
            PredictionMarketError::ResolutionTimeoutNotReached => Self::Custom(12),
            PredictionMarketError::AlreadyClaimed => Self::Custom(13),
            PredictionMarketError::NoWinnings => Self::Custom(14),
            PredictionMarketError::InvalidFee => Self::Custom(15),
            PredictionMarketError::ProtocolFeeTooHigh => Self::Custom(16),
            PredictionMarketError::QuestionTooLong => Self::Custom(17),
            PredictionMarketError::MarketAlreadyExists => Self::Custom(18),
            PredictionMarketError::InvalidSignature => Self::Custom(19),
            PredictionMarketError::InvalidProof => Self::Custom(20),
            PredictionMarketError::UnauthorizedCaller => Self::Custom(21),
            PredictionMarketError::CrossContractFailed => Self::Custom(22),
            PredictionMarketError::ValueCommitmentMismatch => Self::Custom(23),
            PredictionMarketError::PositionAlreadyExists => Self::Custom(24),
            PredictionMarketError::MarketFrozen => Self::Custom(25),
            PredictionMarketError::MarketNotResolved => Self::Custom(26),
            PredictionMarketError::OracleTimeoutExceeded => Self::Custom(27),
            PredictionMarketError::InsufficientBalance => Self::Custom(28),
            PredictionMarketError::ClaimNotFound => Self::Custom(29),
            PredictionMarketError::ArithmeticOverflow => Self::Custom(30),
        }
    }
}
