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

use darkfi_sdk::error::ContractError;

/// DEX contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum DexError {
    #[error("Swap not found")]
    SwapNotFound,

    #[error("Swap already exists")]
    SwapAlreadyExists,

    #[error("Invalid swap state")]
    InvalidSwapState,

    #[error("Swap expired")]
    SwapExpired,

    #[error("Unauthorized cancellation")]
    UnauthorizedCancellation,

    #[error("Unauthorized execution")]
    UnauthorizedExecution,

    #[error("Invalid lock commitment")]
    InvalidLockCommitment,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid ZK proof")]
    InvalidZkProof,

    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Amount mismatch")]
    AmountMismatch,

    #[error("Token mismatch")]
    TokenMismatch,

    #[error("Swap timeout")]
    SwapTimeout,

    #[error("DEX not initialized")]
    DexNotInitialized,

    #[error("Invalid configuration")]
    InvalidConfiguration,

    #[error("Fee too low")]
    FeeTooLow,

    #[error("Invalid children indexes: expected money_v3::otc_swap_v1 calls")]
    InvalidChildrenIndexes,

    #[error("Governance not set")]
    GovernanceNotSet,

    #[error("Invalid governance key")]
    InvalidGovernanceKey,

    #[error("Not authorized")]
    NotAuthorized,

    #[error("Invalid child call")]
    InvalidChildCall,
}

impl From<DexError> for ContractError {
    fn from(e: DexError) -> Self {
        match e {
            DexError::SwapNotFound => Self::Custom(1),
            DexError::SwapAlreadyExists => Self::Custom(2),
            DexError::InvalidSwapState => Self::Custom(3),
            DexError::SwapExpired => Self::Custom(4),
            DexError::UnauthorizedCancellation => Self::Custom(5),
            DexError::UnauthorizedExecution => Self::Custom(6),
            DexError::InvalidLockCommitment => Self::Custom(7),
            DexError::InvalidNullifier => Self::Custom(8),
            DexError::InvalidSignature => Self::Custom(9),
            DexError::InvalidZkProof => Self::Custom(10),
            DexError::InvalidMerkleProof => Self::Custom(11),
            DexError::InsufficientBalance => Self::Custom(12),
            DexError::AmountMismatch => Self::Custom(13),
            DexError::TokenMismatch => Self::Custom(14),
            DexError::SwapTimeout => Self::Custom(15),
            DexError::DexNotInitialized => Self::Custom(16),
            DexError::InvalidConfiguration => Self::Custom(17),
            DexError::FeeTooLow => Self::Custom(18),
            DexError::InvalidChildrenIndexes => Self::Custom(19),
            DexError::GovernanceNotSet => Self::Custom(20),
            DexError::InvalidGovernanceKey => Self::Custom(21),
            DexError::NotAuthorized => Self::Custom(22),
            DexError::InvalidChildCall => Self::Custom(23),
        }
    }
}