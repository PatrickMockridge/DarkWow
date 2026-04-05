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

//! Insurance Market Contract Errors

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum InsuranceMarketError {
    #[error("Risk type already registered")]
    RiskTypeAlreadyExists,

    #[error("Risk type not found")]
    RiskTypeNotFound,

    #[error("Insurance market not found")]
    MarketNotFound,

    #[error("Insurance market already exists")]
    MarketAlreadyExists,

    #[error("Underwriter not found")]
    UnderwriterNotFound,

    #[error("Coverage not found")]
    CoverageNotFound,

    #[error("Claim not found")]
    ClaimNotFound,

    #[error("Insufficient bond amount")]
    InsufficientBond,

    #[error("Insufficient coverage amount")]
    InsufficientCoverage,

    #[error("Insufficient premium balance")]
    InsufficientPremium,

    #[error("Invalid premium rate")]
    InvalidPremiumRate,

    #[error("Invalid coverage period")]
    InvalidCoveragePeriod,

    #[error("Claim already resolved")]
    ClaimAlreadyResolved,

    #[error("Claim not covered")]
    ClaimNotCovered,

    #[error("Coverage expired")]
    CoverageExpired,

    #[error("Coverage already active")]
    CoverageAlreadyActive,

    #[error("Unauthorized underwriter")]
    UnauthorizedUnderwriter,

    #[error("Unauthorized claim resolver")]
    UnauthorizedResolver,

    #[error("Bond too small for coverage")]
    BondTooSmall,

    #[error("Market not active")]
    MarketNotActive,

    #[error("Endowment pool not found")]
    EndowmentPoolNotFound,

    #[error("Invalid risk category")]
    InvalidRiskCategory,

    #[error("Oracle attestation invalid")]
    InvalidOracleAttestation,

    #[error("Slash amount exceeds bond")]
    SlashExceedsBond,

    #[error("Transfer failed")]
    TransferFailed,

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Arithmetic overflow in calculation")]
    ArithmeticOverflow,

    // O-Cap capability errors
    #[error("Capability required for this operation")]
    CapabilityRequired,

    #[error("Capability requirement not met")]
    CapabilityNotMet,

    #[error("Invalid capability")]
    InvalidCapability,

    #[error("Capability revoked")]
    CapabilityRevoked,

    #[error("DAG requirement not met")]
    DAGRequirementNotMet,
}

impl From<InsuranceMarketError> for ContractError {
    fn from(e: InsuranceMarketError) -> Self {
        match e {
            InsuranceMarketError::RiskTypeAlreadyExists => Self::Custom(1),
            InsuranceMarketError::RiskTypeNotFound => Self::Custom(2),
            InsuranceMarketError::MarketNotFound => Self::Custom(3),
            InsuranceMarketError::MarketAlreadyExists => Self::Custom(4),
            InsuranceMarketError::UnderwriterNotFound => Self::Custom(5),
            InsuranceMarketError::CoverageNotFound => Self::Custom(6),
            InsuranceMarketError::ClaimNotFound => Self::Custom(7),
            InsuranceMarketError::InsufficientBond => Self::Custom(8),
            InsuranceMarketError::InsufficientCoverage => Self::Custom(9),
            InsuranceMarketError::InsufficientPremium => Self::Custom(10),
            InsuranceMarketError::InvalidPremiumRate => Self::Custom(11),
            InsuranceMarketError::InvalidCoveragePeriod => Self::Custom(12),
            InsuranceMarketError::ClaimAlreadyResolved => Self::Custom(13),
            InsuranceMarketError::ClaimNotCovered => Self::Custom(14),
            InsuranceMarketError::CoverageExpired => Self::Custom(15),
            InsuranceMarketError::CoverageAlreadyActive => Self::Custom(16),
            InsuranceMarketError::UnauthorizedUnderwriter => Self::Custom(17),
            InsuranceMarketError::UnauthorizedResolver => Self::Custom(18),
            InsuranceMarketError::BondTooSmall => Self::Custom(19),
            InsuranceMarketError::MarketNotActive => Self::Custom(20),
            InsuranceMarketError::EndowmentPoolNotFound => Self::Custom(21),
            InsuranceMarketError::InvalidRiskCategory => Self::Custom(22),
            InsuranceMarketError::InvalidOracleAttestation => Self::Custom(23),
            InsuranceMarketError::SlashExceedsBond => Self::Custom(24),
            InsuranceMarketError::TransferFailed => Self::Custom(25),
            InsuranceMarketError::InvalidParameter(_) => Self::Custom(26),
            InsuranceMarketError::ArithmeticOverflow => Self::Custom(27),
            InsuranceMarketError::CapabilityRequired => Self::Custom(28),
            InsuranceMarketError::CapabilityNotMet => Self::Custom(29),
            InsuranceMarketError::InvalidCapability => Self::Custom(30),
            InsuranceMarketError::CapabilityRevoked => Self::Custom(31),
            InsuranceMarketError::DAGRequirementNotMet => Self::Custom(32),
        }
    }
}