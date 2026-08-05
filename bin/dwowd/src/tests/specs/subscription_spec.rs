//! ContractTestSpec for subscription. Tier: HARVESTABLE — 5 harness methods.
//! 3 real proofs (subscribe-25params, cancel-5params, renew-6params), 2 empty_witnesses.
use dwow_contract_test_harness::harness::{SubscriptionHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn subscription_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(SubscriptionHarness::spawn()));
    let h: &SubscriptionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/subscription/dwow_subscription_contract.wasm");
    ContractTestSpec {
        name: "subscription", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![], // GAP: subscribe needs 25 params, cancel 5, renew 6 — input construction pending
    }
}
