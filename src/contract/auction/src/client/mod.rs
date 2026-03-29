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

//! Auction contract client API
//!
//! This module provides builder structs for constructing auction contract calls.
//!
//! ## Usage
//!
//! ```ignore
//! use darkfi_auction_contract::client::{CreateAuctionBuilder, PlaceBidBuilder};
//!
//! // Seller creates auction
//! let create_auction = CreateAuctionBuilder::new()
//!     .seller_pubkey(seller_pubkey)
//!     .item_commitment(item_hash)
//!     .reserve_price(1000)
//!     .token_id(DRK_TOKEN_ID)
//!     .deadline_block(current_block + 1000)
//!     .build()?;
//!
//! // Bidder places bid
//! let place_bid = PlaceBidBuilder::new()
//!     .auction_id(auction_id)
//!     .bidder_pubkey(bidder_pubkey)
//!     .amount(1500)
//!     .bid_nonce(nonce)
//!     .escrow_id(escrow_id)
//!     .build()?;
//! ```

use darkfi_sdk::{
    crypto::{PublicKey, CONSENSUS_ID_DARKTOKEN, TOKEN_ID_DARK},
    pasta::pallas,
};

use crate::model::{
    ClaimWinningsParamsV1, CloseAuctionParamsV1, CreateAuctionParamsV1, PlaceBidParamsV1,
    RefundBidParamsV1, SettleAuctionParamsV1,
};

/// Builder for CreateAuctionV1 params
#[derive(Default)]
pub struct CreateAuctionBuilder {
    seller_pubkey: Option<PublicKey>,
    item_commitment: Option<pallas::Base>,
    reserve_price: Option<u64>,
    token_id: Option<pallas::Base>,
    deadline_block: Option<u64>,
    auction_id: Option<pallas::Base>,
    seller_commitment: Option<pallas::Base>,
}

impl CreateAuctionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seller_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.seller_pubkey = Some(pubkey);
        self
    }

    pub fn item_commitment(mut self, commitment: pallas::Base) -> Self {
        self.item_commitment = Some(commitment);
        self
    }

    pub fn reserve_price(mut self, price: u64) -> Self {
        self.reserve_price = Some(price);
        self
    }

    pub fn token_id(mut self, token_id: pallas::Base) -> Self {
        self.token_id = Some(token_id);
        self
    }

    pub fn deadline_block(mut self, block: u64) -> Self {
        self.deadline_block = Some(block);
        self
    }

    pub fn auction_id(mut self, id: pallas::Base) -> Self {
        self.auction_id = Some(id);
        self
    }

    pub fn seller_commitment(mut self, commitment: pallas::Base) -> Self {
        self.seller_commitment = Some(commitment);
        self
    }

    pub fn build(self) -> Result<CreateAuctionParamsV1, &'static str> {
        Ok(CreateAuctionParamsV1 {
            seller_pubkey: self.seller_pubkey.ok_or("seller_pubkey not set")?,
            item_commitment: self.item_commitment.ok_or("item_commitment not set")?,
            reserve_price: self.reserve_price.ok_or("reserve_price not set")?,
            token_id: self.token_id.unwrap_or(*TOKEN_ID_DARK),
            deadline_block: self.deadline_block.ok_or("deadline_block not set")?,
            auction_id: self.auction_id.ok_or("auction_id not set")?,
            seller_commitment: self.seller_commitment.ok_or("seller_commitment not set")?,
            merkle_proof: vec![],
            merkle_root: pallas::Base::zero(),
        })
    }
}

/// Builder for PlaceBidV1 params
#[derive(Default)]
pub struct PlaceBidBuilder {
    auction_id: Option<pallas::Base>,
    bidder_pubkey: Option<PublicKey>,
    amount: Option<u64>,
    bid_nonce: Option<pallas::Base>,
    bid_id: Option<pallas::Base>,
    escrow_id: Option<pallas::Base>,
    current_high_bid: Option<u64>,
}

impl PlaceBidBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn auction_id(mut self, id: pallas::Base) -> Self {
        self.auction_id = Some(id);
        self
    }

    pub fn bidder_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.bidder_pubkey = Some(pubkey);
        self
    }

    pub fn amount(mut self, amount: u64) -> Self {
        self.amount = Some(amount);
        self
    }

    pub fn bid_nonce(mut self, nonce: pallas::Base) -> Self {
        self.bid_nonce = Some(nonce);
        self
    }

    pub fn bid_id(mut self, id: pallas::Base) -> Self {
        self.bid_id = Some(id);
        self
    }

    pub fn escrow_id(mut self, id: pallas::Base) -> Self {
        self.escrow_id = Some(id);
        self
    }

    pub fn current_high_bid(mut self, bid: u64) -> Self {
        self.current_high_bid = Some(bid);
        self
    }

    pub fn build(self) -> Result<PlaceBidParamsV1, &'static str> {
        Ok(PlaceBidParamsV1 {
            auction_id: self.auction_id.ok_or("auction_id not set")?,
            bidder_pubkey: self.bidder_pubkey.ok_or("bidder_pubkey not set")?,
            amount: self.amount.ok_or("amount not set")?,
            bid_nonce: self.bid_nonce.ok_or("bid_nonce not set")?,
            bid_id: self.bid_id.ok_or("bid_id not set")?,
            escrow_id: self.escrow_id.ok_or("escrow_id not set")?,
            current_high_bid: self.current_high_bid.unwrap_or(0),
        })
    }
}

/// Builder for CloseAuctionV1 params
#[derive(Default)]
pub struct CloseAuctionBuilder {
    auction_id: Option<pallas::Base>,
    winner_bid_id: Option<pallas::Base>,
    seller_pubkey: Option<PublicKey>,
    current_block: Option<u64>,
}

impl CloseAuctionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn auction_id(mut self, id: pallas::Base) -> Self {
        self.auction_id = Some(id);
        self
    }

    pub fn winner_bid_id(mut self, id: pallas::Base) -> Self {
        self.winner_bid_id = Some(id);
        self
    }

    pub fn seller_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.seller_pubkey = Some(pubkey);
        self
    }

    pub fn current_block(mut self, block: u64) -> Self {
        self.current_block = Some(block);
        self
    }

    pub fn build(self) -> Result<CloseAuctionParamsV1, &'static str> {
        Ok(CloseAuctionParamsV1 {
            auction_id: self.auction_id.ok_or("auction_id not set")?,
            winner_bid_id: self.winner_bid_id.ok_or("winner_bid_id not set")?,
            seller_pubkey: self.seller_pubkey.ok_or("seller_pubkey not set")?,
            current_block: self.current_block.ok_or("current_block not set")?,
        })
    }
}

/// Builder for ClaimWinningsV1 params
#[derive(Default)]
pub struct ClaimWinningsBuilder {
    auction_id: Option<pallas::Base>,
    winner_bid_id: Option<pallas::Base>,
    winner_pubkey: Option<PublicKey>,
    winner_secret: Option<pallas::Base>,
}

impl ClaimWinningsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn auction_id(mut self, id: pallas::Base) -> Self {
        self.auction_id = Some(id);
        self
    }

    pub fn winner_bid_id(mut self, id: pallas::Base) -> Self {
        self.winner_bid_id = Some(id);
        self
    }

    pub fn winner_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.winner_pubkey = Some(pubkey);
        self
    }

    pub fn winner_secret(mut self, secret: pallas::Base) -> Self {
        self.winner_secret = Some(secret);
        self
    }

    pub fn build(self) -> Result<ClaimWinningsParamsV1, &'static str> {
        Ok(ClaimWinningsParamsV1 {
            auction_id: self.auction_id.ok_or("auction_id not set")?,
            winner_bid_id: self.winner_bid_id.ok_or("winner_bid_id not set")?,
            winner_pubkey: self.winner_pubkey.ok_or("winner_pubkey not set")?,
            winner_secret: self.winner_secret.ok_or("winner_secret not set")?,
        })
    }
}

/// Builder for SettleAuctionV1 params
#[derive(Default)]
pub struct SettleAuctionBuilder {
    auction_id: Option<pallas::Base>,
    seller_pubkey: Option<PublicKey>,
    highest_bid_amount: Option<u64>,
    settlement_nullifier: Option<pallas::Base>,
    seller_secret: Option<pallas::Base>,
}

impl SettleAuctionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn auction_id(mut self, id: pallas::Base) -> Self {
        self.auction_id = Some(id);
        self
    }

    pub fn seller_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.seller_pubkey = Some(pubkey);
        self
    }

    pub fn highest_bid_amount(mut self, amount: u64) -> Self {
        self.highest_bid_amount = Some(amount);
        self
    }

    pub fn settlement_nullifier(mut self, nullifier: pallas::Base) -> Self {
        self.settlement_nullifier = Some(nullifier);
        self
    }

    pub fn seller_secret(mut self, secret: pallas::Base) -> Self {
        self.seller_secret = Some(secret);
        self
    }

    pub fn build(self) -> Result<SettleAuctionParamsV1, &'static str> {
        Ok(SettleAuctionParamsV1 {
            auction_id: self.auction_id.ok_or("auction_id not set")?,
            seller_pubkey: self.seller_pubkey.ok_or("seller_pubkey not set")?,
            highest_bid_amount: self.highest_bid_amount.ok_or("highest_bid_amount not set")?,
            settlement_nullifier: self.settlement_nullifier.ok_or("settlement_nullifier not set")?,
            seller_secret: self.seller_secret.ok_or("seller_secret not set")?,
        })
    }
}

/// Builder for RefundBidV1 params
#[derive(Default)]
pub struct RefundBidBuilder {
    bid_id: Option<pallas::Base>,
    bidder_pubkey: Option<PublicKey>,
    refund_nullifier: Option<pallas::Base>,
    bidder_secret: Option<pallas::Base>,
}

impl RefundBidBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bid_id(mut self, id: pallas::Base) -> Self {
        self.bid_id = Some(id);
        self
    }

    pub fn bidder_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.bidder_pubkey = Some(pubkey);
        self
    }

    pub fn refund_nullifier(mut self, nullifier: pallas::Base) -> Self {
        self.refund_nullifier = Some(nullifier);
        self
    }

    pub fn bidder_secret(mut self, secret: pallas::Base) -> Self {
        self.bidder_secret = Some(secret);
        self
    }

    pub fn build(self) -> Result<RefundBidParamsV1, &'static str> {
        Ok(RefundBidParamsV1 {
            bid_id: self.bid_id.ok_or("bid_id not set")?,
            bidder_pubkey: self.bidder_pubkey.ok_or("bidder_pubkey not set")?,
            refund_nullifier: self.refund_nullifier.ok_or("refund_nullifier not set")?,
            bidder_secret: self.bidder_secret.ok_or("bidder_secret not set")?,
        })
    }
}