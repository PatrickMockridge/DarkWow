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

/// DEX contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum DexError {
    #[error("Order not found")]
    OrderNotFound,

    #[error("Order already exists")]
    OrderAlreadyExists,

    #[error("Order already spent")]
    OrderAlreadySpent,

    #[error("Invalid order commitment")]
    InvalidOrderCommitment,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Insufficient liquidity")]
    InsufficientLiquidity,

    #[error("Price mismatch")]
    PriceMismatch,

    #[error("Amount mismatch")]
    AmountMismatch,

    #[error("Invalid merkle proof")]
    InvalidMerkleProof,

    #[error("Invalid ZK proof")]
    InvalidZkProof,

    #[error("DEX not initialized")]
    DexNotInitialized,

    #[error("Invalid configuration")]
    InvalidConfiguration,

    #[error("Unauthorized cancellation")]
    UnauthorizedCancellation,

    #[error("Unauthorized match")]
    UnauthorizedMatch,

    #[error("Order too small")]
    OrderTooSmall,

    #[error("Fee too low")]
    FeeTooLow,

    #[error("Slippage exceeded")]
    SlippageExceeded,

    #[error("Pool not found")]
    PoolNotFound,

    #[error("Invalid LP share")]
    InvalidLpShare,

    #[error("Zero liquidity")]
    ZeroLiquidity,
}

impl From<DexError> for ContractError {
    fn from(e: DexError) -> Self {
        match e {
            DexError::OrderNotFound => Self::Custom(1),
            DexError::OrderAlreadyExists => Self::Custom(2),
            DexError::OrderAlreadySpent => Self::Custom(3),
            DexError::InvalidOrderCommitment => Self::Custom(4),
            DexError::InvalidNullifier => Self::Custom(5),
            DexError::InsufficientBalance => Self::Custom(6),
            DexError::InsufficientLiquidity => Self::Custom(7),
            DexError::PriceMismatch => Self::Custom(8),
            DexError::AmountMismatch => Self::Custom(9),
            DexError::InvalidMerkleProof => Self::Custom(10),
            DexError::InvalidZkProof => Self::Custom(11),
            DexError::DexNotInitialized => Self::Custom(12),
            DexError::InvalidConfiguration => Self::Custom(13),
            DexError::UnauthorizedCancellation => Self::Custom(14),
            DexError::UnauthorizedMatch => Self::Custom(15),
            DexError::OrderTooSmall => Self::Custom(16),
            DexError::FeeTooLow => Self::Custom(17),
            DexError::SlippageExceeded => Self::Custom(18),
            DexError::PoolNotFound => Self::Custom(19),
            DexError::InvalidLpShare => Self::Custom(20),
            DexError::ZeroLiquidity => Self::Custom(21),
        }
    }
}