//! ContractTestSpec for relayer_endowment. Spec: heavyweight-spec.md §5.9.
//! Harness: PARTIAL (3/8, real proofs). Tier: UNDERPOWERED.

use dwow_contract_test_harness::harness::{ContractHarness, RelayerEndowmentHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn relayer_endowment_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(RelayerEndowmentHarness::spawn()));
    let h: &RelayerEndowmentHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/relayer_endowment/dwow_relayer_endowment_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());
    let r_pk = PublicKey::from_secret(SecretKey::from_bytes([2u8; 32]).unwrap());

    ContractTestSpec {
        name: "relayer_endowment", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "InitializeV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.initialize(pk, 1000u32, 0u64)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "DeployCapitalV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.deploy_capital(pallas::Base::from(1u64), pk, 1000,
                        pallas::Base::from(1u64), 0u64,
                        pallas::Scalar::from(100u64), r_pk, 1000u32)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ClaimFeesV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.claim_fees(pallas::Base::from(1u64), pk, 100, 0u64)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
