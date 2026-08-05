//! ContractTestSpec for insurance_market. Tier: UNDERPOWERED — 4 harness methods.
//! 2 real proofs (underwrite, purchase_coverage), 2 empty_witnesses.
//! 12 functions have NO harness methods.
use dwow_contract_test_harness::harness::{InsuranceMarketHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn insurance_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(InsuranceMarketHarness::spawn()));
    let h: &InsuranceMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/insurance_market/dwow_insurance_market_contract.wasm");
    ContractTestSpec {
        name: "insurance_market", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![], // GAP: 4 harness methods need EndpointSpec closures; 12 functions have no harness
    }
}
