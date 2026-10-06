//! ContractTestSpec for relayer_endowment. Spec: heavyweight-spec.md §5.9.
//! Harness: PARTIAL (3/8, real proofs). Tier: UNDERPOWERED.

use dwow_contract_test_harness::harness::{ContractHarness, RelayerEndowmentHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn relayer_endowment_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the proof binds to a commitment derived over the call set, and a call carries the
    // contract it addresses — so the harness must be given the deployed id rather than a
    // placeholder, exactly as the node's `get_metadata` arm derives over the same bytes.
    let relayer_endowment_cid =
        crate::tests::blockchain::derive_contract_id_from_name("relayer_endowment");
    let harness = Box::leak(Box::new(RelayerEndowmentHarness::spawn(relayer_endowment_cid)));
    let h: &RelayerEndowmentHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/relayer_endowment/dwow_relayer_endowment_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());
    let r_pk = PublicKey::from_secret(SecretKey::from_bytes([2u8; 32]).unwrap());

    ContractTestSpec {
        name: "relayer_endowment", is_genesis: false,
        contract_id: relayer_endowment_cid,
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "InitializeV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    // No height argument: the harness takes the verifying block height from the
                    // runner (`ContractHarness::set_next_block_height`), because it is the one
                    // input to `InitializeV2` that a caller cannot choose.
                    let r = h.initialize(pk, 1000u32)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "DeployCapitalV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    // **This row cannot pass yet, and it is not a defect in the contract.**
                    // `deploy_capital_v1` requires one `promissory_note::transfer_v1` child, and
                    // this spec passes none — the run fails `Expected 1 child call
                    // (promissory_note::transfer_v1), got 0` (error code 10). Wiring it needs the
                    // two-phase form on both sides, because the commitment is a derivation over the
                    // **whole ordered call set**: `pn_transfer_prepare` (the peer's helper in
                    // `modules::child_calls`) builds the child's data without proving it, the
                    // parent's is built here, one commitment is taken over [child, parent], and
                    // both are then proven against that value. `deploy_capital_solo` is the
                    // no-child convenience and is deliberately what is called until that lands, so
                    // the failure stays where it belongs rather than being hidden.
                    let r = h.deploy_capital_solo(pk, 1000,
                        pallas::Base::from(1u64),
                        pallas::Scalar::from(100u64), r_pk, 1000u32)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ClaimFeesV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.claim_fees(pallas::Base::from(1u64), pk, 100)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
