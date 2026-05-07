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

//! Tender contract errors

use thiserror::Error;
use dwow_sdk::error::ContractError;

/// Tender contract errors
#[derive(Error, Debug)]
pub enum TenderError {
    #[error("Tender not found")]
    TenderNotFound,

    #[error("Bid not found")]
    BidNotFound,

    #[error("Invalid tender state: expected {expected:?}, got {actual:?}")]
    InvalidTenderState { expected: String, actual: String },

    #[error("Invalid bid state: expected {expected:?}, got {actual:?}")]
    InvalidBidState { expected: String, actual: String },

    #[error("Tender is not accepting bids")]
    TenderNotAcceptingBids,

    #[error("Bidding period has ended")]
    BiddingEnded,

    #[error("Reveal period has ended")]
    RevealEnded,

    #[error("Bid amount out of range")]
    BidAmountOutOfRange,

    #[error("Bid too low")]
    BidTooLow,

    #[error("Bid too high")]
    BidTooHigh,

    #[error("Only the tender requester can perform this action")]
    NotRequester,

    #[error("Only the bidder can reveal this bid")]
    NotBidder,

    #[error("Bid already revealed")]
    BidAlreadyRevealed,

    #[error("Bid not yet revealed")]
    BidNotRevealed,

    #[error("Winner already selected")]
    WinnerAlreadySelected,

    #[error("Competency requirements not met")]
    CompetencyRequirementsNotMet,

    #[error("No bids to select from")]
    NoBids,

    #[error("Nullifier already spent")]
    NullifierSpent,

    #[error("Invalid zk proof")]
    InvalidProof,

    #[error("Sled database error: {0}")]
    SledError(String),

    // O-Cap errors
    #[error("Capability required for this operation")]
    CapabilityRequired,

    #[error("Capability requirement not met")]
    CapabilityNotMet,

    #[error("Invalid capability")]
    InvalidCapability,

    #[error("DAG requirement not met")]
    DAGRequirementNotMet,
}

impl From<TenderError> for ContractError {
    fn from(e: TenderError) -> Self {
        match e {
            TenderError::TenderNotFound => Self::Custom(1),
            TenderError::BidNotFound => Self::Custom(2),
            TenderError::InvalidTenderState { .. } => Self::Custom(3),
            TenderError::InvalidBidState { .. } => Self::Custom(4),
            TenderError::TenderNotAcceptingBids => Self::Custom(5),
            TenderError::BiddingEnded => Self::Custom(6),
            TenderError::RevealEnded => Self::Custom(7),
            TenderError::BidAmountOutOfRange => Self::Custom(8),
            TenderError::BidTooLow => Self::Custom(9),
            TenderError::BidTooHigh => Self::Custom(10),
            TenderError::NotRequester => Self::Custom(11),
            TenderError::NotBidder => Self::Custom(12),
            TenderError::BidAlreadyRevealed => Self::Custom(13),
            TenderError::BidNotRevealed => Self::Custom(14),
            TenderError::WinnerAlreadySelected => Self::Custom(15),
            TenderError::CompetencyRequirementsNotMet => Self::Custom(16),
            TenderError::NoBids => Self::Custom(17),
            TenderError::NullifierSpent => Self::Custom(18),
            TenderError::InvalidProof => Self::Custom(19),
            TenderError::SledError(_) => Self::Custom(20),
            TenderError::CapabilityRequired => Self::Custom(28),
            TenderError::CapabilityNotMet => Self::Custom(29),
            TenderError::InvalidCapability => Self::Custom(30),
            TenderError::DAGRequirementNotMet => Self::Custom(31),
        }
    }
}