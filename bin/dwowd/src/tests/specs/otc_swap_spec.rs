//! ContractTestSpec for otc_swap. Spec: heavyweight-spec.md §5.9.
//! Harness: COMPLETE (4/4 operational functions).

use dwow_contract_test_harness::harness::{ContractHarness, OtcSwapHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn otc_swap_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(OtcSwapHarness::spawn()));
    let state_trees = harness.state_trees();
    let h: &OtcSwapHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/otc_swap/dwow_otc_swap_contract.wasm");

    let alice_sk = pallas::Base::from(1u64);
    let bob_sk = pallas::Base::from(2u64);

    ContractTestSpec {
        name: "otc_swap",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        state_trees,
        endpoints: vec![
            EndpointSpec {
                name: "CreateSwapV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "nullifiers",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.create_swap(alice_sk,
                        PublicKey::from_secret(SecretKey::from_base(alice_sk)),
                        PublicKey::from_secret(SecretKey::from_base(bob_sk)),
                        1000, pallas::Base::from(3u64), 500,
                        pallas::Base::from(4u64), 100)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "FundSwapV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "nullifiers",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.fund_swap(1000, pallas::Scalar::from(100u64),
                        pallas::Base::from(1u64), 0,
                        vec![dwow_sdk::crypto::MerkleNode::new(pallas::Base::from(0u64)); 32])
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ExecuteSwapV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "nullifiers",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.execute_swap(pallas::Base::from(1u64), bob_sk,
                        PublicKey::from_secret(SecretKey::from_base(bob_sk)),
                        PublicKey::from_secret(SecretKey::from_base(alice_sk)),
                        PublicKey::from_secret(SecretKey::from_base(bob_sk)))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "CancelSwapV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "nullifiers",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.cancel_swap(pallas::Base::from(1u64), alice_sk,
                        PublicKey::from_secret(SecretKey::from_base(alice_sk)),
                        1000, 0, PublicKey::from_secret(SecretKey::from_base(alice_sk)))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
