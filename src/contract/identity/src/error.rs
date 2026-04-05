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

/// Identity contract errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum IdentityError {
    #[error("Credential not found")]
    CredentialNotFound,

    #[error("Credential already exists")]
    CredentialAlreadyExists,

    #[error("Credential expired")]
    CredentialExpired,

    #[error("Credential revoked")]
    CredentialRevoked,

    #[error("Invalid issuer")]
    InvalidIssuer,

    #[error("Issuer not trusted")]
    IssuerNotTrusted,

    #[error("Invalid schema")]
    InvalidSchema,

    #[error("Schema not recognized")]
    SchemaNotRecognized,

    #[error("Invalid claim")]
    InvalidClaim,

    #[error("Claim expired")]
    ClaimExpired,

    #[error("Claim already used")]
    ClaimAlreadyUsed,

    #[error("Invalid ZK proof")]
    InvalidProof,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid nullifier")]
    InvalidNullifier,

    #[error("Nullifier already spent")]
    NullifierAlreadySpent,

    #[error("Attribute mismatch")]
    AttributeMismatch,

    #[error("Predicate evaluation failed")]
    PredicateFailed,

    #[error("Insufficient fee")]
    InsufficientFee,

    #[error("Identity not initialized")]
    NotInitialized,

    // O-Cap errors
    #[error("Capability not found")]
    CapabilityNotFound,

    #[error("Capability already exists")]
    CapabilityAlreadyExists,

    #[error("Capability revoked")]
    CapabilityRevoked,

    #[error("Capability expired")]
    CapabilityExpired,

    #[error("Capability max holders reached")]
    CapabilityMaxHoldersReached,

    #[error("Capability requirement not met")]
    CapabilityRequirementNotMet,

    // DAG errors
    #[error("DAG not found")]
    DAGNotFound,

    #[error("Invalid DAG path")]
    InvalidDAGPath,

    #[error("DAG path not satisfied")]
    DAGPathNotSatisfied,
}

impl From<IdentityError> for ContractError {
    fn from(e: IdentityError) -> Self {
        match e {
            IdentityError::CredentialNotFound => Self::Custom(1),
            IdentityError::CredentialAlreadyExists => Self::Custom(2),
            IdentityError::CredentialExpired => Self::Custom(3),
            IdentityError::CredentialRevoked => Self::Custom(4),
            IdentityError::InvalidIssuer => Self::Custom(5),
            IdentityError::IssuerNotTrusted => Self::Custom(6),
            IdentityError::InvalidSchema => Self::Custom(7),
            IdentityError::SchemaNotRecognized => Self::Custom(8),
            IdentityError::InvalidClaim => Self::Custom(9),
            IdentityError::ClaimExpired => Self::Custom(10),
            IdentityError::ClaimAlreadyUsed => Self::Custom(11),
            IdentityError::InvalidProof => Self::Custom(12),
            IdentityError::InvalidSignature => Self::Custom(13),
            IdentityError::InvalidNullifier => Self::Custom(14),
            IdentityError::NullifierAlreadySpent => Self::Custom(15),
            IdentityError::AttributeMismatch => Self::Custom(16),
            IdentityError::PredicateFailed => Self::Custom(17),
            IdentityError::InsufficientFee => Self::Custom(18),
            IdentityError::NotInitialized => Self::Custom(19),
            IdentityError::CapabilityNotFound => Self::Custom(20),
            IdentityError::CapabilityAlreadyExists => Self::Custom(21),
            IdentityError::CapabilityRevoked => Self::Custom(22),
            IdentityError::CapabilityExpired => Self::Custom(23),
            IdentityError::CapabilityMaxHoldersReached => Self::Custom(24),
            IdentityError::CapabilityRequirementNotMet => Self::Custom(25),
            IdentityError::DAGNotFound => Self::Custom(26),
            IdentityError::InvalidDAGPath => Self::Custom(27),
            IdentityError::DAGPathNotSatisfied => Self::Custom(28),
        }
    }
}