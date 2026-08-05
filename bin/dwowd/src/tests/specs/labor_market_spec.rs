//! ContractTestSpec for labor_market. Tier: HARVESTABLE — 9 harness methods, all ZK.
//! Cross-contract deps on identity + attestation.
use dwow_contract_test_harness::harness::{LaborMarketHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn labor_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(LaborMarketHarness::spawn()));
    let h: &LaborMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/labor_market/dwow_labor_market_contract.wasm");
    ContractTestSpec {
        name: "labor_market", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![], // GAP: 9 harness methods need EndpointSpec closures
    }
}
