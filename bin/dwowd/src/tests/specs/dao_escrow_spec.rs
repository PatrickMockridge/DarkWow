//! ContractTestSpec for dao_escrow. Tier: HARVESTABLE — 13 harness methods, 7 ZK, 6 non-ZK.
//! Gaps: pay_premium has circuit bug. Endpoint closures need harness input construction.
use dwow_contract_test_harness::harness::{DaoEscrowHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn dao_escrow_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DaoEscrowHarness::spawn()));
    let h: &DaoEscrowHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm");
    ContractTestSpec {
        name: "dao_escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![], // GAP: 13 harness methods need EndpointSpec closures with input construction
    }
}
