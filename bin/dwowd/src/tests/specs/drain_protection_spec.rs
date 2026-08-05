//! ContractTestSpec for drain_protection. Tier: STUB — all endpoints use empty_witnesses.
//! Per RG-24 (§4.11): NONE may appear as active specs.
//! Tracking: drain_protection-client-proofs
use dwow_contract_test_harness::harness::{DrainProtectionHarness, ContractHarness};
use crate::tests::uniform_runner::*;

pub fn drain_protection_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DrainProtectionHarness::spawn()));
    let h: &DrainProtectionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm");
    ContractTestSpec {
        name: "drain_protection", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![], // ALL empty_witnesses — tracked at drain_protection-client-proofs
    }
}
