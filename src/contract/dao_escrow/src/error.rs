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

/// DAO-Escrow contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum DaoEscrowError {
    #[error("Contract not initialized")]
    NotInitialized,

    #[error("DAO-Escrow not found: {0}")]
    DaoEscrowNotFound(String),

    #[error("DAO-Escrow already exists: {0}")]
    DaoEscrowAlreadyExists(String),

    #[error("Invalid state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    #[error("Claim not found: {0}")]
    ClaimNotFound(String),

    #[error("Claim already exists: {0}")]
    ClaimAlreadyExists(String),

    #[error("Claim not pending")]
    ClaimNotPending,

    #[error("Claim already approved")]
    ClaimAlreadyApproved,

    #[error("Claim already rejected")]
    ClaimAlreadyRejected,

    #[error("Claim already executed")]
    ClaimAlreadyExecuted,

    #[error("Claim already cancelled")]
    ClaimAlreadyCancelled,

    #[error("Insufficient endowment balance")]
    InsufficientEndowment,

    #[error("Insufficient premium balance")]
    InsufficientPremium,

    #[error("Premium payment failed")]
    PremiumPaymentFailed,

    #[error("Invalid commitment")]
    InvalidCommitment,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Double-spend attempt")]
    DoubleSpend,

    #[error("Invalid ZK proof")]
    InvalidZkProof,

    #[error("Unauthorized: not claim proposer")]
    NotClaimProposer,

    #[error("Unauthorized: not DAO-Escrow owner")]
    NotOwner,

    #[error("Unauthorized: not authorized to withdraw")]
    NotAuthorizedToWithdraw,

    #[error("Vote not authorized")]
    VoteNotAuthorized,

    #[error("Already voted on this claim")]
    AlreadyVoted,

    #[error("Claim proposal expired")]
    ClaimExpired,

    #[error("Claim execution deadline passed")]
    ClaimExecutionDeadlinePassed,

    #[error("Invalid approval ratio")]
    InvalidApprovalRatio,

    #[error("Invalid quorum")]
    InvalidQuorum,

    #[error("Invalid premium rate")]
    InvalidPremiumRate,

    #[error("Endowment withdrawal not authorized")]
    EndowmentWithdrawUnauthorized,

    #[error("Maximum claim amount exceeded")]
    MaxClaimAmountExceeded,

    #[error("Minimum stake not met")]
    MinimumStakeNotMet,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid children indexes: expected money_v3::transfer_v1 call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call: expected money_v3::transfer_v1")]
    InvalidChildCall,
}

impl From<DaoEscrowError> for ContractError {
    fn from(e: DaoEscrowError) -> Self {
        match e {
            DaoEscrowError::NotInitialized => Self::Custom(1),
            DaoEscrowError::DaoEscrowNotFound(_) => Self::Custom(2),
            DaoEscrowError::DaoEscrowAlreadyExists(_) => Self::Custom(3),
            DaoEscrowError::InvalidState { .. } => Self::Custom(4),
            DaoEscrowError::ClaimNotFound(_) => Self::Custom(5),
            DaoEscrowError::ClaimAlreadyExists(_) => Self::Custom(6),
            DaoEscrowError::ClaimNotPending => Self::Custom(7),
            DaoEscrowError::ClaimAlreadyApproved => Self::Custom(8),
            DaoEscrowError::ClaimAlreadyRejected => Self::Custom(9),
            DaoEscrowError::ClaimAlreadyExecuted => Self::Custom(10),
            DaoEscrowError::ClaimAlreadyCancelled => Self::Custom(11),
            DaoEscrowError::InsufficientEndowment => Self::Custom(12),
            DaoEscrowError::InsufficientPremium => Self::Custom(13),
            DaoEscrowError::PremiumPaymentFailed => Self::Custom(14),
            DaoEscrowError::InvalidCommitment => Self::Custom(15),
            DaoEscrowError::InvalidNullifier => Self::Custom(16),
            DaoEscrowError::DoubleSpend => Self::Custom(17),
            DaoEscrowError::InvalidZkProof => Self::Custom(18),
            DaoEscrowError::NotClaimProposer => Self::Custom(19),
            DaoEscrowError::NotOwner => Self::Custom(20),
            DaoEscrowError::NotAuthorizedToWithdraw => Self::Custom(21),
            DaoEscrowError::VoteNotAuthorized => Self::Custom(22),
            DaoEscrowError::AlreadyVoted => Self::Custom(23),
            DaoEscrowError::ClaimExpired => Self::Custom(24),
            DaoEscrowError::ClaimExecutionDeadlinePassed => Self::Custom(25),
            DaoEscrowError::InvalidApprovalRatio => Self::Custom(26),
            DaoEscrowError::InvalidQuorum => Self::Custom(27),
            DaoEscrowError::InvalidPremiumRate => Self::Custom(28),
            DaoEscrowError::EndowmentWithdrawUnauthorized => Self::Custom(29),
            DaoEscrowError::MaxClaimAmountExceeded => Self::Custom(30),
            DaoEscrowError::MinimumStakeNotMet => Self::Custom(31),
            DaoEscrowError::InvalidSignature => Self::Custom(32),
            DaoEscrowError::InvalidChildrenIndexes => Self::Custom(33),
            DaoEscrowError::InvalidChildCall => Self::Custom(34),
        }
    }
}
