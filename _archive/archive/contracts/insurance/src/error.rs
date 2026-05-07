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

//! Insurance Contract Errors

use darkfi_sdk::error::ContractError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InsuranceError {
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

impl From<InsuranceError> for ContractError {
    fn from(e: InsuranceError) -> Self {
        match e {
            InsuranceError::PolicyNotFound => Self::Custom(1),
            InsuranceError::PolicyAlreadyExists => Self::Custom(2),
            InsuranceError::InvalidPolicyState => Self::Custom(3),
            InsuranceError::PolicyNotActive => Self::Custom(4),
            InsuranceError::InvalidCoveragePeriod => Self::Custom(5),
            InsuranceError::CoverageAlreadyStarted => Self::Custom(6),
            InsuranceError::InsufficientPremium => Self::Custom(7),
            InsuranceError::InsufficientPoolFunds => Self::Custom(8),
            InsuranceError::UnauthorizedCaller => Self::Custom(9),
            InsuranceError::InvalidSignature => Self::Custom(10),
            InsuranceError::ClaimNotFound => Self::Custom(11),
            InsuranceError::ClaimAlreadyProcessed => Self::Custom(12),
            InsuranceError::InvalidClaimAmount => Self::Custom(13),
            InsuranceError::CoverageRatioExceeded => Self::Custom(14),
            InsuranceError::ArithmeticOverflow => Self::Custom(15),
            InsuranceError::DivisionByZero => Self::Custom(16),
            InsuranceError::InvalidFunction => Self::Custom(17),
            InsuranceError::CrossContractFailed => Self::Custom(18),
        }
    }
}