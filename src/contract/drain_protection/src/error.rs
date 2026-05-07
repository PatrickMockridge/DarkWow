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

/// DrainProtection contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum DrainProtectionError {
    #[error("Contract not initialized")]
    NotInitialized,

    #[error("Rate limit exceeded: {current} per block, max allowed is {max}")]
    RateLimitExceeded { current: u64, max: u64 },

    #[error("Insufficient vote threshold: need {required}% got {actual}%")]
    InsufficientVoteThreshold { required: u64, actual: u64 },

    #[error("Quorum not reached: need {required}% participation got {actual}%")]
    QuorumNotReached { required: u64, actual: u64 },

    #[error("Funds are locked")]
    FundsLocked,

    #[error("Funds unlock timelock not expired: need {needed} more blocks")]
    UnlockTimelockNotExpired { needed: u64 },

    #[error("Emergency lock expired")]
    LockExpired,

    #[error("Lock renewal requires 2/3 vote")]
    LockRenewalRequiresVote,

    #[error("Invalid spend authority")]
    InvalidSpendAuthority,

    #[error("Spend authority change timelock not expired")]
    AuthorityChangeTimelock,

    #[error("Member not found")]
    MemberNotFound,

    #[error("Member already exists")]
    MemberAlreadyExists,

    #[error("Exit haircut applies: {0}% withheld")]
    ExitHaircutApplies(u64),

    #[error("Contribution weight is zero")]
    ZeroContributionWeight,

    #[error("Invalid withdrawal amount")]
    InvalidWithdrawalAmount,

    #[error("Withdrawal exceeds rate limit")]
    WithdrawalExceedsRateLimit,

    #[error("DAO-Escrow bulla mismatch")]
    BullaMismatch,

    #[error("Invalid ZK proof")]
    InvalidZkProof,

    #[error("Unauthorized: not a DAO member")]
    Unauthorized,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Invalid children indexes for child call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call")]
    InvalidChildCall,
}

impl From<DrainProtectionError> for ContractError {
    fn from(e: DrainProtectionError) -> Self {
        match e {
            DrainProtectionError::NotInitialized => Self::Custom(1),
            DrainProtectionError::RateLimitExceeded { .. } => Self::Custom(2),
            DrainProtectionError::InsufficientVoteThreshold { .. } => Self::Custom(3),
            DrainProtectionError::QuorumNotReached { .. } => Self::Custom(4),
            DrainProtectionError::FundsLocked => Self::Custom(5),
            DrainProtectionError::UnlockTimelockNotExpired { .. } => Self::Custom(6),
            DrainProtectionError::LockExpired => Self::Custom(7),
            DrainProtectionError::LockRenewalRequiresVote => Self::Custom(8),
            DrainProtectionError::InvalidSpendAuthority => Self::Custom(9),
            DrainProtectionError::AuthorityChangeTimelock => Self::Custom(10),
            DrainProtectionError::MemberNotFound => Self::Custom(11),
            DrainProtectionError::MemberAlreadyExists => Self::Custom(12),
            DrainProtectionError::ExitHaircutApplies(_) => Self::Custom(13),
            DrainProtectionError::ZeroContributionWeight => Self::Custom(14),
            DrainProtectionError::InvalidWithdrawalAmount => Self::Custom(15),
            DrainProtectionError::WithdrawalExceedsRateLimit => Self::Custom(16),
            DrainProtectionError::BullaMismatch => Self::Custom(17),
            DrainProtectionError::InvalidZkProof => Self::Custom(18),
            DrainProtectionError::Unauthorized => Self::Custom(19),
            DrainProtectionError::InvalidSignature => Self::Custom(20),
            DrainProtectionError::ConfigurationError(_) => Self::Custom(21),
            DrainProtectionError::InvalidChildrenIndexes => Self::Custom(22),
            DrainProtectionError::InvalidChildCall => Self::Custom(23),
        }
    }
}