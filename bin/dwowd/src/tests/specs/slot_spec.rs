//! ContractTestSpec for slot. Tier: STUB — all endpoints use empty_witnesses.
//! Per RG-24 (§4.11): NONE may appear as active specs.
//! Tracking: slot-client-proofs — empty_witnesses proofs prove nothing about contract logic.
use dwow_contract_test_harness::harness::{SlotHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn slot_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(SlotHarness::spawn()));
    let h: &SlotHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/slot/dwow_slot_contract.wasm");
    ContractTestSpec {
        name: "slot", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![], // ALL empty_witnesses — tracked at slot-client-proofs
    }
}
