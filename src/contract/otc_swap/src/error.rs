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

use dwow_sdk::error::ContractError;

/// OTC Swap contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum OtcSwapError {
    #[error("Contract not initialized")]
    NotInitialized,

    #[error("Swap not found: {0}")]
    SwapNotFound(String),

    #[error("Swap already exists: {0}")]
    SwapAlreadyExists(String),

    #[error("Invalid swap state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("Invalid swap state transition")]
    InvalidStateTransition,

    #[error("Invalid commitment")]
    InvalidCommitment,

    #[error("Commitment mismatch")]
    CommitmentMismatch,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Already spent (nullifier exists)")]
    AlreadySpent,

    #[error("Double-spend attempt: nullifier already spent")]
    DoubleSpend,

    #[error("Invalid ZK proof")]
    InvalidZkProof,

    #[error("Timeout not yet reached: need {needed} more blocks")]
    TimeoutNotReached { needed: u64 },

    #[error("Timelock not expired")]
    TimelockNotExpired,

    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Insufficient funds in swap")]
    InsufficientFunds,

    #[error("Unauthorized: only Alice can perform this action")]
    OnlyAliceCanPerform,

    #[error("Unauthorized: only Bob can perform this action")]
    OnlyBobCanPerform,

    #[error("Swap already executed")]
    SwapAlreadyExecuted,

    #[error("Swap already cancelled")]
    SwapAlreadyCancelled,

    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    #[error("Invalid timeout: must be in the future")]
    InvalidTimeout,

    #[error("Cannot cancel: swap is funded and timeout not reached")]
    CannotCancelFundedNoTimeout,

    #[error("Cannot execute: swap is not funded")]
    CannotExecuteNotFunded,

    #[error("Invalid children_indexes: expected 1 child call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call: expected promissory_note::transfer_v1")]
    InvalidChildCall,

    #[error("Token mismatch: swap tokens don't match")]
    TokenMismatch,
}

impl From<OtcSwapError> for ContractError {
    fn from(e: OtcSwapError) -> Self {
        match e {
            OtcSwapError::NotInitialized => Self::Custom(1),
            OtcSwapError::SwapNotFound(_) => Self::Custom(2),
            OtcSwapError::SwapAlreadyExists(_) => Self::Custom(3),
            OtcSwapError::InvalidState { .. } => Self::Custom(4),
            OtcSwapError::InvalidStateTransition => Self::Custom(5),
            OtcSwapError::InvalidCommitment => Self::Custom(6),
            OtcSwapError::CommitmentMismatch => Self::Custom(7),
            OtcSwapError::InvalidNullifier => Self::Custom(8),
            OtcSwapError::AlreadySpent => Self::Custom(9),
            OtcSwapError::DoubleSpend => Self::Custom(10),
            OtcSwapError::InvalidZkProof => Self::Custom(11),
            OtcSwapError::TimeoutNotReached { .. } => Self::Custom(12),
            OtcSwapError::TimelockNotExpired => Self::Custom(13),
            OtcSwapError::InvalidMerkleProof => Self::Custom(14),
            OtcSwapError::InsufficientFunds => Self::Custom(15),
            OtcSwapError::OnlyAliceCanPerform => Self::Custom(16),
            OtcSwapError::OnlyBobCanPerform => Self::Custom(17),
            OtcSwapError::SwapAlreadyExecuted => Self::Custom(18),
            OtcSwapError::SwapAlreadyCancelled => Self::Custom(19),
            OtcSwapError::InvalidAmount(_) => Self::Custom(20),
            OtcSwapError::InvalidTimeout => Self::Custom(21),
            OtcSwapError::CannotCancelFundedNoTimeout => Self::Custom(22),
            OtcSwapError::CannotExecuteNotFunded => Self::Custom(23),
            OtcSwapError::InvalidChildrenIndexes => Self::Custom(24),
            OtcSwapError::InvalidChildCall => Self::Custom(25),
            OtcSwapError::TokenMismatch => Self::Custom(26),
        }
    }
}
