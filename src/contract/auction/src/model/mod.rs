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

//! Auction contract data structures
//!
//! ## Auction State Machine
//!
//! ```text
//! Created ──[PlaceBid]──> Active ──[Close]──> Closed ──[Settle]──> Settled
//!                                            │
//!                         ┌──────────────────┴──────────────────┐
//!                         │                                     │
//!                   [ClaimWinnings]                      [RefundBid]
//!                         │                                     │
//!                         ▼                                     ▼
//!                    Winner Paid                          Bids Refunded
//! ```
//!
//! ## Bid State Machine
//!
//! ```text
//! Active ──[Outbid]──> Outbid ──[Refund]──> Refunded
//!    │
//!    └──[Close]──> Won ──[Claim]──> Claimed
//! ```

use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Auction unique identifier (hash of auction data)
pub type AuctionId = pallas::Base;

/// Bid unique identifier
pub type BidId = pallas::Base;

/// Represents the current state of an auction
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum AuctionState {
    /// Auction created, not yet started accepting bids
    Created = 0,
    /// Auction is accepting bids
    Active = 1,
    /// Auction has ended
    Closed = 2,
    /// Auction settled, all funds distributed
    Settled = 3,
}

impl TryFrom<u8> for AuctionState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Created),
            1 => Ok(Self::Active),
            2 => Ok(Self::Closed),
            3 => Ok(Self::Settled),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Represents the current state of a bid
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum BidState {
    /// Bid is currently the highest (active)
    Active = 0,
    /// Bid was outbid by another bid
    Outbid = 1,
    /// Bid won the auction
    Won = 2,
    /// Bid amount was refunded
    Refunded = 3,
}

impl TryFrom<u8> for BidState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Outbid),
            2 => Ok(Self::Won),
            3 => Ok(Self::Refunded),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Core auction data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Auction {
    pub version: u8,
    /// Auction identifier (commitment)
    pub id: AuctionId,
    /// Seller's public key
    pub seller_pubkey: PublicKey,
    /// Commitment to the item being auctioned (H(item_description))
    pub item_commitment: pallas::Base,
    /// Minimum bid price
    pub reserve_price: u64,
    /// Token ID for bidding
    pub token_id: pallas::Base,
    /// Block height when auction ends
    pub deadline_block: u64,
    /// Current state
    pub state: AuctionState,
    /// Highest bid amount (None if no bids yet)
    pub highest_bid: Option<u64>,
    /// Highest bidder's public key
    pub highest_bidder: Option<PublicKey>,
    /// ID of the winning bid
    pub highest_bid_id: Option<BidId>,
    /// Total number of bids
    pub bid_count: u64,
    /// Block height when auction was created
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}

impl Auction {
    /// Derive the auction ID from auction parameters
    #[allow(dead_code)]
    pub fn derive_id(
        seller_pubkey: &PublicKey,
        item_commitment: pallas::Base,
        reserve_price: u64,
        token_id: pallas::Base,
        deadline_block: u64,
        seller_secret: pallas::Base,
    ) -> AuctionId {
        let (sx, sy) = seller_pubkey.xy();
        poseidon_hash([
            sx,
            sy,
            item_commitment,
            pallas::Base::from(reserve_price),
            token_id,
            pallas::Base::from(deadline_block),
            seller_secret,
        ])
    }

    /// Compute the settlement nullifier for this auction
    #[allow(dead_code)]
    pub fn compute_settlement_nullifier(&self, seller_secret: pallas::Base) -> pallas::Base {
        poseidon_hash([self.id, seller_secret])
    }
}

/// Core bid data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Bid {
    pub version: u8,
    /// Bid identifier (commitment)
    pub id: BidId,
    /// Auction this bid is for
    pub auction_id: AuctionId,
    /// Bidder's public key
    pub bidder_pubkey: PublicKey,
    /// Bid amount
    pub amount: u64,
    /// ID of the escrow holding this bid's deposit
    pub escrow_id: pallas::Base,
    /// Current state
    pub state: BidState,
    /// Block height when bid was placed
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}

impl Bid {
    /// Derive the bid ID from bid parameters
    #[allow(dead_code)]
    pub fn derive_id(
        auction_id: AuctionId,
        bidder_pubkey: &PublicKey,
        amount: u64,
        bid_nonce: pallas::Base,
    ) -> BidId {
        let (bx, by) = bidder_pubkey.xy();
        poseidon_hash([
            auction_id,
            bx,
            by,
            pallas::Base::from(amount),
            bid_nonce,
        ])
    }

    /// Compute the refund nullifier for this bid
    #[allow(dead_code)]
    pub fn compute_refund_nullifier(&self, bidder_secret: pallas::Base) -> pallas::Base {
        poseidon_hash([self.id, bidder_secret])
    }
}

/// Parameters for `Auction::CreateAuctionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateAuctionParamsV1 {
    /// Seller's public key
    pub seller_pubkey: PublicKey,
    /// Commitment to the item
    pub item_commitment: pallas::Base,
    /// Minimum bid price
    pub reserve_price: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Auction deadline block height
    pub deadline_block: u64,
    /// Commitment to the auction parameters
    pub auction_id: AuctionId,
    /// Seller's public key commitment (for privacy)
    pub seller_commitment: pallas::Base,
    /// Merkle proof for the zk proof
    pub merkle_proof: Vec<pallas::Base>,
    /// Merkle root
    pub merkle_root: pallas::Base,
    pub instance_seed: [u8; 32],
}

/// State update for `Auction::CreateAuctionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateAuctionUpdateV1 {
    /// The created auction ID
    pub auction_id: AuctionId,
}

/// Parameters for `Auction::PlaceBidV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBidParamsV1 {
    /// Auction ID
    pub auction_id: AuctionId,
    /// Bidder's public key
    pub bidder_pubkey: PublicKey,
    /// Bid amount
    pub amount: u64,
    /// Unique nonce for this bid
    pub bid_nonce: pallas::Base,
    /// Bid ID commitment
    pub bid_id: BidId,
    /// ID of the escrow for this bid's deposit
    pub escrow_id: pallas::Base,
    /// Current highest bid (for verification)
    pub current_high_bid: u64,
    pub instance_seed: [u8; 32],
}

/// State update for `Auction::PlaceBidV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBidUpdateV1 {
    /// The auction ID
    pub auction_id: AuctionId,
    /// The new highest bid
    pub highest_bid: u64,
    /// The new highest bidder
    pub highest_bidder: PublicKey,
    /// The winning bid ID
    pub highest_bid_id: BidId,
}

/// Parameters for `Auction::CloseAuctionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CloseAuctionParamsV1 {
    /// Auction ID
    pub auction_id: AuctionId,
    /// Winner's bid ID
    pub winner_bid_id: BidId,
    /// Seller's public key
    pub seller_pubkey: PublicKey,
    /// Current block height
    pub current_block: u64,
}

/// State update for `Auction::CloseAuctionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CloseAuctionUpdateV1 {
    /// The closed auction ID
    pub auction_id: AuctionId,
    /// The winning bid ID
    pub winner_bid_id: BidId,
}

/// Parameters for `Auction::ClaimWinningsV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimWinningsParamsV1 {
    /// Auction ID
    pub auction_id: AuctionId,
    /// Winner's bid ID
    pub winner_bid_id: BidId,
    /// Winner's public key
    pub winner_pubkey: PublicKey,
    /// Winner's secret (for proof)
    pub winner_secret: pallas::Base,
}

/// State update for `Auction::ClaimWinningsV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimWinningsUpdateV1 {
    /// The auction ID
    pub auction_id: AuctionId,
    /// The winning bid ID
    pub winner_bid_id: BidId,
}

/// Parameters for `Auction::SettleAuctionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleAuctionParamsV1 {
    /// Auction ID
    pub auction_id: AuctionId,
    /// Seller's public key
    pub seller_pubkey: PublicKey,
    /// Highest bid amount
    pub highest_bid_amount: u64,
    /// Settlement nullifier
    pub settlement_nullifier: pallas::Base,
    /// Seller's secret (for proof)
    pub seller_secret: pallas::Base,
}

/// State update for `Auction::SettleAuctionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleAuctionUpdateV1 {
    /// The settled auction ID
    pub auction_id: AuctionId,
    /// The settlement nullifier
    pub settlement_nullifier: pallas::Base,
}

/// Parameters for `Auction::RefundBidV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundBidParamsV1 {
    /// Bid ID
    pub bid_id: BidId,
    /// Bidder's public key
    pub bidder_pubkey: PublicKey,
    /// Refund nullifier
    pub refund_nullifier: pallas::Base,
    /// Bidder's secret (for proof)
    pub bidder_secret: pallas::Base,
}

/// State update for `Auction::RefundBidV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundBidUpdateV1 {
    /// The refunded bid ID
    pub bid_id: BidId,
    /// The refund nullifier
    pub refund_nullifier: pallas::Base,
}