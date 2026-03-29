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

//! Oracle Contract Errors

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
