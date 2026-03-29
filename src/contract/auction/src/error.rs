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

//! Auction contract errors

use thiserror::Error;

/// Auction contract errors
#[derive(Error, Debug)]
pub enum AuctionError {
    #[error("Auction not found")]
    AuctionNotFound,

    #[error("Bid not found")]
    BidNotFound,

    #[error("Invalid auction state: expected {expected:?}, got {actual:?}")]
    InvalidAuctionState { expected: String, actual: String },

    #[error("Invalid bid state: expected {expected:?}, got {actual:?}")]
    InvalidBidState { expected: String, actual: String },

    #[error("Auction is not active")]
    AuctionNotActive,

    #[error("Auction has not ended yet")]
    AuctionNotEnded,

    #[error("Bid amount must be greater than current highest bid")]
    BidTooLow,

    #[error("Bid must be at least the reserve price")]
    BelowReservePrice,

    #[error("Only the auction seller can perform this action")]
    NotSeller,

    #[error("Only the winner can claim winnings")]
    NotWinner,

    #[error("Only the bidder can request a refund")]
    NotBidder,

    #[error("Seller commitment mismatch")]
    SellerCommitmentMismatch,

    #[error("Winner public key mismatch")]
    WinnerPubkeyMismatch,

    #[error("Bidder public key mismatch")]
    BidderPubkeyMismatch,

    #[error("Settlement nullifier mismatch")]
    SettlementNullifierMismatch,

    #[error("Refund nullifier mismatch")]
    RefundNullifierMismatch,

    #[error("Nullifier already spent")]
    NullifierSpent,

    #[error("Invalid zk proof")]
    InvalidProof,

    #[error("Invalid signature")]
    InvalidSignature,
}