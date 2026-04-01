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

//! Plain Insurance Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InsurancePlainError {
    #[error("Policy not found")]
    PolicyNotFound,

    #[error("Policy already exists")]
    PolicyAlreadyExists,

    #[error("Policy not in expected state")]
    InvalidPolicyState,

    #[error("Policy not active")]
    PolicyNotActive,

    #[error("Invalid coverage period")]
    InvalidCoveragePeriod,

    #[error("Coverage period already started")]
    CoverageAlreadyStarted,

    #[error("Insufficient premium")]
    InsufficientPremium,

    #[error("Insufficient pool funds")]
    InsufficientPoolFunds,

    #[error("Unauthorized caller")]
    UnauthorizedCaller,

    #[error("Signature verification failed")]
    InvalidSignature,

    #[error("Claim not found")]
    ClaimNotFound,

    #[error("Claim already processed")]
    ClaimAlreadyProcessed,

    #[error("Invalid claim amount")]
    InvalidClaimAmount,

    #[error("Coverage ratio exceeded")]
    CoverageRatioExceeded,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Invalid function call")]
    InvalidFunction,

    #[error("Cross-contract call failed")]
    CrossContractFailed,
}

impl From<InsurancePlainError> for ContractError {
    fn from(e: InsurancePlainError) -> Self {
        match e {
            InsurancePlainError::PolicyNotFound => Self::Custom(1),
            InsurancePlainError::PolicyAlreadyExists => Self::Custom(2),
            InsurancePlainError::InvalidPolicyState => Self::Custom(3),
            InsurancePlainError::PolicyNotActive => Self::Custom(4),
            InsurancePlainError::InvalidCoveragePeriod => Self::Custom(5),
            InsurancePlainError::CoverageAlreadyStarted => Self::Custom(6),
            InsurancePlainError::InsufficientPremium => Self::Custom(7),
            InsurancePlainError::InsufficientPoolFunds => Self::Custom(8),
            InsurancePlainError::UnauthorizedCaller => Self::Custom(9),
            InsurancePlainError::InvalidSignature => Self::Custom(10),
            InsurancePlainError::ClaimNotFound => Self::Custom(11),
            InsurancePlainError::ClaimAlreadyProcessed => Self::Custom(12),
            InsurancePlainError::InvalidClaimAmount => Self::Custom(13),
            InsurancePlainError::CoverageRatioExceeded => Self::Custom(14),
            InsurancePlainError::ArithmeticOverflow => Self::Custom(15),
            InsurancePlainError::DivisionByZero => Self::Custom(16),
            InsurancePlainError::InvalidFunction => Self::Custom(17),
            InsurancePlainError::CrossContractFailed => Self::Custom(18),
        }
    }
}