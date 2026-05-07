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

//! Oracle Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

/// Contract error types
#[derive(Debug, Error)]
pub enum OracleError {
    #[error("Oracle not found")]
    OracleNotFound,

    #[error("Oracle not active")]
    OracleNotActive,

    #[error("Not authorized: must be oracle operator")]
    NotAuthorized,

    #[error("Oracle already exists")]
    OracleAlreadyExists,

    #[error("Invalid predicate type")]
    InvalidPredicate,

    #[error("Value update too soon")]
    UpdateTooSoon,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("ZK proof verification failed")]
    ZkProofVerificationFailed,
}

impl From<OracleError> for ContractError {
    fn from(e: OracleError) -> Self {
        match e {
            OracleError::OracleNotFound => Self::Custom(1),
            OracleError::OracleNotActive => Self::Custom(2),
            OracleError::NotAuthorized => Self::Custom(3),
            OracleError::OracleAlreadyExists => Self::Custom(4),
            OracleError::InvalidPredicate => Self::Custom(5),
            OracleError::UpdateTooSoon => Self::Custom(6),
            OracleError::InvalidSignature => Self::Custom(7),
            OracleError::ZkProofVerificationFailed => Self::Custom(8),
        }
    }
}
