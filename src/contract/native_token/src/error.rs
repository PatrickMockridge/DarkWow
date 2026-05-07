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

//! NativeToken Error types

use darkfi_sdk::error::ContractError;
use thiserror::Error;

/// NativeToken-specific errors
#[derive(Debug, Error)]
pub enum NativeTokenError {
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

    #[error("Roots value data mismatch")]
    RootsValueDataMismatch,

    #[error("Merkle root not found in previous state")]
    TransferMerkleRootNotFound,

    #[error("Duplicate coin found")]
    DuplicateCoin,

    #[error("Missing inputs in transfer")]
    TransferMissingInputs,

    #[error("Burn call must have at least one input")]
    BurnMissingInputs,

    #[error("Missing outputs in transfer")]
    TransferMissingOutputs,

    #[error("Token mismatch")]
    TokenMismatch,

    #[error("Duplicate nullifier (double-spend)")]
    DuplicateNullifier,

    #[error("Invalid function (deprecated or removed)")]
    InvalidFunction,

    #[error("Value mismatch")]
    ValueMismatch,
}

impl From<NativeTokenError> for ContractError {
    fn from(e: NativeTokenError) -> Self {
        ContractError::Custom(e as u32)
    }
}