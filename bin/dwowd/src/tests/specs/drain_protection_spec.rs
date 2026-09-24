//! ContractTestSpec for drain_protection.
//!
//! **Every endpoint's proof is now real** (`OBL-C88`): the harness proves through the contract's own
//! client (`create_authority_proof` for the eight authority circuits, `create_exit_proof` for
//! `exit`) and the params carry the same public inputs the proof was made with, where they used to
//! be `empty_witnesses` fabrications. The declarations this header used to carry are gone with them.
//!
//! The endpoints are a **flow**: `propose` derives the proposal id that `vote` and `execute` name
//! (`poseidon_hash([fund.id, message_hash])`), and `initialize` must run first for the fund the
//! others act on. `ExecuteV1` is expected to fail inside the contract — it looks the funds tree up
//! by the *proposal* id and requires a nonzero `multisig_group_id` that `init` stores as zero
//! (`OBL-C98`), so the endpoint's expectation here is not evidence about the fixture.
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
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            mk_ep("initialize", true, Box::new(move || {
                let r = h.initialize().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // The two `update_config` endpoints sit here, immediately after the fund they act on and
            // before the proposal flow, because they are `OBL-C97`'s control pair and a control has
            // to be read as a pair: the same call from the fund's authority and from a stranger.
            mk_ep("update_config", true, Box::new(move || {
                let r = h.update_config().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // NEGATIVE — the endpoint above is what makes this one a control. The stranger holds a
            // **real** proof of the real circuit, built by the contract's own client; the only
            // difference from `update_config` is which secret the published point derives from.
            // Every other precondition is identical — the fund exists, it is unlocked, and the call
            // changes nothing else — so if the host stops comparing the point against the fund's
            // registered `spend_authority`, this endpoint starts succeeding and the control is gone.
            // Read the rejection's cause in the `DWOW_TEST_LOGS=1` output: it must be
            // `InvalidSpendAuthority` ("authority is not the fund's registered spend authority"),
            // not a proof-verification failure and not a state precondition.
            EndpointSpec {
                name: "update_config_by_stranger", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.update_config_as_stranger().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            mk_ep("propose", true, Box::new(move || {
                let r = h.propose().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("vote", true, Box::new(move || {
                let r = h.vote().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("execute", true, Box::new(move || {
                let r = h.execute().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("exit", true, Box::new(move || {
                let r = h.exit().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("transfer", true, Box::new(move || {
                let r = h.transfer().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("lock", true, Box::new(move || {
                let r = h.lock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("unlock", true, Box::new(move || {
                let r = h.unlock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
