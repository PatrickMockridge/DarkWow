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
    crypto::{pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    error::ContractError,
    pasta::pallas,
};

/// Auction unique identifier (hash of auction data)
pub type AuctionId = pallas::Base;

/// Bid unique identifier
pub type BidId = pallas::Base;

/// Represents the current state of an auction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
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
        let (sx, sy) = seller_pubkey.xy().expect("pk not identity");
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
#[derive(Debug, Clone)]
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
        let (bx, by) = bidder_pubkey.xy().expect("pk not identity");
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

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE — STORED TYPES + BRIDGE UPDATES
// ============================================================================

impl Auction {
    /// version(1) + id(32) + seller_pubkey(32) + item_commitment(32) + reserve_price(8)
    /// + token_id(32) + deadline_block(8) + state(1) + highest_bid(9)
    /// + highest_bidder(33) + highest_bid_id(33) + bid_count(8) + created_at(8) + instance_seed(32)
    pub const ENCODED_SIZE: usize = 269;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.id.to_repr());
        buf.extend_from_slice(&self.seller_pubkey.to_bytes());
        buf.extend_from_slice(&self.item_commitment.to_repr());
        buf.extend_from_slice(&self.reserve_price.to_le_bytes());
        buf.extend_from_slice(&self.token_id.to_repr());
        buf.extend_from_slice(&self.deadline_block.to_le_bytes());
        buf.push(self.state as u8);
        match self.highest_bid {
            Some(v) => { buf.push(1); buf.extend_from_slice(&v.to_le_bytes()); }
            None => { buf.push(0); buf.extend_from_slice(&[0u8; 8]); }
        }
        match &self.highest_bidder {
            Some(pk) => { buf.push(1); buf.extend_from_slice(&pk.to_bytes()); }
            None => { buf.push(0); buf.extend_from_slice(&[0u8; 32]); }
        }
        match self.highest_bid_id {
            Some(v) => { buf.push(1); buf.extend_from_slice(&v.to_repr()); }
            None => { buf.push(0); buf.extend_from_slice(&[0u8; 32]); }
        }
        buf.extend_from_slice(&self.bid_count.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Auction: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Auction: invalid id".into()))?;
        let seller_pubkey = PublicKey::from_bytes(data[33..65].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Auction: invalid seller_pubkey: {:?}", e)))?;
        let item_commitment = Option::<pallas::Base>::from(pallas::Base::from_repr(data[65..97].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Auction: invalid item_commitment".into()))?;
        let reserve_price = u64::from_le_bytes(data[97..105].try_into().unwrap());
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[105..137].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Auction: invalid token_id".into()))?;
        let deadline_block = u64::from_le_bytes(data[137..145].try_into().unwrap());
        let state = AuctionState::try_from(data[145])?;
        let highest_bid = if data[146] == 1 {
            Some(u64::from_le_bytes(data[147..155].try_into().unwrap()))
        } else { None };
        let highest_bidder = if data[155] == 1 {
            Some(PublicKey::from_bytes(data[156..188].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("Auction: invalid highest_bidder: {:?}", e)))?)
        } else { None };
        let highest_bid_id = if data[188] == 1 {
            Some(Option::<pallas::Base>::from(pallas::Base::from_repr(data[189..221].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("Auction: invalid highest_bid_id".into()))?)
        } else { None };
        let bid_count = u64::from_le_bytes(data[221..229].try_into().unwrap());
        let created_at = u64::from_le_bytes(data[229..237].try_into().unwrap());
        let instance_seed: [u8; 32] = data[237..269].try_into().unwrap();
        Ok(Auction { version, id, seller_pubkey, item_commitment, reserve_price, token_id, deadline_block, state, highest_bid, highest_bidder, highest_bid_id, bid_count, created_at, instance_seed })
    }
}

impl Bid {
    /// version(1) + id(32) + auction_id(32) + bidder_pubkey(32) + amount(8)
    /// + escrow_id(32) + state(1) + created_at(8) + instance_seed(32)
    pub const ENCODED_SIZE: usize = 178;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.id.to_repr());
        buf.extend_from_slice(&self.auction_id.to_repr());
        buf.extend_from_slice(&self.bidder_pubkey.to_bytes());
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.escrow_id.to_repr());
        buf.push(self.state as u8);
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Bid: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Bid: invalid id".into()))?;
        let auction_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Bid: invalid auction_id".into()))?;
        let bidder_pubkey = PublicKey::from_bytes(data[65..97].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Bid: invalid bidder_pubkey: {:?}", e)))?;
        let amount = u64::from_le_bytes(data[97..105].try_into().unwrap());
        let escrow_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[105..137].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Bid: invalid escrow_id".into()))?;
        let state = BidState::try_from(data[137])?;
        let created_at = u64::from_le_bytes(data[138..146].try_into().unwrap());
        let instance_seed: [u8; 32] = data[146..178].try_into().unwrap();
        Ok(Bid { version, id, auction_id, bidder_pubkey, amount, escrow_id, state, created_at, instance_seed })
    }
}

impl CreateAuctionUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let auction_enc = self.auction.encode();
        let mut buf = Vec::with_capacity(32 + auction_enc.len());
        buf.extend_from_slice(&self.auction_id.to_repr());
        buf.extend_from_slice(&auction_enc);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 {
            return Err(ContractError::IoError("CreateAuctionUpdateV1: data too short".into()));
        }
        let auction_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CreateAuctionUpdateV1: invalid auction_id".into()))?;
        let auction = Auction::decode(&data[32..])?;
        Ok(CreateAuctionUpdateV1 { auction_id, auction })
    }
}

impl PlaceBidUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let auction_enc = self.auction.encode();
        let bid_enc = self.bid.encode();
        let cap = 32 + 8 + 32 + 32 + auction_enc.len() + bid_enc.len()
            + 1 + 32 + (if self.prev_bid.is_some() { 1 + Bid::ENCODED_SIZE } else { 0 });
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.auction_id.to_repr());
        buf.extend_from_slice(&self.highest_bid.to_le_bytes());
        buf.extend_from_slice(&self.highest_bidder.to_bytes());
        buf.extend_from_slice(&self.highest_bid_id.to_repr());
        buf.extend_from_slice(&auction_enc);
        buf.extend_from_slice(&bid_enc);
        match &self.prev_bid_id {
            Some(v) => { buf.push(1); buf.extend_from_slice(&v.to_repr()); }
            None => { buf.push(0); buf.extend_from_slice(&[0u8; 32]); }
        }
        match &self.prev_bid {
            Some(b) => { buf.push(1); buf.extend_from_slice(&b.encode()); }
            None => { buf.push(0); }
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 + 8 + 32 + 32 {
            return Err(ContractError::IoError("PlaceBidUpdateV1: data too short".into()));
        }
        let auction_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PlaceBidUpdateV1: invalid auction_id".into()))?;
        let highest_bid = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let highest_bidder = PublicKey::from_bytes(data[40..72].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("PlaceBidUpdateV1: invalid highest_bidder: {:?}", e)))?;
        let highest_bid_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[72..104].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PlaceBidUpdateV1: invalid highest_bid_id".into()))?;
        let auction = Auction::decode(&data[104..104 + Auction::ENCODED_SIZE])?;
        let bid_start = 104 + Auction::ENCODED_SIZE;
        let bid = Bid::decode(&data[bid_start..bid_start + Bid::ENCODED_SIZE])?;
        let after_bid = bid_start + Bid::ENCODED_SIZE;
        if data.len() < after_bid + 33 {
            return Err(ContractError::IoError("PlaceBidUpdateV1: data too short for prev_bid_id".into()));
        }
        let prev_bid_id = if data[after_bid] == 1 {
            Some(Option::<pallas::Base>::from(pallas::Base::from_repr(data[after_bid+1..after_bid+33].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("PlaceBidUpdateV1: invalid prev_bid_id".into()))?)
        } else { None };
        let after_prev_id = after_bid + 33;
        let prev_bid = if data.len() > after_prev_id && data[after_prev_id] == 1 {
            Some(Bid::decode(&data[after_prev_id + 1..])?)
        } else { None };
        Ok(PlaceBidUpdateV1 { auction_id, highest_bid, highest_bidder, highest_bid_id, auction, bid, prev_bid_id, prev_bid })
    }
}

impl CloseAuctionUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let auction_enc = self.auction.encode();
        let cap = 32 + 32 + auction_enc.len() + 1
            + (if self.winner_bid.is_some() { Bid::ENCODED_SIZE } else { 0 });
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.auction_id.to_repr());
        buf.extend_from_slice(&self.winner_bid_id.to_repr());
        buf.extend_from_slice(&auction_enc);
        match &self.winner_bid {
            Some(b) => { buf.push(1); buf.extend_from_slice(&b.encode()); }
            None => { buf.push(0); }
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 + 32 + Auction::ENCODED_SIZE + 1 {
            return Err(ContractError::IoError("CloseAuctionUpdateV1: data too short".into()));
        }
        let auction_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CloseAuctionUpdateV1: invalid auction_id".into()))?;
        let winner_bid_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CloseAuctionUpdateV1: invalid winner_bid_id".into()))?;
        let auction = Auction::decode(&data[64..64 + Auction::ENCODED_SIZE])?;
        let after_auction = 64 + Auction::ENCODED_SIZE;
        let winner_bid = if data[after_auction] == 1 {
            Some(Bid::decode(&data[after_auction + 1..])?)
        } else { None };
        Ok(CloseAuctionUpdateV1 { auction_id, winner_bid_id, auction, winner_bid })
    }
}

impl ClaimWinningsUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let auction_enc = self.auction.encode();
        let mut buf = Vec::with_capacity(32 + 32 + auction_enc.len());
        buf.extend_from_slice(&self.auction_id.to_repr());
        buf.extend_from_slice(&self.winner_bid_id.to_repr());
        buf.extend_from_slice(&auction_enc);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 + 32 + Auction::ENCODED_SIZE {
            return Err(ContractError::IoError("ClaimWinningsUpdateV1: data too short".into()));
        }
        let auction_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClaimWinningsUpdateV1: invalid auction_id".into()))?;
        let winner_bid_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClaimWinningsUpdateV1: invalid winner_bid_id".into()))?;
        let auction = Auction::decode(&data[64..64 + Auction::ENCODED_SIZE])?;
        Ok(ClaimWinningsUpdateV1 { auction_id, winner_bid_id, auction })
    }
}

impl SettleAuctionUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let auction_enc = self.auction.encode();
        let mut buf = Vec::with_capacity(32 + 32 + auction_enc.len());
        buf.extend_from_slice(&self.auction_id.to_repr());
        buf.extend_from_slice(&self.settlement_nullifier.to_repr());
        buf.extend_from_slice(&auction_enc);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 + 32 + Auction::ENCODED_SIZE {
            return Err(ContractError::IoError("SettleAuctionUpdateV1: data too short".into()));
        }
        let auction_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("SettleAuctionUpdateV1: invalid auction_id".into()))?;
        let settlement_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("SettleAuctionUpdateV1: invalid settlement_nullifier".into()))?;
        let auction = Auction::decode(&data[64..64 + Auction::ENCODED_SIZE])?;
        Ok(SettleAuctionUpdateV1 { auction_id, settlement_nullifier, auction })
    }
}

impl RefundBidUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let bid_enc = self.bid.encode();
        let mut buf = Vec::with_capacity(32 + 32 + bid_enc.len());
        buf.extend_from_slice(&self.bid_id.to_repr());
        buf.extend_from_slice(&self.refund_nullifier.to_repr());
        buf.extend_from_slice(&bid_enc);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 + 32 + Bid::ENCODED_SIZE {
            return Err(ContractError::IoError("RefundBidUpdateV1: data too short".into()));
        }
        let bid_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("RefundBidUpdateV1: invalid bid_id".into()))?;
        let refund_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("RefundBidUpdateV1: invalid refund_nullifier".into()))?;
        let bid = Bid::decode(&data[64..64 + Bid::ENCODED_SIZE])?;
        Ok(RefundBidUpdateV1 { bid_id, refund_nullifier, bid })
    }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }

/// Parameters for `Auction::CreateAuctionV1`
#[derive(Debug, Clone,)]
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

impl CreateAuctionParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 266 + self.merkle_proof.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.seller_pubkey.to_bytes());
        buf.extend_from_slice(&self.item_commitment.to_repr());
        buf.extend_from_slice(&self.reserve_price.to_le_bytes());
        buf.extend_from_slice(&self.token_id.to_repr());
        buf.extend_from_slice(&self.deadline_block.to_le_bytes());
        buf.extend_from_slice(&self.auction_id.to_repr());
        buf.extend_from_slice(&self.seller_commitment.to_repr());
        buf.push(self.merkle_proof.len() as u8);
        for p in &self.merkle_proof { buf.extend_from_slice(&p.to_repr()); }
        buf.extend_from_slice(&self.merkle_root.to_repr());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 234 { return Err(ContractError::IoError("CreateAuctionParamsV1: too short".into())); }
        let seller_pubkey = PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CreateAuctionParamsV1: invalid seller_pubkey: {}", e)))?;
        let item_commitment = read_base(&data[32..64])?;
        let reserve_price = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let token_id = read_base(&data[72..104])?;
        let deadline_block = u64::from_le_bytes(data[104..112].try_into().unwrap());
        let auction_id = read_base(&data[112..144])?;
        let seller_commitment = read_base(&data[144..176])?;
        let proof_count = data[176] as usize;
        let mp_end = 177 + proof_count * 32;
        if data.len() < mp_end + 64 { return Err(ContractError::IoError("CreateAuctionParamsV1: merkle_proof truncated".into())); }
        let mut merkle_proof = Vec::with_capacity(proof_count);
        for i in 0..proof_count { merkle_proof.push(read_base(&data[177 + i*32..177 + (i+1)*32])?); }
        let merkle_root = read_base(&data[mp_end..mp_end+32])?;
        let instance_seed: [u8; 32] = data[mp_end+32..mp_end+64].try_into().unwrap();
        Ok(CreateAuctionParamsV1 { seller_pubkey, item_commitment, reserve_price, token_id, deadline_block, auction_id, seller_commitment, merkle_proof, merkle_root, instance_seed })
    }
}

/// State update for `Auction::CreateAuctionV1`
#[derive(Debug, Clone)]
pub struct CreateAuctionUpdateV1 {
    /// The created auction ID
    pub auction_id: AuctionId,
    /// The auction to store in apply phase
    pub auction: Auction,
}

/// Parameters for `Auction::PlaceBidV1`
#[derive(Debug, Clone,)]
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

impl PlaceBidParamsV1 {
    pub const ENCODED_SIZE: usize = 208;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.auction_id.to_repr()); buf.extend_from_slice(&self.bidder_pubkey.to_bytes());
        buf.extend_from_slice(&self.amount.to_le_bytes()); buf.extend_from_slice(&self.bid_nonce.to_repr());
        buf.extend_from_slice(&self.bid_id.to_repr()); buf.extend_from_slice(&self.escrow_id.to_repr());
        buf.extend_from_slice(&self.current_high_bid.to_le_bytes()); buf.extend_from_slice(&self.instance_seed); buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("PlaceBidParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        let auction_id = read_base(&data[0..32])?; let bidder_pubkey = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PlaceBidParamsV1: invalid bidder_pubkey: {}", e)))?;
        let amount = u64::from_le_bytes(data[64..72].try_into().unwrap()); let bid_nonce = read_base(&data[72..104])?;
        let bid_id = read_base(&data[104..136])?; let escrow_id = read_base(&data[136..168])?;
        let current_high_bid = u64::from_le_bytes(data[168..176].try_into().unwrap());
        let instance_seed: [u8; 32] = data[176..208].try_into().unwrap();
        Ok(PlaceBidParamsV1 { auction_id, bidder_pubkey, amount, bid_nonce, bid_id, escrow_id, current_high_bid, instance_seed })
    }
}

/// State update for `Auction::PlaceBidV1`
#[derive(Debug, Clone)] pub struct PlaceBidUpdateV1 { pub auction_id: AuctionId, pub highest_bid: u64, pub highest_bidder: PublicKey, pub highest_bid_id: BidId, pub auction: Auction, pub bid: Bid, pub prev_bid_id: Option<BidId>, pub prev_bid: Option<Bid> }

/// Parameters for `Auction::CloseAuctionV1`
#[derive(Debug, Clone,)] pub struct CloseAuctionParamsV1 { pub auction_id: AuctionId, pub winner_bid_id: BidId, pub seller_pubkey: PublicKey, pub current_block: u64 }
impl CloseAuctionParamsV1 {
    pub const ENCODED_SIZE: usize = 104;
    pub fn encode(&self) -> Vec<u8> { let mut buf = Vec::with_capacity(Self::ENCODED_SIZE); buf.extend_from_slice(&self.auction_id.to_repr()); buf.extend_from_slice(&self.winner_bid_id.to_repr()); buf.extend_from_slice(&self.seller_pubkey.to_bytes()); buf.extend_from_slice(&self.current_block.to_le_bytes()); buf }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("CloseAuctionParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); } Ok(CloseAuctionParamsV1 { auction_id: read_base(&data[0..32])?, winner_bid_id: read_base(&data[32..64])?, seller_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CloseAuctionParamsV1: invalid seller_pubkey: {}", e)))?, current_block: u64::from_le_bytes(data[96..104].try_into().unwrap()) }) }
}

/// State update for `Auction::CloseAuctionV1`
#[derive(Debug, Clone)] pub struct CloseAuctionUpdateV1 { pub auction_id: AuctionId, pub winner_bid_id: BidId, pub auction: Auction, pub winner_bid: Option<Bid> }

/// Parameters for `Auction::ClaimWinningsV1`
#[derive(Debug, Clone,)] pub struct ClaimWinningsParamsV1 { pub auction_id: AuctionId, pub winner_bid_id: BidId, pub winner_pubkey: PublicKey, pub winner_secret: pallas::Base }
impl ClaimWinningsParamsV1 {
    pub const ENCODED_SIZE: usize = 128;
    pub fn encode(&self) -> Vec<u8> { let mut buf = Vec::with_capacity(Self::ENCODED_SIZE); buf.extend_from_slice(&self.auction_id.to_repr()); buf.extend_from_slice(&self.winner_bid_id.to_repr()); buf.extend_from_slice(&self.winner_pubkey.to_bytes()); buf.extend_from_slice(&self.winner_secret.to_repr()); buf }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("ClaimWinningsParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); } Ok(ClaimWinningsParamsV1 { auction_id: read_base(&data[0..32])?, winner_bid_id: read_base(&data[32..64])?, winner_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ClaimWinningsParamsV1: invalid winner_pubkey: {}", e)))?, winner_secret: read_base(&data[96..128])? }) }
}

/// State update for `Auction::ClaimWinningsV1`
#[derive(Debug, Clone)] pub struct ClaimWinningsUpdateV1 { pub auction_id: AuctionId, pub winner_bid_id: BidId, pub auction: Auction }

/// Parameters for `Auction::SettleAuctionV1`
#[derive(Debug, Clone,)] pub struct SettleAuctionParamsV1 { pub auction_id: AuctionId, pub seller_pubkey: PublicKey, pub highest_bid_amount: u64, pub settlement_nullifier: pallas::Base, pub seller_secret: pallas::Base }
impl SettleAuctionParamsV1 {
    pub const ENCODED_SIZE: usize = 136;
    pub fn encode(&self) -> Vec<u8> { let mut buf = Vec::with_capacity(Self::ENCODED_SIZE); buf.extend_from_slice(&self.auction_id.to_repr()); buf.extend_from_slice(&self.seller_pubkey.to_bytes()); buf.extend_from_slice(&self.highest_bid_amount.to_le_bytes()); buf.extend_from_slice(&self.settlement_nullifier.to_repr()); buf.extend_from_slice(&self.seller_secret.to_repr()); buf }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("SettleAuctionParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); } Ok(SettleAuctionParamsV1 { auction_id: read_base(&data[0..32])?, seller_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("SettleAuctionParamsV1: invalid seller_pubkey: {}", e)))?, highest_bid_amount: u64::from_le_bytes(data[64..72].try_into().unwrap()), settlement_nullifier: read_base(&data[72..104])?, seller_secret: read_base(&data[104..136])? }) }
}

/// State update for `Auction::SettleAuctionV1`
#[derive(Debug, Clone)] pub struct SettleAuctionUpdateV1 { pub auction_id: AuctionId, pub settlement_nullifier: pallas::Base, pub auction: Auction }

/// Parameters for `Auction::RefundBidV1`
#[derive(Debug, Clone,)] pub struct RefundBidParamsV1 { pub bid_id: BidId, pub bidder_pubkey: PublicKey, pub refund_nullifier: pallas::Base, pub bidder_secret: pallas::Base }
impl RefundBidParamsV1 {
    pub const ENCODED_SIZE: usize = 128;
    pub fn encode(&self) -> Vec<u8> { let mut buf = Vec::with_capacity(Self::ENCODED_SIZE); buf.extend_from_slice(&self.bid_id.to_repr()); buf.extend_from_slice(&self.bidder_pubkey.to_bytes()); buf.extend_from_slice(&self.refund_nullifier.to_repr()); buf.extend_from_slice(&self.bidder_secret.to_repr()); buf }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("RefundBidParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); } Ok(RefundBidParamsV1 { bid_id: read_base(&data[0..32])?, bidder_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RefundBidParamsV1: invalid bidder_pubkey: {}", e)))?, refund_nullifier: read_base(&data[64..96])?, bidder_secret: read_base(&data[96..128])? }) }
}

/// State update for `Auction::RefundBidV1`
#[derive(Debug, Clone)]
pub struct RefundBidUpdateV1 {
    pub bid_id: BidId,
    pub refund_nullifier: pallas::Base,
    pub bid: Bid,
}