//! ContractTestSpec for identity contract. Spec: heavyweight-spec.md §5.4.

use dwow_contract_test_harness::harness::{ContractHarness, IdentityHarness};
use dwow_sdk::crypto::{IDENTITY_CONTRACT_ID, IntentNullifier, PublicKey, SecretKey, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::pasta::pallas;
use dwow_identity_contract::model::{CapabilityId, CapabilitySecret, CredentialRequirement};

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn identity_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(IdentityHarness::spawn()));
    let h: &IdentityHarness = harness;

    // Deterministic inputs (all pallas::Base — Copy)
    let issuer_secret = pallas::Base::from(10u64);
    let credential_secret = pallas::Base::from(20u64);
    let schema_hash = pallas::Base::from(30u64);
    let claim_type = pallas::Base::from(50u64);
    let capability_secret = pallas::Base::from(777u64);

    // Pre-compute credential commitment (deterministic from harness seeds)
    let issuer_pub = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
    // The attributes are *named*: the commitment covers `poseidon(10, name, value)` per slot, and
    // the capability below requires `role`, so the credential's first slot must be `role` or the
    // host's comparison rejects the proof.
    let issue_result = h.issue_credential(issuer_secret, credential_secret,
        b"role", pallas::Base::from(100u64),
        b"tenure", pallas::Base::from(200u64),
        pallas::Base::from(300u64), schema_hash, 0, 100000)
        .expect("pre-compute issue_credential");
    let commitment = issue_result.public_inputs.commitment;

    // Pre-compute capability_id. This must be the *same* requirement the endpoint below registers —
    // a capability's id is derived from its requirement (`CapabilityId`: "hash of name + credential
    // requirement"), so two spellings of the requirement are two capabilities and the `verify_state`
    // lookup keys on this one.
    let reg_result = h.register_capability(b"can_vote".to_vec(),
        CredentialRequirement {
            schema_hash: schema_hash.to_repr(), issuer_pub,
            min_threshold: 1, attribute_name: b"role".to_vec(),
        }, None)
        .expect("pre-compute register_capability");
    let cap_id = reg_result.capability_id.inner();
    // Pre-computed keys for verify_state closures
    // Issuer key = compute_issuer_key(pub) = poseidon_hash([x, y, 0, 0]).
    let issuer_key = poseidon_hash([
        issuer_pub.x().expect("not identity"), issuer_pub.y().expect("not identity"),
        pallas::Base::zero(), pallas::Base::zero(),
    ]).to_repr().to_vec();
    // Credential nullifier = poseidon_hash([1, credential_secret, commitment]).
    let credential_nullifier = poseidon_hash([
        pallas::Base::from(1u64), credential_secret, commitment,
    ]).to_repr().to_vec();
    let cap_key = cap_id.to_repr().to_vec();

    ContractTestSpec {
        name: "identity",
        is_genesis: true,
        contract_id: *IDENTITY_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: true,
        initialize: Some(Box::new(move || {
            let r = h.initialize()?;
            Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
        })),
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "RegisterIssuerV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = issuer_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "issuers", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("issuer must be stored".into())); } Ok(()) } })),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    let name = b"test_issuer".to_vec();
                    move || {
                        let r = h.register_issuer(pk, name.clone(), vec![])?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "IssueCredentialV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = credential_nullifier.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "credentials", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("credential must be stored".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.issue_credential(issuer_secret, credential_secret,
                        b"role", pallas::Base::from(100u64),
                        b"tenure", pallas::Base::from(200u64),
                        pallas::Base::from(300u64), schema_hash, 0, 100000)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "RegisterCapabilityV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = cap_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "capabilities", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("state not found".into())); } Ok(()) } })),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        // The capability's requirement must actually describe the credential this
                        // spec issues: same schema, same issuer, and a floor the credential's
                        // attribute clears. It used to require schema `[0u8; 32]` while issuing the
                        // credential with schema `schema_hash` — an incoherence nothing noticed
                        // because nothing compared them.
                        let r = h.register_capability(b"can_vote".to_vec(),
                            CredentialRequirement {
                                schema_hash: schema_hash.to_repr(), issuer_pub: pk,
                                min_threshold: 1, attribute_name: b"role".to_vec(),
                            }, None)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "VerifyCapabilityV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = cap_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "capabilities", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("state not found".into())); } Ok(()) } })),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        // The credential's own parts, exactly as `IssueCredentialV1` above built
                        // them: the verify circuit reconstructs the commitment from these, so a
                        // fixture that disagreed with the issuance would not prove at all.
                        let holder_pub = PublicKey::from_secret(SecretKey::from_base(credential_secret));
                        let r = h.verify_capability(credential_secret, cap_id,
                            pallas::Base::from(50u64),
                            b"role", pallas::Base::from(100u64),
                            b"tenure", pallas::Base::from(200u64),
                            pallas::Base::from(300u64),
                            capability_secret, pk, holder_pub, schema_hash, 0, 100000, true)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // After `VerifyCapabilityV1`, not before it. The order used to be the reverse, and the
            // verification endpoint therefore ran against a credential this one had just revoked —
            // which passed only because revocation was checked nowhere. It is checked now.
            EndpointSpec {
                name: "RevokeCredentialV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = credential_nullifier.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "credentials", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("credential must be updated".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let nf = IntentNullifier::from_base(poseidon_hash([
                        pallas::Base::from(1u64), credential_secret, commitment,
                    ]));
                    let r = h.revoke_credential(issuer_secret, nf, b"test".to_vec())?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "IssueCapabilityV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = cap_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "capabilities", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("state not found".into())); } Ok(()) } })),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let nf = IntentNullifier::from_base(poseidon_hash([
                            pallas::Base::from(1u64), credential_secret, commitment,
                        ]));
                        let r = h.issue_capability(CapabilityId(cap_id), pk, nf)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "RevokeCapabilityV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = cap_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "capabilities", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("state not found".into())); } Ok(()) } })),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.revoke_capability(CapabilityId(cap_id), pk,
                            CapabilitySecret(capability_secret), b"test".to_vec())?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
        ],
    }
}
