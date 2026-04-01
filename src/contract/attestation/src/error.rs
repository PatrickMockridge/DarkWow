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

//! Attestation contract errors

use thiserror::Error;

/// Attestation contract errors
#[derive(Error, Debug)]
pub enum AttestationError {
    #[error("Attestation not found")]
    AttestationNotFound,

    #[error("Claim not found")]
    ClaimNotFound,

    #[error("Invalid attestation state: expected {expected:?}, got {actual:?}")]
    InvalidAttestationState { expected: String, actual: String },

    #[error("Invalid claim state: expected {expected:?}, got {actual:?}")]
    InvalidClaimState { expected: String, actual: String },

    #[error("Attestation already exists")]
    AttestationAlreadyExists,

    #[error("Claim already exists")]
    ClaimAlreadyExists,

    #[error("Attestation expired")]
    AttestationExpired,

    #[error("Attestation revoked")]
    AttestationRevoked,

    #[error("Attestation not active")]
    AttestationNotActive,

    #[error("Claim not pending")]
    ClaimNotPending,

    #[error("Claim not verified")]
    ClaimNotVerified,

    #[error("Claim already consumed")]
    ClaimAlreadyConsumed,

    #[error("Only attestor can perform this action")]
    NotAttestor,

    #[error("Only claimant can perform this action")]
    NotClaimant,

    #[error("Predicate mismatch")]
    PredicateMismatch,

    #[error("Evidence does not match attestation")]
    EvidenceMismatch,

    #[error("Value out of range")]
    ValueOutOfRange,

    #[error("Nullifier already spent")]
    NullifierSpent,

    #[error("Invalid zk proof")]
    InvalidProof,

    #[error("Predicate not allowed for this attestation type")]
    PredicateNotAllowed,

    #[error("Claim rate limit exceeded")]
    ClaimRateLimitExceeded,

    #[error("Invalid predicate ID")]
    InvalidPredicateId,

    #[error("Sled database error: {0}")]
    SledError(String),
}