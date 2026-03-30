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

//! Auction contract integration tests

use darkfi_auction_contract::{
    model::{
        Auction, AuctionId, AuctionState, Bid, BidId, BidState, ClaimWinningsParamsV1,
        ClaimWinningsUpdateV1, CloseAuctionParamsV1, CloseAuctionUpdateV1, CreateAuctionParamsV1,
        CreateAuctionUpdateV1, PlaceBidParamsV1, PlaceBidUpdateV1, RefundBidParamsV1,
        RefundBidUpdateV1, SettleAuctionParamsV1, SettleAuctionUpdateV1,
    },
    AuctionFunction,
    // Constants
    AUCTION_CONTRACT_AUCTIONS_TREE, AUCTION_CONTRACT_BIDS_TREE,
    AUCTION_CONTRACT_NULLIFIERS_TREE, AUCTION_CONTRACT_INFO_TREE,
};

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
    assert_eq!(AuctionState::try_from(0), Ok(AuctionState::Created));
    assert_eq!(AuctionState::try_from(1), Ok(AuctionState::Active));
    assert_eq!(AuctionState::try_from(2), Ok(AuctionState::Closed));
    assert_eq!(AuctionState::try_from(3), Ok(AuctionState::Settled));
    assert!(AuctionState::try_from(4).is_err());
    assert!(AuctionState::try_from(255).is_err());
}

#[test]
fn test_bid_state_from_u8() {
    assert_eq!(BidState::try_from(0), Ok(BidState::Active));
    assert_eq!(BidState::try_from(1), Ok(BidState::Outbid));
    assert_eq!(BidState::try_from(2), Ok(BidState::Won));
    assert_eq!(BidState::try_from(3), Ok(BidState::Refunded));
    assert!(BidState::try_from(4).is_err());
    assert!(BidState::try_from(255).is_err());
}

#[test]
fn test_auction_derive_id() {
    let seller_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let item_commitment = darkfi_sdk::pasta::pallas::Base::from(1);
    let reserve_price: u64 = 1000;
    let token_id = darkfi_sdk::pasta::pallas::Base::ONE;
    let deadline_block: u64 = 100000;
    let seller_secret = darkfi_sdk::pasta::pallas::Base::from(42);

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
        item_commitment + darkfi_sdk::pasta::pallas::Base::ONE,
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
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        seller_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        item_commitment: darkfi_sdk::pasta::pallas::Base::from(2),
        reserve_price: 1000,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        deadline_block: 100000,
        state: AuctionState::Created,
        highest_bid: None,
        highest_bidder: None,
        highest_bid_id: None,
        bid_count: 0,
        created_at: 50000,
    };

    let seller_secret = darkfi_sdk::pasta::pallas::Base::from(99);
    let nullifier = auction.compute_settlement_nullifier(seller_secret);

    // Should be deterministic
    let nullifier2 = auction.compute_settlement_nullifier(seller_secret);
    assert_eq!(nullifier, nullifier2);
}

#[test]
fn test_bid_derive_id() {
    let auction_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let bidder_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let amount: u64 = 500;
    let bid_nonce = darkfi_sdk::pasta::pallas::Base::from(42);

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
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        auction_id: darkfi_sdk::pasta::pallas::Base::from(2),
        bidder_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        amount: 500,
        escrow_id: darkfi_sdk::pasta::pallas::Base::from(3),
        state: BidState::Active,
        created_at: 50000,
    };

    let bidder_secret = darkfi_sdk::pasta::pallas::Base::from(99);
    let nullifier = bid.compute_refund_nullifier(bidder_secret);

    // Should be deterministic
    let nullifier2 = bid.compute_refund_nullifier(bidder_secret);
    assert_eq!(nullifier, nullifier2);
}

#[test]
fn test_auction_encoding() {
    let auction = Auction {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        seller_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        item_commitment: darkfi_sdk::pasta::pallas::Base::from(2),
        reserve_price: 1000,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        deadline_block: 100000,
        state: AuctionState::Created,
        highest_bid: None,
        highest_bidder: None,
        highest_bid_id: None,
        bid_count: 0,
        created_at: 50000,
    };

    let encoded = auction.encode().unwrap();
    let decoded = Auction::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, auction.id);
    assert_eq!(decoded.reserve_price, auction.reserve_price);
    assert_eq!(decoded.state, auction.state);
    assert_eq!(decoded.bid_count, auction.bid_count);
}

#[test]
fn test_bid_encoding() {
    let bid = Bid {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        auction_id: darkfi_sdk::pasta::pallas::Base::from(2),
        bidder_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        amount: 500,
        escrow_id: darkfi_sdk::pasta::pallas::Base::from(3),
        state: BidState::Active,
        created_at: 50000,
    };

    let encoded = bid.encode().unwrap();
    let decoded = Bid::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, bid.id);
    assert_eq!(decoded.amount, bid.amount);
    assert_eq!(decoded.state, bid.state);
    assert_eq!(decoded.created_at, bid.created_at);
}

#[test]
fn test_create_auction_params_encoding() {
    let params = CreateAuctionParamsV1 {
        seller_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        item_commitment: darkfi_sdk::pasta::pallas::Base::from(1),
        reserve_price: 1000,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        deadline_block: 100000,
        auction_id: darkfi_sdk::pasta::pallas::Base::from(2),
        seller_commitment: darkfi_sdk::pasta::pallas::Base::from(3),
        merkle_proof: vec![darkfi_sdk::pasta::pallas::Base::from(4)],
        merkle_root: darkfi_sdk::pasta::pallas::Base::from(5),
    };

    let encoded = params.encode().unwrap();
    let decoded = CreateAuctionParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.reserve_price, params.reserve_price);
    assert_eq!(decoded.deadline_block, params.deadline_block);
}

#[test]
fn test_create_auction_update_encoding() {
    let update = CreateAuctionUpdateV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = CreateAuctionUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
}

#[test]
fn test_place_bid_params_encoding() {
    let params = PlaceBidParamsV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        bidder_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        amount: 500,
        bid_nonce: darkfi_sdk::pasta::pallas::Base::from(2),
        bid_id: darkfi_sdk::pasta::pallas::Base::from(3),
        escrow_id: darkfi_sdk::pasta::pallas::Base::from(4),
        current_high_bid: 400,
    };

    let encoded = params.encode().unwrap();
    let decoded = PlaceBidParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.amount, params.amount);
    assert_eq!(decoded.current_high_bid, params.current_high_bid);
}

#[test]
fn test_place_bid_update_encoding() {
    let update = PlaceBidUpdateV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        highest_bid: 500,
        highest_bidder: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        highest_bid_id: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = PlaceBidUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.highest_bid, update.highest_bid);
    assert_eq!(decoded.highest_bid_id, update.highest_bid_id);
}

#[test]
fn test_close_auction_params_encoding() {
    let params = CloseAuctionParamsV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        winner_bid_id: darkfi_sdk::pasta::pallas::Base::from(2),
        seller_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        current_block: 100500,
    };

    let encoded = params.encode().unwrap();
    let decoded = CloseAuctionParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.auction_id, params.auction_id);
    assert_eq!(decoded.current_block, params.current_block);
}

#[test]
fn test_close_auction_update_encoding() {
    let update = CloseAuctionUpdateV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        winner_bid_id: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = CloseAuctionUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
    assert_eq!(decoded.winner_bid_id, update.winner_bid_id);
}

#[test]
fn test_claim_winnings_params_encoding() {
    let params = ClaimWinningsParamsV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        winner_bid_id: darkfi_sdk::pasta::pallas::Base::from(2),
        winner_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        winner_secret: darkfi_sdk::pasta::pallas::Base::from(3),
    };

    let encoded = params.encode().unwrap();
    let decoded = ClaimWinningsParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.auction_id, params.auction_id);
    assert_eq!(decoded.winner_bid_id, params.winner_bid_id);
}

#[test]
fn test_claim_winnings_update_encoding() {
    let update = ClaimWinningsUpdateV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        winner_bid_id: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = ClaimWinningsUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
    assert_eq!(decoded.winner_bid_id, update.winner_bid_id);
}

#[test]
fn test_settle_auction_params_encoding() {
    let params = SettleAuctionParamsV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        seller_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        highest_bid_amount: 500,
        settlement_nullifier: darkfi_sdk::pasta::pallas::Base::from(2),
        seller_secret: darkfi_sdk::pasta::pallas::Base::from(3),
    };

    let encoded = params.encode().unwrap();
    let decoded = SettleAuctionParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.auction_id, params.auction_id);
    assert_eq!(decoded.highest_bid_amount, params.highest_bid_amount);
}

#[test]
fn test_settle_auction_update_encoding() {
    let update = SettleAuctionUpdateV1 {
        auction_id: darkfi_sdk::pasta::pallas::Base::from(1),
        settlement_nullifier: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = SettleAuctionUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.auction_id, update.auction_id);
    assert_eq!(decoded.settlement_nullifier, update.settlement_nullifier);
}

#[test]
fn test_refund_bid_params_encoding() {
    let params = RefundBidParamsV1 {
        bid_id: darkfi_sdk::pasta::pallas::Base::from(1),
        bidder_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        refund_nullifier: darkfi_sdk::pasta::pallas::Base::from(2),
        bidder_secret: darkfi_sdk::pasta::pallas::Base::from(3),
    };

    let encoded = params.encode().unwrap();
    let decoded = RefundBidParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.bid_id, params.bid_id);
    assert_eq!(decoded.refund_nullifier, params.refund_nullifier);
}

#[test]
fn test_refund_bid_update_encoding() {
    let update = RefundBidUpdateV1 {
        bid_id: darkfi_sdk::pasta::pallas::Base::from(1),
        refund_nullifier: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = RefundBidUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

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