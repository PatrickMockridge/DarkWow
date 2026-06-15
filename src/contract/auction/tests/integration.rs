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

//! Auction contract integration tests

use dwow_auction_contract::{
    model::{
        Auction, AuctionState, Bid, BidState, ClaimWinningsParamsV1, ClaimWinningsUpdateV1,
        CloseAuctionParamsV1, CloseAuctionUpdateV1, CreateAuctionParamsV1, CreateAuctionUpdateV1,
        PlaceBidParamsV1, PlaceBidUpdateV1, RefundBidParamsV1, RefundBidUpdateV1,
        SettleAuctionParamsV1, SettleAuctionUpdateV1,
    },
    AuctionFunction,
    // Constants
    AUCTION_CONTRACT_AUCTIONS_TREE, AUCTION_CONTRACT_BIDS_TREE,
    AUCTION_CONTRACT_NULLIFIERS_TREE, AUCTION_CONTRACT_INFO_TREE,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{pasta_prelude::Group, PublicKey, SecretKey},
    pasta::pallas,
};

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_auction_function_enum_valid() {
    assert!(AuctionFunction::try_from(0x00).is_ok()); // CreateAuctionV1
    assert!(AuctionFunction::try_from(0x01).is_ok()); // PlaceBidV1
    assert!(AuctionFunction::try_from(0x02).is_ok()); // CloseAuctionV1
    assert!(AuctionFunction::try_from(0x03).is_ok()); // ClaimWinningsV1
    assert!(AuctionFunction::try_from(0x04).is_ok()); // SettleAuctionV1
    assert!(AuctionFunction::try_from(0x05).is_ok()); // RefundBidV1
}

#[test]
fn test_auction_function_enum_invalid() {
    assert!(AuctionFunction::try_from(0xFF).is_err());
    assert!(AuctionFunction::try_from(0x06).is_err());
    assert!(AuctionFunction::try_from(0x10).is_err());
}

#[test]
fn test_auction_state_from_u8() {
    assert!(matches!(AuctionState::try_from(0), Ok(AuctionState::Created)));
    assert!(matches!(AuctionState::try_from(1), Ok(AuctionState::Active)));
    assert!(matches!(AuctionState::try_from(2), Ok(AuctionState::Closed)));
    assert!(matches!(AuctionState::try_from(3), Ok(AuctionState::Settled)));
    assert!(AuctionState::try_from(4).is_err());
    assert!(AuctionState::try_from(255).is_err());
}

#[test]
fn test_bid_state_from_u8() {
    assert!(matches!(BidState::try_from(0), Ok(BidState::Active)));
    assert!(matches!(BidState::try_from(1), Ok(BidState::Outbid)));
    assert!(matches!(BidState::try_from(2), Ok(BidState::Won)));
    assert!(matches!(BidState::try_from(3), Ok(BidState::Refunded)));
    assert!(BidState::try_from(4).is_err());
    assert!(BidState::try_from(255).is_err());
}

#[test]
fn test_auction_derive_id() {
    let seller_pubkey = make_pubkey(1);
    let item_commitment = pallas::Base::from(1);
    let reserve_price: u64 = 1000;
    let token_id = pallas::Base::one();
    let deadline_block: u64 = 100000;
    let seller_secret = pallas::Base::from(42);

    let id = Auction::derive_id(
        &seller_pubkey,
        item_commitment,
        reserve_price,
        token_id,
        deadline_block,
        seller_secret,
    );

    // Should be deterministic
    let id2 = Auction::derive_id(
        &seller_pubkey,
        item_commitment,
        reserve_price,
        token_id,
        deadline_block,
        seller_secret,
    );
    assert_eq!(id, id2);

    // Different input should produce different ID
    let id_different = Auction::derive_id(
        &seller_pubkey,
        item_commitment + pallas::Base::one(),
        reserve_price,
        token_id,
        deadline_block,
        seller_secret,
    );
    assert_ne!(id, id_different);
}

#[test]
fn test_auction_compute_settlement_nullifier() {
    let auction = Auction {

        version: 0,        id: pallas::Base::from(1),
        seller_pubkey: make_pubkey(1),
        item_commitment: pallas::Base::from(2),
        reserve_price: 1000,
        token_id: pallas::Base::one(),
        deadline_block: 100000,
        state: AuctionState::Created,
        highest_bid: None,
        highest_bidder: None,
        highest_bid_id: None,
        bid_count: 0,
        created_at: 50000,
        instance_seed: [0u8; 32],
    };

    let seller_secret = pallas::Base::from(99);
    let nullifier = auction.compute_settlement_nullifier(seller_secret);

    // Should be deterministic
    let nullifier2 = auction.compute_settlement_nullifier(seller_secret);
    assert_eq!(nullifier, nullifier2);
}

#[test]
fn test_bid_derive_id() {
    let auction_id = pallas::Base::from(1);
    let bidder_pubkey = make_pubkey(1);
    let amount: u64 = 500;
    let bid_nonce = pallas::Base::from(42);

    let id = Bid::derive_id(auction_id, &bidder_pubkey, amount, bid_nonce);

    // Should be deterministic
    let id2 = Bid::derive_id(auction_id, &bidder_pubkey, amount, bid_nonce);
    assert_eq!(id, id2);

    // Different input should produce different ID
    let id_different = Bid::derive_id(auction_id, &bidder_pubkey, amount + 1, bid_nonce);
    assert_ne!(id, id_different);
}

#[test]
fn test_bid_compute_refund_nullifier() {
    let bid = Bid {

        version: 0,        id: pallas::Base::from(1),
        auction_id: pallas::Base::from(2),
        bidder_pubkey: make_pubkey(1),
        amount: 500,
        escrow_id: pallas::Base::from(3),
        state: BidState::Active,
        created_at: 50000,
        instance_seed: [0u8; 32],
    };

    let bidder_secret = pallas::Base::from(99);
    let nullifier = bid.compute_refund_nullifier(bidder_secret);

    // Should be deterministic
    let nullifier2 = bid.compute_refund_nullifier(bidder_secret);
    assert_eq!(nullifier, nullifier2);
}

#[test]
fn test_auction_encoding() {
    let auction = Auction {

        version: 0,        id: pallas::Base::from(1),
        seller_pubkey: make_pubkey(1),
        item_commitment: pallas::Base::from(2),
        reserve_price: 1000,
        token_id: pallas::Base::one(),
        deadline_block: 100000,
        state: AuctionState::Created,
        highest_bid: None,
        highest_bidder: None,
        highest_bid_id: None,
        bid_count: 0,
        created_at: 50000,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&auction);
    let decoded: Auction = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, auction.id);
    assert_eq!(decoded.reserve_price, auction.reserve_price);
    assert_eq!(decoded.state, auction.state);
    assert_eq!(decoded.bid_count, auction.bid_count);
}

#[test]
fn test_bid_encoding() {
    let bid = Bid {

        version: 0,        id: pallas::Base::from(1),
        auction_id: pallas::Base::from(2),
        bidder_pubkey: make_pubkey(1),
        amount: 500,
        escrow_id: pallas::Base::from(3),
        state: BidState::Active,
        created_at: 50000,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&bid);
    let decoded: Bid = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, bid.id);
    assert_eq!(decoded.amount, bid.amount);
    assert_eq!(decoded.state, bid.state);
    assert_eq!(decoded.created_at, bid.created_at);
}

#[test]
fn test_create_auction_params_encoding() {
    let params = CreateAuctionParamsV1 {
        seller_pubkey: make_pubkey(1),
        item_commitment: pallas::Base::from(1),
        reserve_price: 1000,
        token_id: pallas::Base::one(),
        deadline_block: 100000,
        auction_id: pallas::Base::from(2),
        seller_commitment: pallas::Base::from(3),
        merkle_proof: vec![pallas::Base::from(4)],
        merkle_root: pallas::Base::from(5),
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: CreateAuctionParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.reserve_price, params.reserve_price);
    assert_eq!(decoded.deadline_block, params.deadline_block);
}

#[test]
fn test_create_auction_update_encoding() {
    let update = CreateAuctionUpdateV1 { auction_id: pallas::Base::from(1) };

    let encoded = serialize(&update);
    let decoded: CreateAuctionUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
}

#[test]
fn test_place_bid_params_encoding() {
    let params = PlaceBidParamsV1 {
        auction_id: pallas::Base::from(1),
        bidder_pubkey: make_pubkey(1),
        amount: 500,
        bid_nonce: pallas::Base::from(2),
        bid_id: pallas::Base::from(3),
        escrow_id: pallas::Base::from(4),
        current_high_bid: 400,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: PlaceBidParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.amount, params.amount);
    assert_eq!(decoded.current_high_bid, params.current_high_bid);
}

#[test]
fn test_place_bid_update_encoding() {
    let update = PlaceBidUpdateV1 {
        auction_id: pallas::Base::from(1),
        highest_bid: 500,
        highest_bidder: make_pubkey(2),
        highest_bid_id: pallas::Base::from(2),
    };

    let encoded = serialize(&update);
    let decoded: PlaceBidUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.highest_bid, update.highest_bid);
    assert_eq!(decoded.highest_bid_id, update.highest_bid_id);
}

#[test]
fn test_close_auction_params_encoding() {
    let params = CloseAuctionParamsV1 {
        auction_id: pallas::Base::from(1),
        winner_bid_id: pallas::Base::from(2),
        seller_pubkey: make_pubkey(1),
        current_block: 100500,
    };

    let encoded = serialize(&params);
    let decoded: CloseAuctionParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.auction_id, params.auction_id);
    assert_eq!(decoded.current_block, params.current_block);
}

#[test]
fn test_close_auction_update_encoding() {
    let update = CloseAuctionUpdateV1 {
        auction_id: pallas::Base::from(1),
        winner_bid_id: pallas::Base::from(2),
    };

    let encoded = serialize(&update);
    let decoded: CloseAuctionUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
    assert_eq!(decoded.winner_bid_id, update.winner_bid_id);
}

#[test]
fn test_claim_winnings_params_encoding() {
    let params = ClaimWinningsParamsV1 {
        auction_id: pallas::Base::from(1),
        winner_bid_id: pallas::Base::from(2),
        winner_pubkey: make_pubkey(1),
        winner_secret: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: ClaimWinningsParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.auction_id, params.auction_id);
    assert_eq!(decoded.winner_bid_id, params.winner_bid_id);
}

#[test]
fn test_claim_winnings_update_encoding() {
    let update = ClaimWinningsUpdateV1 {
        auction_id: pallas::Base::from(1),
        winner_bid_id: pallas::Base::from(2),
    };

    let encoded = serialize(&update);
    let decoded: ClaimWinningsUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
    assert_eq!(decoded.winner_bid_id, update.winner_bid_id);
}

#[test]
fn test_settle_auction_params_encoding() {
    let params = SettleAuctionParamsV1 {
        auction_id: pallas::Base::from(1),
        seller_pubkey: make_pubkey(1),
        highest_bid_amount: 500,
        settlement_nullifier: pallas::Base::from(2),
        seller_secret: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: SettleAuctionParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.auction_id, params.auction_id);
    assert_eq!(decoded.highest_bid_amount, params.highest_bid_amount);
}

#[test]
fn test_settle_auction_update_encoding() {
    let update = SettleAuctionUpdateV1 {
        auction_id: pallas::Base::from(1),
        settlement_nullifier: pallas::Base::from(2),
    };

    let encoded = serialize(&update);
    let decoded: SettleAuctionUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
    assert_eq!(decoded.settlement_nullifier, update.settlement_nullifier);
}

#[test]
fn test_refund_bid_params_encoding() {
    let params = RefundBidParamsV1 {
        bid_id: pallas::Base::from(1),
        bidder_pubkey: make_pubkey(1),
        refund_nullifier: pallas::Base::from(2),
        bidder_secret: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: RefundBidParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bid_id, params.bid_id);
    assert_eq!(decoded.refund_nullifier, params.refund_nullifier);
}

#[test]
fn test_refund_bid_update_encoding() {
    let update = RefundBidUpdateV1 {
        bid_id: pallas::Base::from(1),
        refund_nullifier: pallas::Base::from(2),
    };

    let encoded = serialize(&update);
    let decoded: RefundBidUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bid_id, update.bid_id);
    assert_eq!(decoded.refund_nullifier, update.refund_nullifier);
}

#[test]
fn test_constants() {
    assert_eq!(AUCTION_CONTRACT_AUCTIONS_TREE, "auctions");
    assert_eq!(AUCTION_CONTRACT_BIDS_TREE, "bids");
    assert_eq!(AUCTION_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(AUCTION_CONTRACT_INFO_TREE, "info");
}