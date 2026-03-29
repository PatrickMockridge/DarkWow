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

//! Tender contract errors

use thiserror::Error;

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
}