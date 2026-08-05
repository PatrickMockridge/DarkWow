//! ContractTestSpec for darkbet_exchange. Tier: UNDERPOWERED — 10 harness methods.
//! 4 real proofs, 6 empty_witnesses.
use dwow_contract_test_harness::harness::{DarkbetExchangeHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn darkbet_exchange_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DarkbetExchangeHarness::spawn()));
    let h: &DarkbetExchangeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/darkbet_exchange/dwow_darkbet_exchange_contract.wasm");
    ContractTestSpec {
        name: "darkbet_exchange", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![], // GAP: 10 harness methods need EndpointSpec closures; 6 use empty_witnesses
    }
}
