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

//! Money V3 Error types

pub use dwow_sdk::error::ContractError;
use thiserror::Error;

/// MoneyV3-specific errors
#[derive(Debug, Error)]
pub enum MoneyV3Error {
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

    #[error("Duplicate nullifier (double-spend)")]
    DuplicateNullifier,

    #[error("Invalid function (deprecated or removed)")]
    InvalidFunction,

    #[error("Value mismatch")]
    ValueMismatch,

    // Money V3 specific errors
    #[error("Invalid Schnorr signature")]
    InvalidSchnorrSignature,

    #[error("Public key does not match secret")]
    PublicKeyMismatch,

    #[error("Token ID commitment mismatch")]
    TokenCommitmentMismatch,

    #[error("Token not registered in token registry")]
    TokenNotRegistered,

    #[error("Token registry root not found")]
    TokenRegistryRootNotFound,

    #[error("Invalid child contract ID")]
    InvalidChildContractId,
}

impl From<MoneyV3Error> for ContractError {
    fn from(e: MoneyV3Error) -> Self {
        ContractError::Custom(e as u32)
    }
}