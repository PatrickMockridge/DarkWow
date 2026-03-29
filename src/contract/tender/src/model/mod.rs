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

//! Tender contract data structures
//!
//! ## Tender State Machine
//!
//! ```text
//! Created ──[SubmitBid]──> Bidding ──[Close]──> Revealed ──[Select]──> Awarded
//!                                                │
//!                                                └──[Cancel]──> Cancelled
//! ```
//!
//! ## Bid State Machine
//!
//! ```text
//! Sealed ──[Reveal]──> Revealed ──[Accept]──> Accepted
//!   │                        │
//!   └──[Timeout]──> Expired  └──[Reject]──> Rejected
//! ```

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Tender unique identifier (hash of tender data)
pub type TenderId = pallas::Base;

/// Bid unique identifier
pub type BidId = pallas::Base;

/// Represents the current state of a tender
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum TenderState {
    /// Tender created, accepting bids
    Created = 0,
    /// Tender is accepting bids (transitioned from Created on first bid)
    Bidding = 1,
    /// Bidding period ended, revealing bids
    Revealed = 2,
    /// Winner selected, job created in labor market
    Awarded = 3,
    /// Tender cancelled
    Cancelled = 4,
}

impl TryFrom<u8> for TenderState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Created),
            1 => Ok(Self::Bidding),
            2 => Ok(Self::Revealed),
            3 => Ok(Self::Awarded),
            4 => Ok(Self::Cancelled),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Represents the current state of a bid
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum BidState {
    /// Bid submitted but not yet revealed
    Sealed = 0,
    /// Bid revealed (amount public)
    Revealed = 1,
    /// Bid accepted as winning bid
    Accepted = 2,
    /// Bid rejected after reveal
    Rejected = 3,
    /// Bid expired (not revealed in time)
    Expired = 4,
}

impl TryFrom<u8> for BidState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Sealed),
            1 => Ok(Self::Revealed),
            2 => Ok(Self::Accepted),
            3 => Ok(Self::Rejected),
            4 => Ok(Self::Expired),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Core tender data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Tender {
    /// Tender identifier (commitment)
    pub id: TenderId,
    /// Requester's public key (who created the tender)
    pub requester_pubkey: PublicKey,
    /// Title of the tender
    pub title: String,
    /// Hash of the specification document
    pub specification: pallas::Base,
    /// Commitment to competency requirements
    pub requirement_commitment: pallas::Base,
    /// Minimum bid amount
    pub min_bid: u64,
    /// Maximum bid amount
    pub max_bid: u64,
    /// Block height when bidding closes
    pub bid_deadline: u64,
    /// Block height when reveal period ends
    pub reveal_deadline: u64,
    /// Block height when delivery is due
    pub delivery_deadline: u64,
    /// Current state
    pub state: TenderState,
    /// ID of the winning bid
    pub selected_bid_id: Option<BidId>,
    /// Total number of bids received
    pub bid_count: u64,
    /// Block height when tender was created
    pub created_at: u64,
}

impl Tender {
    /// Derive the tender ID from tender parameters
    #[allow(dead_code)]
    pub fn derive_id(
        requester_pubkey: &PublicKey,
        title: &str,
        specification: pallas::Base,
        requirement_commitment: pallas::Base,
        min_bid: u64,
        max_bid: u64,
        bid_deadline: u64,
        reveal_deadline: u64,
        delivery_deadline: u64,
        requester_secret: pallas::Base,
    ) -> TenderId {
        let (rx, ry) = requester_pubkey.xy();
        poseidon_hash([
            rx,
            ry,
            pallas::Base::from_text(&title).unwrap_or_default(),
            specification,
            requirement_commitment,
            pallas::Base::from(min_bid),
            pallas::Base::from(max_bid),
            pallas::Base::from(bid_deadline),
            pallas::Base::from(reveal_deadline),
            pallas::Base::from(delivery_deadline),
            requester_secret,
        ])
    }
}

/// Core bid data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Bid {
    /// Bid identifier (commitment)
    pub id: BidId,
    /// Tender this bid is for
    pub tender_id: TenderId,
    /// Bidder's public key
    pub bidder_pubkey: PublicKey,
    /// Bid amount (hidden until reveal)
    pub amount: u64,
    /// Commitment to competency proof (verifiable off-chain)
    pub competency_commitment: pallas::Base,
    /// Encrypted bid details (decrypted by requester after reveal)
    pub encrypted_payload: Vec<u8>,
    /// Current state
    pub state: BidState,
    /// Amount revealed (if revealed)
    pub revealed_amount: Option<u64>,
    /// Block height when bid was submitted
    pub created_at: u64,
}

impl Bid {
    /// Derive the bid ID from bid parameters
    #[allow(dead_code)]
    pub fn derive_id(
        tender_id: TenderId,
        bidder_pubkey: &PublicKey,
        amount: u64,
        bid_nonce: pallas::Base,
    ) -> BidId {
        let (bx, by) = bidder_pubkey.xy();
        poseidon_hash([
            tender_id,
            bx,
            by,
            pallas::Base::from(amount),
            bid_nonce,
        ])
    }
}

/// Parameters for creating a new tender
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateTenderParamsV1 {
    /// ZK proof for tender creation
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Requester's public key x coordinate
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate
    pub requester_pub_y: pallas::Base,
    /// Title of the tender
    pub title: String,
    /// Hash of the specification document
    pub specification: pallas::Base,
    /// Commitment to competency requirements
    pub requirement_commitment: pallas::Base,
    /// Minimum bid amount
    pub min_bid: u64,
    /// Maximum bid amount
    pub max_bid: u64,
    /// Bidding deadline block
    pub bid_deadline: u64,
    /// Reveal deadline block
    pub reveal_deadline: u64,
    /// Delivery deadline block
    pub delivery_deadline: u64,
}

/// State update for CreateTenderV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateTenderUpdateV1 {
    /// The created tender ID
    pub tender_id: TenderId,
}

/// Parameters for submitting a bid
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubmitBidParamsV1 {
    /// ZK proof for bid submission
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Bid ID
    pub bid_id: BidId,
    /// Bidder's public key x coordinate
    pub bidder_pub_x: pallas::Base,
    /// Bidder's public key y coordinate
    pub bidder_pub_y: pallas::Base,
    /// Bid amount (hidden)
    pub amount: u64,
    /// Commitment to competency proof
    pub competency_commitment: pallas::Base,
    /// Encrypted bid details
    pub encrypted_payload: Vec<u8>,
}

/// State update for SubmitBidV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubmitBidUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The submitted bid ID
    pub bid_id: BidId,
}

/// Parameters for revealing a bid
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealBidParamsV1 {
    /// ZK proof for bid reveal
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Bid ID
    pub bid_id: BidId,
    /// Revealed bid amount
    pub revealed_amount: u64,
}

/// State update for RevealBidV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealBidUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The revealed bid ID
    pub bid_id: BidId,
}

/// Parameters for closing bidding and starting reveal
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CloseTenderParamsV1 {
    /// Tender ID
    pub tender_id: TenderId,
    /// Requester's public key
    pub requester_pubkey: PublicKey,
}

/// State update for CloseTenderV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CloseTenderUpdateV1 {
    /// The closed tender ID
    pub tender_id: TenderId,
}

/// Parameters for selecting winner
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SelectWinnerParamsV1 {
    /// ZK proof for winner selection
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Winner's bid ID
    pub winner_bid_id: BidId,
    /// Winner's public key
    pub winner_pubkey: PublicKey,
    /// Winning bid amount
    pub winning_amount: u64,
}

/// State update for SelectWinnerV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SelectWinnerUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The winning bid ID
    pub winner_bid_id: BidId,
    /// The job ID in labor market (for tracking)
    pub labor_job_id: Option<pallas::Base>,
}

/// Parameters for cancelling a tender
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelTenderParamsV1 {
    /// Tender ID
    pub tender_id: TenderId,
    /// Requester's public key
    pub requester_pubkey: PublicKey,
}

/// State update for CancelTenderV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelTenderUpdateV1 {
    /// The cancelled tender ID
    pub tender_id: TenderId,
}

/// Parameters for rejecting a bid
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RejectBidParamsV1 {
    /// Tender ID
    pub tender_id: TenderId,
    /// Bid ID being rejected
    pub bid_id: BidId,
    /// Requester's public key
    pub requester_pubkey: PublicKey,
}

/// State update for RejectBidV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RejectBidUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The rejected bid ID
    pub bid_id: BidId,
}