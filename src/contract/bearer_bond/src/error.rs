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

//! Bearer Bond Error types

pub use dwow_sdk::error::ContractError;
use thiserror::Error;

/// BearerBond-specific errors
#[derive(Debug, Error)]
pub enum BearerBondError {
    #[error("Invalid function code")]
    InvalidFunction,

    #[error("Stake not found")]
    StakeNotFound,

    #[error("Stake already exists")]
    StakeAlreadyExists,

    #[error("Stake not yet matured — current block {current}, maturity block {maturity}")]
    StakeNotMatured { current: u64, maturity: u64 },

    #[error("Stake already matured — cannot transfer after maturity")]
    StakeAlreadyMatured,

    #[error("Stake already unstaked")]
    StakeAlreadyUnstaked,

    #[error("coverage ratio below minimum: {reported} bps < 10000 bps")]
    InsufficientCoverage { reported: u64 },

    #[error("Invalid maturity — must be in the future")]
    InvalidMaturity,

    #[error("Invalid principal — must be non-zero")]
    InvalidPrincipal,

    #[error("Interest calculation overflow")]
    InterestOverflow,

    #[error("Invalid interest claim — last_claim_block {last} is not less than current block {current}")]
    InvalidInterestClaim { last: u64, current: u64 },

    #[error("Invalid block height")]
    InvalidBlockHeight,

    #[error("Invalid Merkle proof")]
    InvalidMerkleProof,

    #[error("Duplicate nullifier (double-spend)")]
    DuplicateNullifier,

    #[error("Value mismatch — input/output Pedersen commitment sums differ")]
    ValueMismatch,

    #[error("Issuer contract ID mismatch")]
    IssuerMismatch,

    #[error("Missing inputs")]
    MissingInputs,

    #[error("Missing outputs")]
    MissingOutputs,

    #[error("Token ID mismatch")]
    TokenIdMismatch,

    #[error("Invalid Schnorr signature")]
    InvalidSchnorrSignature,

    #[error("Public key does not match secret")]
    PublicKeyMismatch,

    #[error("Series is voided — coverage fell below minimum, only emergency unstake allowed")]
    SeriesVoided,

    #[error("Series is not active — current status prevents this operation")]
    SeriesNotActive,

    #[error("Insufficient reserves for interest obligation — reserve {reserve}, obligation {obligation}")]
    InsufficientReserveForInterest { reserve: u64, obligation: u64 },

    #[error("No coverage report found for this series")]
    CoverageNotVerified,

    #[error("Roots value data length mismatch")]
    RootsValueDataMismatch,

    #[error("coverage report already exists for this block")]
    CoverageReportExists,

    #[error("invalid coverage proof")]
    InvalidCoverageProof,

    #[error("Invalid interest rate — must be non-zero")]
    InvalidInterestRate,

    #[error("Emergency unstake not allowed — coverage is above minimum")]
    EmergencyUnstakeNotAllowed,
}

impl BearerBondError {
    /// Convert to a numeric error code for ContractError::Custom.
    pub fn code(&self) -> u32 {
        match self {
            BearerBondError::InvalidFunction => 0,
            BearerBondError::StakeNotFound => 1,
            BearerBondError::StakeAlreadyExists => 2,
            BearerBondError::StakeNotMatured { .. } => 3,
            BearerBondError::StakeAlreadyMatured => 4,
            BearerBondError::StakeAlreadyUnstaked => 5,
            BearerBondError::InsufficientCoverage { .. } => 6,
            BearerBondError::InvalidMaturity => 7,
            BearerBondError::InvalidPrincipal => 8,
            BearerBondError::InterestOverflow => 9,
            BearerBondError::InvalidInterestClaim { .. } => 10,
            BearerBondError::InvalidBlockHeight => 11,
            BearerBondError::InvalidMerkleProof => 12,
            BearerBondError::DuplicateNullifier => 13,
            BearerBondError::ValueMismatch => 14,
            BearerBondError::IssuerMismatch => 15,
            BearerBondError::MissingInputs => 16,
            BearerBondError::MissingOutputs => 17,
            BearerBondError::TokenIdMismatch => 18,
            BearerBondError::InvalidSchnorrSignature => 19,
            BearerBondError::PublicKeyMismatch => 20,
            BearerBondError::SeriesVoided => 21,
            BearerBondError::SeriesNotActive => 22,
            BearerBondError::InsufficientReserveForInterest { .. } => 23,
            BearerBondError::CoverageNotVerified => 24,
            BearerBondError::RootsValueDataMismatch => 25,
            BearerBondError::CoverageReportExists => 26,
            BearerBondError::InvalidCoverageProof => 27,
            BearerBondError::InvalidInterestRate => 28,
            BearerBondError::EmergencyUnstakeNotAllowed => 29,
        }
    }
}

impl From<BearerBondError> for ContractError {
    fn from(e: BearerBondError) -> Self {
        ContractError::Custom(e.code())
    }
}
