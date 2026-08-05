//! ContractTestSpec for box contract.
//!
//! Category: L1 O-Cap Primitive (genesis).
//! Functions: 3 (InitializeV1=0x00 non-ZK, PutV1=0x01 ZK, TakeV1=0x02 ZK).
//! Spec: heavyweight-spec.md §5.3.

use dwow_contract_test_harness::harness::{BoxHarness, ContractHarness};
use dwow_sdk::crypto::{BOX_CONTRACT_ID, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::pasta::pallas;

use crate::tests::blockchain::HeavyweightPipeline;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

/// Box contract test specification.
/// The harness uses deterministic values for reproducible proofs.
pub fn box_test_spec() -> ContractTestSpec<'static> {
    // Leak harness to get 'static lifetime — tests run once per process
    let harness = Box::leak(Box::new(BoxHarness::spawn()));

    ContractTestSpec {
        name: "box",
        is_genesis: true,
        contract_id: *BOX_CONTRACT_ID,
        harness,
        wasm_bytes: None,
        has_initialize: false,   // BoxHarness doesn't expose initialize() yet
        initialize: None,
        needs_coinbase_coordination: false,
        state_trees: harness.state_trees(),
        endpoints: vec![
            EndpointSpec {
                name: "PutV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let k = pallas::Base::from(1u64).to_repr().to_vec();
                    let c = *BOX_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "box_roots", &k)?;
                        assert!(r.is_some(), "PutV1: box_roots must contain updated root");
                        Ok(())
                    }
                })),
                state_tree: "box_roots",
                state_key_fn: Box::new(|| {
                    pallas::Base::from(1u64).to_repr().to_vec()
                }),
                generate: Box::new(|| {
                    let r = harness.put()?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "TakeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let dnl = pallas::Base::from(1u64);
                    let os = pallas::Base::from(42u64);
                    let bid = pallas::Base::from(1u64);
                    let sn = pallas::Base::from(1u64);
                    let nf = poseidon_hash([dnl, os, bid, sn]).to_repr().to_vec();
                    let c = *BOX_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "nullifiers", &nf)?;
                        assert!(r.is_some(), "TakeV1: nullifier must exist after consumption");
                        Ok(())
                    }
                })),
                state_tree: "nullifiers",
                state_key_fn: Box::new(|| {
                    let dnl = pallas::Base::from(1u64);
                    let os = pallas::Base::from(42u64);
                    let bid = pallas::Base::from(1u64);
                    let sn = pallas::Base::from(1u64);
                    poseidon_hash([dnl, os, bid, sn]).to_repr().to_vec()
                }),
                generate: Box::new(|| {
                    let r = harness.take()?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
