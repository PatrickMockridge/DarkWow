//! ContractTestSpec for attestation contract. Spec: heavyweight-spec.md §5.5.

use dwow_contract_test_harness::harness::{AttestationHarness, ContractHarness};
use dwow_attestation_contract::model::Predicate;
use dwow_sdk::crypto::{ATTESTATION_CONTRACT_ID, PublicKey, SecretKey, MerkleNode, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::pasta::pallas;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::modules;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn attestation_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(AttestationHarness::spawn()));
    let state_trees = harness.state_trees();
    let h: &AttestationHarness = harness;

    let attestor_secret = pallas::Base::from(10u64);
    let attestor_pub = PublicKey::from_secret(SecretKey::from_base(attestor_secret));
    let claimant_secret = pallas::Base::from(20u64);
    let claimant_pub = PublicKey::from_secret(SecretKey::from_base(claimant_secret));
    let attestation_id = pallas::Base::from(100u64);
    let claim_id = pallas::Base::from(200u64);

    ContractTestSpec {
        name: "attestation",
        is_genesis: true,
        contract_id: *ATTESTATION_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        state_trees,
        endpoints: vec![
            EndpointSpec {
                name: "CreateAttestationV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let aid = attestation_id.to_repr().to_vec();
                    let c = *ATTESTATION_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "attestations", &aid)?;
                        assert!(r.is_some(), "CreateAttestation: attestation must be stored");
                        Ok(())
                    }
                })),
                state_tree: "attestations",
                state_key_fn: Box::new(move || attestation_id.to_repr().to_vec()),
                generate: Box::new({
                    let pk = attestor_pub;
                    move || {
                        let r = h.create_attestation(attestor_secret, pk,
                            Predicate::GreaterOrEqual, vec![pallas::Base::from(50u64)],
                            b"test".to_vec(), None, attestation_id)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "CreateClaimV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "claims",
                state_key_fn: Box::new(move || claim_id.to_repr().to_vec()),
                generate: Box::new({
                    let pk = claimant_pub;
                    move || {
                        let r = h.create_claim(attestation_id, claimant_secret, pk,
                            Predicate::GreaterOrEqual,
                            pallas::Base::from(2u64).to_repr().to_vec(),
                            b"result".to_vec(), claim_id)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "VerifyClaimV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "claims",
                state_key_fn: Box::new(move || vec![]),
                generate: Box::new(move || {
                    let r = h.verify_claim(claim_id, attestation_id,
                        pallas::Base::from(1u64), pallas::Base::from(2u64),
                        pallas::Base::from(3u64), pallas::Base::from(4u64),
                        pallas::Base::from(5u64), [pallas::Base::from(0u64); 255],
                        pallas::Base::from(6u64))
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ConsumeClaimV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let nf_key = poseidon_hash([
                        pallas::Base::from(1u64), claim_id, claimant_secret,
                    ]).to_repr().to_vec();
                    let c = *ATTESTATION_CONTRACT_ID;
                    move |chain: &HeavyweightPipeline| {
                        let r = chain.query_contract_state(c, "nullifiers", &nf_key)?;
                        assert!(r.is_some(), "ConsumeClaim: nullifier must exist after consumption");
                        Ok(())
                    }
                })),
                state_tree: "nullifiers",
                state_key_fn: Box::new(move || vec![]),
                generate: Box::new({
                    let pk = claimant_pub;
                    move || {
                        let consume_nf = dwow_sdk::crypto::poseidon_hash([
                            pallas::Base::from(1u64), claim_id, claimant_secret,
                        ]);
                        let r = h.consume_claim(claim_id, attestation_id, consume_nf,
                            claimant_secret, pk)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "DelegateAttestationV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "delegations",
                state_key_fn: Box::new(move || vec![]),
                generate: Box::new({
                    let apk = attestor_pub;
                    let cpk = claimant_pub;
                    move || {
                        let r = h.delegate_attestation(
                            pallas::Base::from(1u64), pallas::Base::from(2u64),
                            attestor_secret, pallas::Base::from(3u64),
                            pallas::Base::from(4u64), pallas::Base::from(5u64),
                            pallas::Base::from(6u64), pallas::Base::from(7u64),
                            pallas::Base::from(8u64), pallas::Base::from(9u64),
                            pallas::Base::from(10u64), pallas::Base::from(11u64),
                            pallas::Base::from(12u64), [pallas::Base::from(0u64); 255],
                            pallas::Base::from(13u64), [pallas::Base::from(0u64); 255],
                            apk, cpk)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "CheckNotRevokedV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "nullifiers",
                state_key_fn: Box::new(move || vec![]),
                generate: Box::new(move || {
                    let ep: Vec<MerkleNode> = vec![MerkleNode::new(pallas::Base::from(0u64)); 32];
                    let r = h.check_not_revoked(
                        pallas::Base::from(100u64), pallas::Base::from(200u64), 0, ep)
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "UpdateDelegationV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "delegations",
                state_key_fn: Box::new(move || vec![]),
                generate: Box::new(move || {
                    let r = h.update_delegation(
                        attestation_id, pallas::Base::from(0u64),
                        pallas::Base::from(0u64), pallas::Base::from(5u64),
                        pallas::Base::from(1000u64), pallas::Base::from(500u64),
                        pallas::Base::from(10000u64),
                        1000, 500, 10000, 0, 5, 0)
                        .map_err(modules::error_bridge::bridge)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "AttestSlashV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "attestations",
                state_key_fn: Box::new(move || vec![]),
                generate: Box::new({
                    let pk = attestor_pub;
                    move || {
                        let r = h.attest_slash(pk, 500, pallas::Base::from(999u64), 5)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "CommitFeeScheduleV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "attestations",
                state_key_fn: Box::new(move || vec![]),
                generate: Box::new({
                    let pk = attestor_pub;
                    move || {
                        let r = h.commit_fee_schedule(pk, 100, 50, 1000000, 1000, vec![])
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
