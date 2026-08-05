//! ContractTestSpec for auction. Tier: READY.
use dwow_contract_test_harness::harness::{AuctionHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn auction_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(AuctionHarness::spawn()));
    let h: &AuctionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/auction/dwow_auction_contract.wasm");
    let seller_sk = pallas::Base::from(10u64);
    let seller_pk = PublicKey::from_secret(SecretKey::from_base(seller_sk));
    let bidder_sk = pallas::Base::from(20u64);
    let bidder_pk = PublicKey::from_secret(SecretKey::from_base(bidder_sk));
    let winner_sk = pallas::Base::from(30u64);
    let winner_pk = PublicKey::from_secret(SecretKey::from_base(winner_sk));

    ContractTestSpec {
        name: "auction", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            mk_ep("CreateAuctionV1", true, Box::new(move || {
                let r = h.create_auction(seller_sk, pallas::Base::from(100u64), 1000, pallas::Base::from(1u64), 500, 0, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("PlaceBidV1", true, Box::new(move || {
                let r = h.place_bid(pallas::Base::from(1u64), bidder_sk, 1500, pallas::Base::from(1u64), 500, 10, 0, bidder_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("CloseAuctionV1", true, Box::new(move || {
                let r = h.close_auction(pallas::Base::from(1u64), pallas::Base::from(1u64), seller_sk, 500, 100, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ClaimWinningsV1", true, Box::new(move || {
                let r = h.claim_winnings(pallas::Base::from(1u64), pallas::Base::from(1u64), winner_sk, winner_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SettleAuctionV1", true, Box::new(move || {
                let r = h.settle_auction(pallas::Base::from(1u64), seller_sk, 1500, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RefundBidV1", true, Box::new(move || {
                let r = h.refund_bid(pallas::Base::from(1u64), bidder_sk, bidder_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}

