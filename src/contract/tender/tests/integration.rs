
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

//! Tender contract integration tests

/// A placeholder encoded record. The update structs carry the records exec built, so these
/// round-trip tests must supply bytes — they exercise the bridge codec, not the records.
fn rec(seed: u8) -> Vec<u8> {
    vec![seed; 40]
}

use dwow_serial::{deserialize, serialize};
use dwow_sdk::pasta::pallas;
use dwow_tender_contract::{
    model::{
        Bid, BidState, CancelTenderParamsV1, CancelTenderUpdateV1, CloseTenderParamsV1,
        CloseTenderUpdateV1, CreateTenderParamsV1, CreateTenderUpdateV1, RejectBidParamsV1,
        RejectBidUpdateV1, RevealBidParamsV1, RevealBidUpdateV1, SelectWinnerParamsV1,
        SelectWinnerUpdateV1, SubmitBidParamsV1, SubmitBidUpdateV1, Tender, TenderId, TenderState,
    },
    TenderFunction,
    // Constants
    TENDER_CONTRACT_TENDERS_TREE, TENDER_CONTRACT_BIDS_TREE,
    TENDER_CONTRACT_NULLIFIERS_TREE, TENDER_CONTRACT_INFO_TREE,
    TENDER_CONTRACT_ZKAS_CREATE_NS_V1, TENDER_CONTRACT_ZKAS_SUBMIT_BID_NS_V1,
    TENDER_CONTRACT_ZKAS_REVEAL_BID_NS_V1, TENDER_CONTRACT_ZKAS_SELECT_WINNER_NS_V1,
};

#[test]
fn test_tender_function_enum_valid() {
    assert!(TenderFunction::try_from(0x00).is_ok()); // CreateTenderV1
    assert!(TenderFunction::try_from(0x01).is_ok()); // SubmitBidV1
    assert!(TenderFunction::try_from(0x02).is_ok()); // RevealBidV1
    assert!(TenderFunction::try_from(0x03).is_ok()); // CloseTenderV1
    assert!(TenderFunction::try_from(0x04).is_ok()); // SelectWinnerV1
    assert!(TenderFunction::try_from(0x05).is_ok()); // CancelTenderV1
    assert!(TenderFunction::try_from(0x06).is_ok()); // RejectBidV1
}

#[test]
fn test_tender_function_enum_invalid() {
    assert!(TenderFunction::try_from(0xFF).is_err());
    assert!(TenderFunction::try_from(0x09).is_err());
    assert!(TenderFunction::try_from(0x10).is_err());
}

#[test]
fn test_tender_state_from_u8() {
    assert_eq!(TenderState::try_from(0).unwrap(), TenderState::Created);
    assert_eq!(TenderState::try_from(1).unwrap(), TenderState::Bidding);
    assert_eq!(TenderState::try_from(2).unwrap(), TenderState::Revealed);
    assert_eq!(TenderState::try_from(3).unwrap(), TenderState::Awarded);
    assert_eq!(TenderState::try_from(4).unwrap(), TenderState::Cancelled);
    assert!(TenderState::try_from(5).is_err());
    assert!(TenderState::try_from(255).is_err());
}

#[test]
fn test_bid_state_from_u8() {
    assert_eq!(BidState::try_from(0).unwrap(), BidState::Sealed);
    assert_eq!(BidState::try_from(1).unwrap(), BidState::Revealed);
    assert_eq!(BidState::try_from(2).unwrap(), BidState::Accepted);
    assert_eq!(BidState::try_from(3).unwrap(), BidState::Rejected);
    assert_eq!(BidState::try_from(4).unwrap(), BidState::Expired);
    assert!(BidState::try_from(5).is_err());
    assert!(BidState::try_from(255).is_err());
}

#[test]
fn test_tender_derive_id() {
    let requester_pub_x = pallas::Base::from(1);
    let requester_pub_y = pallas::Base::from(2);
    let title = "Build Web App";
    let specification = pallas::Base::from(1);
    let attestation_id = pallas::Base::from(2);
    let min_bid: u64 = 1000;
    let max_bid: u64 = 10000;
    let bid_deadline: u64 = 100000;
    let reveal_deadline: u64 = 110000;
    let delivery_deadline: u64 = 200000;
    let requester_secret = pallas::Base::from(42);

    let id = Tender::derive_id(
        requester_pub_x,
        requester_pub_y,
        title,
        specification,
        attestation_id,
        min_bid,
        max_bid,
        bid_deadline,
        reveal_deadline,
        delivery_deadline,
        requester_secret,
    );

    // Should be deterministic
    let id2 = Tender::derive_id(
        requester_pub_x,
        requester_pub_y,
        title,
        specification,
        attestation_id,
        min_bid,
        max_bid,
        bid_deadline,
        reveal_deadline,
        delivery_deadline,
        requester_secret,
    );
    assert_eq!(id, id2);
}

#[test]
fn test_bid_derive_id() {
    let tender_id = pallas::Base::from(1);
    let bidder_pub_x = pallas::Base::from(3);
    let bidder_pub_y = pallas::Base::from(4);
    let amount: u64 = 5000;
    let bid_nonce = pallas::Base::from(42);

    let id = Bid::derive_id(tender_id, bidder_pub_x, bidder_pub_y, amount, bid_nonce);

    // Should be deterministic
    let id2 = Bid::derive_id(tender_id, bidder_pub_x, bidder_pub_y, amount, bid_nonce);
    assert_eq!(id, id2);
}

#[test]
fn test_tender_encoding() {
    let tender = Tender {

        version: 0,        id: pallas::Base::from(1),
        requester_pub_x: pallas::Base::from(2),
        requester_pub_y: pallas::Base::from(3),
        title: "Build Web App".to_string(),
        specification: pallas::Base::from(1),
        attestation_id: pallas::Base::from(2),
        min_bid: 1000,
        max_bid: 10000,
        bid_deadline: 100000,
        reveal_deadline: 110000,
        delivery_deadline: 200000,
        state: TenderState::Created,
        selected_bid_id: None,
        bid_count: 0,
        created_at: 50000,
        required_capability: None,
        required_dag_id: None,
    };

    let encoded = tender.encode().unwrap();
    let decoded = Tender::decode(&encoded).unwrap();

    assert_eq!(decoded.id, tender.id);
    assert_eq!(decoded.title, tender.title);
    assert_eq!(decoded.min_bid, tender.min_bid);
    assert_eq!(decoded.max_bid, tender.max_bid);
    assert_eq!(decoded.state, tender.state);
}

#[test]
fn test_bid_encoding() {
    let bid = Bid {

        version: 0,        id: pallas::Base::from(1),
        tender_id: pallas::Base::from(2),
        bidder_pub_x: pallas::Base::from(3),
        bidder_pub_y: pallas::Base::from(4),
        amount: 5000,
        claim_id: pallas::Base::from(3),
        encrypted_payload: vec![1, 2, 3, 4],
        state: BidState::Sealed,
        revealed_amount: None,
        created_at: 50000,
    };

    let encoded = bid.encode().unwrap();
    let decoded = Bid::decode(&encoded).unwrap();

    assert_eq!(decoded.id, bid.id);
    assert_eq!(decoded.tender_id, bid.tender_id);
    assert_eq!(decoded.amount, bid.amount);
    assert_eq!(decoded.state, bid.state);
}

#[test]
fn test_create_tender_params_encoding() {
    let params = CreateTenderParamsV1 {
        proof: vec![1, 2, 3],
        tender_id: pallas::Base::from(1),
        requester_pub_x: pallas::Base::from(2),
        requester_pub_y: pallas::Base::from(3),
        title: "Build Web App".to_string(),
        specification: pallas::Base::from(1),
        attestation_id: pallas::Base::from(4),
        min_bid: 1000,
        max_bid: 10000,
        bid_deadline: 100000,
        reveal_deadline: 110000,
        delivery_deadline: 200000,
        // Distinctive values, so the round-trip below exercises the pair appended for
        // `OBL-C78` rather than passing on zeros.
        tx_binding: pallas::Base::from(9000),
        tx_nonce: pallas::Base::from(9001),
    };

    let encoded = serialize(&params);
    let decoded: CreateTenderParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, params.tender_id);
    assert_eq!(decoded.title, params.title);
    assert_eq!(decoded.min_bid, params.min_bid);
    assert_eq!(decoded.max_bid, params.max_bid);
}

/// A tender proof is kilobyte-scale and a title can exceed 255 bytes, so **both** prefixes in
/// `CreateTenderParamsV1` truncate at the sizes below. The test above uses a 3-byte proof and an
/// 13-byte title, which is why it could not see either.
#[test]
fn test_create_tender_params_prefixes_are_not_bytes() {
    let params = CreateTenderParamsV1 {
        proof: vec![0xA5; 700],
        tender_id: pallas::Base::from(21),
        requester_pub_x: pallas::Base::from(22),
        requester_pub_y: pallas::Base::from(23),
        title: "t".repeat(300),
        specification: pallas::Base::from(24),
        attestation_id: pallas::Base::from(25),
        min_bid: 111,
        max_bid: 222,
        bid_deadline: 333,
        reveal_deadline: 444,
        delivery_deadline: 555,
        // Distinctive values, so the round-trip below exercises the pair appended for
        // `OBL-C78` rather than passing on zeros.
        tx_binding: pallas::Base::from(9001),
        tx_nonce: pallas::Base::from(9002),
    };

    let encoded = params.encode().unwrap();
    assert_eq!(encoded.len(), 4 + 700 + 32 + 32 + 32 + 4 + 300 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 64);

    let decoded = CreateTenderParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.proof.len(), 700);
    assert_eq!(decoded.title.len(), 300);
    assert_eq!(decoded.tender_id, params.tender_id);
    assert_eq!(decoded.min_bid, 111);
    assert_eq!(decoded.delivery_deadline, 555);
}

/// `SubmitBidParamsV1` carries a proof and an encrypted payload, both with their own prefix, and the
/// decoder's exact-consumption check is what a wrong width would trip.
#[test]
fn test_submit_bid_params_prefixes_are_not_bytes() {
    let params = SubmitBidParamsV1 {
        proof: vec![0xB6; 700],
        tender_id: pallas::Base::from(31),
        bid_id: pallas::Base::from(32),
        bidder_pub_x: pallas::Base::from(33),
        bidder_pub_y: pallas::Base::from(34),
        amount: 5150,
        claim_id: pallas::Base::from(35),
        encrypted_payload: vec![0xC7; 300],
        // Distinctive values, so the round-trip below exercises the pair appended for
        // `OBL-C78` rather than passing on zeros.
        tx_binding: pallas::Base::from(9002),
        tx_nonce: pallas::Base::from(9003),
    };

    let encoded = params.encode().unwrap();
    assert_eq!(encoded.len(), 4 + 700 + 168 + 4 + 300 + 64);

    let decoded = SubmitBidParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.proof.len(), 700);
    assert_eq!(decoded.encrypted_payload.len(), 300);
    assert_eq!(decoded.amount, 5150);
    assert_eq!(decoded.claim_id, params.claim_id);
}

#[test]
fn test_create_tender_update_encoding() {
    let update = CreateTenderUpdateV1 {
        tender_id: pallas::Base::from(1),
        tender_bytes: rec(7),
    };

    let encoded = serialize(&update);
    let decoded: CreateTenderUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, update.tender_id);
}

#[test]
fn test_submit_bid_params_encoding() {
    let params = SubmitBidParamsV1 {
        proof: vec![1, 2, 3],
        tender_id: pallas::Base::from(1),
        bid_id: pallas::Base::from(2),
        bidder_pub_x: pallas::Base::from(3),
        bidder_pub_y: pallas::Base::from(4),
        amount: 5000,
        claim_id: pallas::Base::from(5),
        encrypted_payload: vec![1, 2, 3, 4],
        // Distinctive values, so the round-trip below exercises the pair appended for
        // `OBL-C78` rather than passing on zeros.
        tx_binding: pallas::Base::from(9003),
        tx_nonce: pallas::Base::from(9004),
    };

    let encoded = serialize(&params);
    let decoded: SubmitBidParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, params.tender_id);
    assert_eq!(decoded.bid_id, params.bid_id);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_submit_bid_update_encoding() {
    let update = SubmitBidUpdateV1 {
        tender_id: pallas::Base::from(1),
        bid_id: pallas::Base::from(2),
        bid_bytes: rec(8),
        tender_bytes: rec(7),
    };

    let encoded = serialize(&update);
    let decoded: SubmitBidUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, update.tender_id);
    assert_eq!(decoded.bid_id, update.bid_id);
}

#[test]
fn test_reveal_bid_params_encoding() {
    let params = RevealBidParamsV1 {
        proof: vec![1, 2, 3],
        tender_id: pallas::Base::from(1),
        bid_id: pallas::Base::from(2),
        revealed_amount: 5000,
        // Distinctive values, so the round-trip below exercises the pair appended for
        // `OBL-C78` rather than passing on zeros.
        tx_binding: pallas::Base::from(9004),
        tx_nonce: pallas::Base::from(9005),
    };

    let encoded = serialize(&params);
    let decoded: RevealBidParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, params.tender_id);
    assert_eq!(decoded.bid_id, params.bid_id);
    assert_eq!(decoded.revealed_amount, params.revealed_amount);
}

#[test]
fn test_reveal_bid_update_encoding() {
    let update = RevealBidUpdateV1 {
        tender_id: pallas::Base::from(1),
        bid_id: pallas::Base::from(2),
        bid_bytes: rec(8),
        reveal_nullifier: pallas::Base::from(9),
    };

    let encoded = serialize(&update);
    let decoded: RevealBidUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, update.tender_id);
    assert_eq!(decoded.bid_id, update.bid_id);
}

#[test]
fn test_close_tender_params_encoding() {
    let params = CloseTenderParamsV1 {
        tender_id: pallas::Base::from(1),
        requester_pub_x: pallas::Base::from(2),
        requester_pub_y: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: CloseTenderParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, params.tender_id);
}

#[test]
fn test_close_tender_update_encoding() {
    let update = CloseTenderUpdateV1 {
        tender_id: pallas::Base::from(1),
        tender_bytes: rec(7),
    };

    let encoded = serialize(&update);
    let decoded: CloseTenderUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, update.tender_id);
}

#[test]
fn test_select_winner_params_encoding() {
    let params = SelectWinnerParamsV1 {
        proof: vec![1, 2, 3],
        tender_id: pallas::Base::from(1),
        winner_bid_id: pallas::Base::from(2),
        requester_pub_x: pallas::Base::from(5),
        requester_pub_y: pallas::Base::from(6),
        winner_pub_x: pallas::Base::from(3),
        winner_pub_y: pallas::Base::from(4),
        winning_amount: 5000,
        // Distinctive values, so the round-trip below exercises the pair appended for
        // `OBL-C78` rather than passing on zeros.
        tx_binding: pallas::Base::from(9006),
        tx_nonce: pallas::Base::from(9007),
    };

    let encoded = serialize(&params);
    let decoded: SelectWinnerParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, params.tender_id);
    assert_eq!(decoded.winner_bid_id, params.winner_bid_id);
    assert_eq!(decoded.winning_amount, params.winning_amount);
}

#[test]
fn test_select_winner_update_encoding() {
    let update = SelectWinnerUpdateV1 {
        tender_id: pallas::Base::from(1),
        winner_bid_id: pallas::Base::from(2),
        labor_job_id: Some(pallas::Base::from(3)),
        tender_bytes: rec(7),
        winner_bid_bytes: rec(8),
    };

    let encoded = serialize(&update);
    let decoded: SelectWinnerUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, update.tender_id);
    assert_eq!(decoded.winner_bid_id, update.winner_bid_id);
    assert_eq!(decoded.labor_job_id, update.labor_job_id);
}

#[test]
fn test_cancel_tender_params_encoding() {
    let params = CancelTenderParamsV1 {
        tender_id: pallas::Base::from(1),
        requester_pub_x: pallas::Base::from(2),
        requester_pub_y: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: CancelTenderParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, params.tender_id);
}

#[test]
fn test_cancel_tender_update_encoding() {
    let update = CancelTenderUpdateV1 {
        tender_id: pallas::Base::from(1),
        tender_bytes: rec(7),
    };

    let encoded = serialize(&update);
    let decoded: CancelTenderUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, update.tender_id);
}

#[test]
fn test_reject_bid_params_encoding() {
    let params = RejectBidParamsV1 {
        tender_id: pallas::Base::from(1),
        bid_id: pallas::Base::from(2),
        requester_pub_x: pallas::Base::from(3),
        requester_pub_y: pallas::Base::from(4),
    };

    let encoded = serialize(&params);
    let decoded: RejectBidParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, params.tender_id);
    assert_eq!(decoded.bid_id, params.bid_id);
}

#[test]
fn test_reject_bid_update_encoding() {
    let update = RejectBidUpdateV1 {
        tender_id: pallas::Base::from(1),
        bid_id: pallas::Base::from(2),
        bid_bytes: rec(8),
    };

    let encoded = serialize(&update);
    let decoded: RejectBidUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.tender_id, update.tender_id);
    assert_eq!(decoded.bid_id, update.bid_id);
}

#[test]
fn test_constants() {
    assert_eq!(TENDER_CONTRACT_TENDERS_TREE, "tenders");
    assert_eq!(TENDER_CONTRACT_BIDS_TREE, "bids");
    assert_eq!(TENDER_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(TENDER_CONTRACT_INFO_TREE, "info");
    assert_eq!(TENDER_CONTRACT_ZKAS_CREATE_NS_V1, "CreateTender");
    assert_eq!(TENDER_CONTRACT_ZKAS_SUBMIT_BID_NS_V1, "SubmitBid");
    assert_eq!(TENDER_CONTRACT_ZKAS_REVEAL_BID_NS_V1, "RevealBid");
    assert_eq!(TENDER_CONTRACT_ZKAS_SELECT_WINNER_NS_V1, "SelectWinner");
}