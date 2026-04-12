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

//! SimpleCoin Error types

use darkfi_sdk::error::ContractError;
use thiserror::Error;

/// SimpleCoin-specific errors
#[derive(Debug, Error)]
pub enum SimplecoinError {
    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Coin not found")]
    CoinNotFound,

    #[error("Coin already spent (double-spend)")]
    CoinAlreadySpent,

    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Value overflow")]
    ValueOverflow,

    #[error("Invalid coin value")]
    InvalidCoinValue,

    #[error("Too many coins in transaction")]
    TooManyCoins,

    #[error("Recipient is zero")]
    InvalidRecipient,

    #[error("Token ID mismatch")]
    TokenIdMismatch,

    #[error("Genesis already exists")]
    GenesisAlreadyExists,

    #[error("No coins to melt")]
    NoCoinsToMelt,
}

impl From<SimplecoinError> for ContractError {
    fn from(e: SimplecoinError) -> Self {
        ContractError::Custom(e as u32)
    }
}