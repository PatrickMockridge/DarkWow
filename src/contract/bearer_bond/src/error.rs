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

    #[error("Insufficient profits accrued — share {share}, minimum {minimum}")]
    InsufficientProfits { share: u64, minimum: u64 },

    #[error("Profit calculation overflow")]
    ProfitOverflow,

    #[error("Invalid profit claim — last_claim_block {last} is not less than current block {current}")]
    InvalidProfitClaim { last: u64, current: u64 },

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

    #[error("Profit already claimed for this block")]
    ProfitAlreadyClaimed,

    #[error("No profits declared for this series")]
    NoProfitsDeclared,

    #[error("Invalid profit declaration — start block {start} must be less than end block {end}")]
    InvalidProfitDeclaration { start: u64, end: u64 },

    #[error("Invalid profit amount — must be non-zero")]
    InvalidProfitAmount,

    #[error("Total staked is zero — cannot compute profit share")]
    ZeroTotalStaked,

    #[error("Roots value data length mismatch")]
    RootsValueDataMismatch,

    #[error("coverage report already exists for this block")]
    CoverageReportExists,

    #[error("invalid coverage proof")]
    InvalidCoverageProof,
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
            BearerBondError::InsufficientProfits { .. } => 9,
            BearerBondError::ProfitOverflow => 10,
            BearerBondError::InvalidProfitClaim { .. } => 11,
            BearerBondError::InvalidBlockHeight => 12,
            BearerBondError::InvalidMerkleProof => 13,
            BearerBondError::DuplicateNullifier => 14,
            BearerBondError::ValueMismatch => 15,
            BearerBondError::IssuerMismatch => 16,
            BearerBondError::MissingInputs => 17,
            BearerBondError::MissingOutputs => 18,
            BearerBondError::TokenIdMismatch => 19,
            BearerBondError::InvalidSchnorrSignature => 20,
            BearerBondError::PublicKeyMismatch => 21,
            BearerBondError::ProfitAlreadyClaimed => 22,
            BearerBondError::NoProfitsDeclared => 23,
            BearerBondError::InvalidProfitDeclaration { .. } => 24,
            BearerBondError::InvalidProfitAmount => 25,
            BearerBondError::ZeroTotalStaked => 26,
            BearerBondError::RootsValueDataMismatch => 27,
            BearerBondError::CoverageReportExists => 28,
            BearerBondError::InvalidCoverageProof => 29,
        }
    }
}

impl From<BearerBondError> for ContractError {
    fn from(e: BearerBondError) -> Self {
        ContractError::Custom(e.code())
    }
}
