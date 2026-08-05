//! ContractTestSpec for baccarat. Spec: heavyweight-spec.md §5.9.
//! Harness: COMPLETE (4/4, real proofs). Tier: READY.

use dwow_baccarat_contract::model::BetType;
use dwow_contract_test_harness::harness::{BaccaratHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn baccarat_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BaccaratHarness::spawn()));
    let state_trees = harness.state_trees();
    let h: &BaccaratHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/baccarat/dwow_baccarat_contract.wasm");
    let player_pub = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());

    ContractTestSpec {
        name: "baccarat", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees,
        endpoints: vec![
            EndpointSpec {
                name: "CommitBetV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.commit_bet(player_pub, 1000, BetType::Player,
                        pallas::Base::from(99u64), pallas::Base::from(3u64),
                        pallas::Base::from(1u64), 200, 1)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "DrawCardsV1", is_zk: false, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.draw_cards(pallas::Base::from(1u64), pallas::Base::from(99u64),
                        pallas::Base::from(1u64), pallas::Base::zero(), pallas::Base::zero())
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "SettleBetV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.settle_bet(pallas::Base::from(1u64), pallas::Base::from(99u64),
                        player_pub, 1000, BetType::Player,
                        pallas::Base::from(1u64), pallas::Base::from(3u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "HouseCloseV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.house_close(pallas::Base::from(1u64), pallas::Base::from(10u64),
                        pallas::Base::from(3u64), pallas::Base::from(4u64),
                        pallas::Base::zero(), pallas::Base::zero())
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
