/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Plain Attestation Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AttestationPlainError {
    #[error("Attestation not found")]
    AttestationNotFound,

    #[error("Attestation already exists")]
    AttestationAlreadyExists,

    #[error("Attestor not found")]
    AttestorNotFound,

    #[error("Attestor already registered")]
    AttestorAlreadyExists,

    #[error("Invalid attestation schema")]
    InvalidSchema,

    #[error("Invalid delegation depth")]
    InvalidDelegationDepth,

    #[error("Delegation ratio exceeded")]
    DelegationRatioExceeded,

    #[error("Credential expired")]
    CredentialExpired,

    #[error("Credential revoked")]
    CredentialRevoked,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Cross-contract call failed")]
    CrossContractFailed,
}

impl From<AttestationPlainError> for ContractError {
    fn from(e: AttestationPlainError) -> Self {
        match e {
            AttestationPlainError::AttestationNotFound => Self::Custom(1),
            AttestationPlainError::AttestationAlreadyExists => Self::Custom(2),
            AttestationPlainError::AttestorNotFound => Self::Custom(3),
            AttestationPlainError::AttestorAlreadyExists => Self::Custom(4),
            AttestationPlainError::InvalidSchema => Self::Custom(5),
            AttestationPlainError::InvalidDelegationDepth => Self::Custom(6),
            AttestationPlainError::DelegationRatioExceeded => Self::Custom(7),
            AttestationPlainError::CredentialExpired => Self::Custom(8),
            AttestationPlainError::CredentialRevoked => Self::Custom(9),
            AttestationPlainError::UnauthorizedCaller => Self::Custom(10),
            AttestationPlainError::InvalidSignature => Self::Custom(11),
            AttestationPlainError::ArithmeticOverflow => Self::Custom(12),
            AttestationPlainError::DivisionByZero => Self::Custom(13),
            AttestationPlainError::InvalidFunction => Self::Custom(14),
            AttestationPlainError::CrossContractFailed => Self::Custom(15),
        }
    }
}