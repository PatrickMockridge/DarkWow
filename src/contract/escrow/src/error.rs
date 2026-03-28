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

/// Escrow contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum EscrowError {
    #[error("Contract not initialized")]
    NotInitialized,

    #[error("Escrow not found: {0}")]
    EscrowNotFound(String),

    #[error("Escrow already exists: {0}")]
    EscrowAlreadyExists(String),

    #[error("Invalid escrow state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("Invalid escrow state transition")]
    InvalidStateTransition,

    #[error("Invalid commitment")]
    InvalidCommitment,

    #[error("Commitment mismatch")]
    CommitmentMismatch,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Already spent (nullifier exists)")]
    AlreadySpent,

    #[error("Commitment not found in tree")]
    CommitmentNotFound,

    #[error("Double-spend attempt: nullifier already spent")]
    DoubleSpend,

    #[error("Invalid ZK proof")]
    InvalidZkProof,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Timeout not yet reached: need {needed} more blocks")]
    TimeoutNotReached { needed: u64 },

    #[error("Timelock not expired")]
    TimelockNotExpired,

    #[error("Seller secret does not match")]
    SellerSecretMismatch,

    #[error("Buyer secret does not match")]
    BuyerSecretMismatch,

    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Insufficient funds in escrow")]
    InsufficientFunds,

    #[error("Unauthorized: only buyer can request refund")]
    OnlyBuyerCanRefund,

    #[error("Unauthorized: only seller can claim")]
    OnlySellerCanClaim,

    #[error("Escrow already claimed")]
    EscrowAlreadyClaimed,

    #[error("Escrow already refunded")]
    EscrowAlreadyRefunded,

    #[error("Escrow already cancelled")]
    EscrowAlreadyCancelled,

    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    #[error("Invalid timeout: must be in the future")]
    InvalidTimeout,

    #[error("Cannot cancel: escrow is funded")]
    CannotCancelFunded,

    #[error("Cannot cancel: only buyer can cancel before funding")]
    CannotCancelNonBuyer,
}

impl From<EscrowError> for ContractError {
    fn from(e: EscrowError) -> Self {
        match e {
            EscrowError::NotInitialized => Self::Custom(1),
            EscrowError::EscrowNotFound(_) => Self::Custom(2),
            EscrowError::EscrowAlreadyExists(_) => Self::Custom(3),
            EscrowError::InvalidState { .. } => Self::Custom(4),
            EscrowError::InvalidStateTransition => Self::Custom(5),
            EscrowError::InvalidCommitment => Self::Custom(6),
            EscrowError::CommitmentMismatch => Self::Custom(7),
            EscrowError::InvalidNullifier => Self::Custom(8),
            EscrowError::AlreadySpent => Self::Custom(9),
            EscrowError::CommitmentNotFound => Self::Custom(10),
            EscrowError::DoubleSpend => Self::Custom(11),
            EscrowError::InvalidZkProof => Self::Custom(12),
            EscrowError::InvalidSignature => Self::Custom(13),
            EscrowError::TimeoutNotReached { .. } => Self::Custom(14),
            EscrowError::TimelockNotExpired => Self::Custom(15),
            EscrowError::SellerSecretMismatch => Self::Custom(16),
            EscrowError::BuyerSecretMismatch => Self::Custom(17),
            EscrowError::InvalidMerkleProof => Self::Custom(18),
            EscrowError::InsufficientFunds => Self::Custom(19),
            EscrowError::OnlyBuyerCanRefund => Self::Custom(20),
            EscrowError::OnlySellerCanClaim => Self::Custom(21),
            EscrowError::EscrowAlreadyClaimed => Self::Custom(22),
            EscrowError::EscrowAlreadyRefunded => Self::Custom(23),
            EscrowError::EscrowAlreadyCancelled => Self::Custom(24),
            EscrowError::InvalidAmount(_) => Self::Custom(25),
            EscrowError::InvalidTimeout => Self::Custom(26),
            EscrowError::CannotCancelFunded => Self::Custom(27),
            EscrowError::CannotCancelNonBuyer => Self::Custom(28),
        }
    }
}
