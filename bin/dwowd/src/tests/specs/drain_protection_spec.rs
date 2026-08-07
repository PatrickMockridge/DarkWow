//! ContractTestSpec for drain_protection. All endpoints use harness methods
//! with empty_witnesses proofs. Client proof modules pending.
use dwow_contract_test_harness::harness::{DrainProtectionHarness, ContractHarness};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

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
        endpoints: vec![
            mk_ep("initialize", true, Box::new(move || {
                let r = h.initialize().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("propose", true, Box::new(move || {
                let r = h.propose().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("vote", true, Box::new(move || {
                let r = h.vote().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("execute", true, Box::new(move || {
                let r = h.execute().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("exit", true, Box::new(move || {
                let r = h.exit().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("transfer", true, Box::new(move || {
                let r = h.transfer().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("lock", true, Box::new(move || {
                let r = h.lock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("unlock", true, Box::new(move || {
                let r = h.unlock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("update_config", true, Box::new(move || {
                let r = h.update_config().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
