//! ContractTestSpec for identity contract. Spec: heavyweight-spec.md §5.4.

use dwow_contract_test_harness::harness::{ContractHarness, IdentityHarness};
use dwow_sdk::crypto::{IDENTITY_CONTRACT_ID, IntentNullifier, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use dwow_identity_contract::model::{CapabilityId, CapabilitySecret, CredentialRequirement};
use dwow_sdk::crypto::IntentNullifier;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn identity_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(IdentityHarness::spawn()));
    let state_trees = harness.state_trees();
    let h: &IdentityHarness = harness;

    // Deterministic inputs (all pallas::Base — Copy)
    let issuer_secret = pallas::Base::from(10u64);
    let credential_secret = pallas::Base::from(20u64);
    let schema_hash = pallas::Base::from(30u64);
    let claim_type = pallas::Base::from(50u64);
    let capability_secret = pallas::Base::from(777u64);

    // Pre-compute credential commitment (deterministic from harness seeds)
    let issuer_pub = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
    let issue_result = h.issue_credential(issuer_secret, credential_secret,
        pallas::Base::from(100u64), pallas::Base::from(200u64),
        pallas::Base::from(300u64), schema_hash, 0, 100000)
        .expect("pre-compute issue_credential");
    let commitment = issue_result.public_inputs.commitment;

    // Pre-compute capability_id
    let reg_result = h.register_capability(b"can_vote".to_vec(),
        CredentialRequirement {
            schema_hash: [0u8; 32], issuer_pub,
            min_threshold: 1, attribute_name: b"role".to_vec(),
        }, None)
        .expect("pre-compute register_capability");
    let cap_id = reg_result.capability_id.inner();

    ContractTestSpec {
        name: "identity",
        is_genesis: true,
        contract_id: *IDENTITY_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: true,
        initialize: Some(Box::new(move || {
            let r = h.initialize()?;
            Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
        })),
        needs_coinbase_coordination: false,
        state_trees,
        endpoints: vec![
            EndpointSpec {
                name: "RegisterIssuerV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "issuers",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    let name = b"test_issuer".to_vec();
                    move || {
                        let r = h.register_issuer(pk, name.clone(), vec![])?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "IssueCredentialV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "credentials",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let r = h.issue_credential(issuer_secret, credential_secret,
                        pallas::Base::from(100u64), pallas::Base::from(200u64),
                        pallas::Base::from(300u64), schema_hash, 0, 100000)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "RevokeCredentialV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "credentials",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new(move || {
                    let nf = IntentNullifier::from_bytes([0u8; 32]).unwrap();
                    let r = h.revoke_credential(issuer_secret, nf, b"test".to_vec())?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "CreateClaimV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "nullifiers",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.create_claim(credential_secret, pallas::Base::from(100u64),
                            pallas::Base::from(50u64), commitment, pk, schema_hash, claim_type)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RegisterCapabilityV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "capabilities",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.register_capability(b"can_vote".to_vec(),
                            CredentialRequirement {
                                schema_hash: [0u8; 32], issuer_pub: pk,
                                min_threshold: 1, attribute_name: b"role".to_vec(),
                            }, None)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "VerifyCapabilityV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "capabilities",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.verify_capability(credential_secret, commitment,
                            pallas::Base::from(100u64), pallas::Base::from(50u64),
                            capability_secret, pk, schema_hash, cap_id, true)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "IssueCapabilityV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "capabilities",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.issue_capability(CapabilityId(cap_id), pk,
                            IntentNullifier::from_bytes([0u8; 32]).unwrap())?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "RevokeCapabilityV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                state_tree: "capabilities",
                state_key_fn: Box::new(|| vec![]),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.revoke_capability(CapabilityId(cap_id), pk,
                            CapabilitySecret(capability_secret), b"test".to_vec())?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
        ],
    }
}
