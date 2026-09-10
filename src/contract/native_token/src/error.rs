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

use dwow_sdk::error::ContractError;
use thiserror::Error;

/// NativeToken-specific errors
#[derive(Debug, Error)]
pub enum NativeTokenError {
    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Parse error — invalid fee parameters")]
    ParseError = 2,

    #[error("Capability not found")]
    CommitmentNotFound = 3,

    // UNVERIFIED(HYG-10-4): needs cargo check -p dwow-native-token-contract
    // --features client -j 2 && cargo test -p dwow-native-token-contract --test-threads=2
    // 8 never-constructed variants removed (zero construction/match sites
    // repo-wide): CommitmentAlreadySpent, ValueOverflow, InvalidValue,
    // TooManyCommitments, InvalidRecipient, GenesisAlreadyExists,
    // NoCommitmentsToMelt, BurnMissingInputs — leftovers from pre-plaintext
    // fee/burn logic. NOTE: From<NativeTokenError> for ContractError casts
    // `e as u32`, so every variant from InvalidMerkleProof onward SHIFTS
    // its on-chain error code (e.g. InvalidMerkleProof 5 -> 4). Verify no
    // host/logic code depends on the old discriminants.
    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Token ID mismatch")]
    AssetIdMismatch,

    #[error("Roots value data mismatch")]
    RootsValueDataMismatch,

    #[error("Merkle root not found in previous state")]
    TransferMerkleRootNotFound,

    #[error("Duplicate capability found")]
    DuplicateCommitment,

    #[error("Missing inputs in transfer")]
    TransferMissingInputs,

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

    #[error("Fee total mismatch — does not match the fees_db[height] plaintext sum")]
    FeeTotalMismatch,

    #[error("Zero-fee claim rejected")]
    ZeroFeeClaim,
}

impl From<NativeTokenError> for ContractError {
    /// H-6: This conversion discards variant information — every variant
    /// collapses to a bare `u32` discriminant. Enum reordering silently
    /// breaks caller interpretation. The log-substitution mitigation
    /// (msg!() before error return) preserves diagnostic context in the
    /// host log but not in the error type itself.
    ///
    /// TODO(H-6): Add `ParseError(&'static str)` variant carrying the
    /// field name. Audit all match sites for reordering impact.
    fn from(e: NativeTokenError) -> Self {
        ContractError::Custom(e as u32)
    }
}