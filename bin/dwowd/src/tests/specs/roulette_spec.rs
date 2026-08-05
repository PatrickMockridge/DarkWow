//! ContractTestSpec for roulette. Spec: heavyweight-spec.md §5.9.
//! Harness: COMPLETE (4/4, real proofs). Tier: READY.

use dwow_contract_test_harness::harness::{ContractHarness, RouletteHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn roulette_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(RouletteHarness::spawn()));
    let h: &RouletteHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/roulette/dwow_roulette_contract.wasm");
    let player_pub = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());
    let house_pub = PublicKey::from_secret(SecretKey::from_bytes([2u8; 32]).unwrap());
    let table_id = pallas::Base::from(1u64);

    ContractTestSpec {
        name: "roulette", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        state_trees: harness.state_trees(),
        endpoints: vec![
            EndpointSpec {
                name: "PlaceBetV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.place_bet(table_id, player_pub, 1, vec![0u8], 1000,
                        pallas::Base::from(99u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SpinWheelV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.spin_wheel(table_id, house_pub, pallas::Base::from(42u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SettleBetsV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.settle_bets(table_id, vec![pallas::Base::from(1u64)])
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "HouseCloseV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                state_tree: "nullifiers", state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.house_close(table_id, house_pub)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
