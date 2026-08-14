//! ContractTestSpec for oracle contract. Spec: heavyweight-spec.md §5.8.

use dwow_contract_test_harness::harness::{ContractHarness, OracleHarness};
use dwow_sdk::crypto::{ORACLE_CONTRACT_ID, PublicKey, SecretKey, MerkleNode, pasta_prelude::PrimeField};
use dwow_sdk::pasta::pallas;

use crate::tests::modules;
use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn oracle_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(OracleHarness::spawn()));
    let h: &OracleHarness = harness;

    let oracle_secret = pallas::Base::from(10u64);
    let oracle_pub = PublicKey::from_secret(SecretKey::from_base(oracle_secret));
    let oracle_id = pallas::Base::from(1u64);
    // Pre-compute state key bytes (oracle_id) to avoid lifetime issues.
    // The contract keys the "oracles" tree by oracle_id.to_bytes() (== to_repr()).
    let oracle_key = oracle_id.to_repr().to_vec();

    ContractTestSpec {
        name: "oracle",
        is_genesis: true,
        contract_id: *ORACLE_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "RegisterOracleV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = oracle_key.clone(); let c = *ORACLE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "oracles", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("oracle must be stored".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.register_oracle(oracle_secret, oracle_pub,
                        oracle_id, "price_feed".to_string(), "u64".to_string())
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "PushValueV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = oracle_key.clone(); let c = *ORACLE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "oracles", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("value must be updated".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.push_value(oracle_id, oracle_secret, oracle_pub, pallas::Base::from(42u64))
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "AttestValueV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = oracle_key.clone(); let c = *ORACLE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "oracles", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("attestation must be stored".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.attest_value(oracle_id, pallas::Base::from(100u64),
                        oracle_secret, pallas::Base::from(0u64), pallas::Base::from(42u64),
                        pallas::Base::from(42u64), oracle_pub)
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "PushValueCommitmentV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = oracle_key.clone(); let c = *ORACLE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "oracles", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("commitment must be stored".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let ep = vec![MerkleNode::new(pallas::Base::from(0u64)); 32];
                    let r = h.push_value_commitment(oracle_id, oracle_secret, 0, ep,
                        pallas::Base::from(42u64), pallas::Base::from(99u64), oracle_pub,
                        pallas::Base::from(100u64), pallas::Base::from(200u64))
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "AggregateV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = oracle_key.clone(); let c = *ORACLE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "oracles", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("result must be stored".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.aggregate(oracle_id,
                        [pallas::Base::from(10u64); 4], [pallas::Base::from(1u64); 4],
                        pallas::Base::from(4u64), pallas::Base::from(10u64),
                        pallas::Base::from(0u64), pallas::Base::from(100u64))
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SetOracleActiveV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = oracle_key.clone(); let c = *ORACLE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "oracles", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("active flag must be set".into())); } Ok(()) } })),
                generate: Box::new({
                    let opk = oracle_pub;
                    move || {
                        let r = h.set_oracle_active(opk, true)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
        ],
    }
}
