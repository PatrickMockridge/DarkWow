//! ContractTestSpec for bridge. Tier: UNDERPOWERED — 7 harness methods, 8 missing.
//! 6 deposit variants use real proofs but old test match-Err-skipped them (RG-10).
//! Gaps: 8 functions have no harness methods. Sinsemilla Merkle data mismatch for deposits.
use dwow_contract_test_harness::harness::{BridgeHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn bridge_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BridgeHarness::spawn()));
    let h: &BridgeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/bridge/dwow_bridge_contract.wasm");
    ContractTestSpec {
        name: "bridge", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![], // GAP: 7 harness methods need EndpointSpec closures; 8 missing harness methods
    }
}
