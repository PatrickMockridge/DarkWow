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

    #[error("Invalid children: expected 1 money_v3::transfer_v1 child call")]
    InvalidChildrenIndexes,

    #[error("Child call is not money_v3::transfer_v1 (0x04)")]
    InvalidChildCall,
}

impl From<AuctionError> for darkfi_sdk::error::ContractError {
    fn from(e: AuctionError) -> Self {
        match e {
            AuctionError::AuctionNotFound => Self::Custom(1),
            AuctionError::BidNotFound => Self::Custom(2),
            AuctionError::InvalidAuctionState { .. } => Self::Custom(3),
            AuctionError::InvalidBidState { .. } => Self::Custom(4),
            AuctionError::AuctionNotActive => Self::Custom(5),
            AuctionError::AuctionNotEnded => Self::Custom(6),
            AuctionError::BidTooLow => Self::Custom(7),
            AuctionError::BelowReservePrice => Self::Custom(8),
            AuctionError::NotSeller => Self::Custom(9),
            AuctionError::NotWinner => Self::Custom(10),
            AuctionError::NotBidder => Self::Custom(11),
            AuctionError::SellerCommitmentMismatch => Self::Custom(12),
            AuctionError::WinnerPubkeyMismatch => Self::Custom(13),
            AuctionError::BidderPubkeyMismatch => Self::Custom(14),
            AuctionError::SettlementNullifierMismatch => Self::Custom(15),
            AuctionError::RefundNullifierMismatch => Self::Custom(16),
            AuctionError::NullifierSpent => Self::Custom(17),
            AuctionError::InvalidProof => Self::Custom(18),
            AuctionError::InvalidSignature => Self::Custom(19),
            AuctionError::InvalidChildrenIndexes => Self::Custom(20),
            AuctionError::InvalidChildCall => Self::Custom(21),
        }
    }
}