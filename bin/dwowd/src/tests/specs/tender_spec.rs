//! ContractTestSpec for tender. Tier: HARVESTABLE.
use dwow_contract_test_harness::harness::{ContractHarness, TenderHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey}; use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn tender_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(TenderHarness::spawn()));
    let h: &TenderHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/tender/dwow_tender_contract.wasm");
    let r_sk = pallas::Base::from(10u64); let r_pk = PublicKey::from_secret(SecretKey::from_base(r_sk));
    let b_sk = pallas::Base::from(20u64); let b_pk = PublicKey::from_secret(SecretKey::from_base(b_sk));
    ContractTestSpec { name: "tender", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            mk_ep("CreateTenderV1", true, Box::new(move || {
                let r = h.create_tender(r_pk, r_sk, "Test Tender".to_string(), pallas::Base::from(1u64), pallas::Base::from(2u64), 100, 10000, 500, 1000, 2000)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SubmitBidV1", true, Box::new(move || {
                let r = h.submit_bid(pallas::Base::from(1u64), b_pk, b_sk, 5000, pallas::Base::from(3u64), pallas::Base::from(4u64), b"encrypted".to_vec())
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RevealBidV1", true, Box::new(move || {
                let r = h.reveal_bid(pallas::Base::from(1u64), pallas::Base::from(1u64), b_pk, b_sk, 5000)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SelectWinnerV1", true, Box::new(move || {
                let r = h.select_winner(pallas::Base::from(1u64), pallas::Base::from(1u64), r_pk, r_sk, b_pk, 5000)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
