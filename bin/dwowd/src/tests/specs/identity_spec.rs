//! ContractTestSpec for identity contract. Spec: heavyweight-spec.md §5.4.

use dwow_contract_test_harness::harness::{ContractHarness, IdentityHarness};
use dwow_sdk::crypto::{IDENTITY_CONTRACT_ID, IntentNullifier, PublicKey, SecretKey, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::pasta::pallas;
use dwow_identity_contract::model::{
    capability_revoke_key, CapabilityId, CredentialRequirement,
};

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn identity_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the harness needs the id its calls carry, because the commitment covers it.
    // `identity` is a genesis contract, so the id is the constant rather than a derivation.
    let harness = Box::leak(Box::new(IdentityHarness::spawn(*IDENTITY_CONTRACT_ID)));
    let h: &IdentityHarness = harness;

    // Deterministic inputs (all pallas::Base — Copy)
    let issuer_secret = pallas::Base::from(10u64);
    let credential_secret = pallas::Base::from(20u64);
    let schema_hash = pallas::Base::from(30u64);
    let claim_type = pallas::Base::from(50u64);

    // Pre-compute credential commitment (deterministic from harness seeds)
    let issuer_pub = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
    // The attributes are *named*: the commitment covers `poseidon(10, name, value)` per slot, and
    // the capability below requires `role`, so the credential's first slot must be `role` or the
    // host's comparison rejects the proof.
    let issue_result = h.issue_credential(&[], issuer_secret, credential_secret,
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
    // A second capability over the **same** requirement with a different name. The pre-fix
    // derivation hashed only `schema_hash[0..8]`, so this collided with `cap_id` and
    // `register_capability` refused it as `CapabilityAlreadyExists`; it must now be accepted under
    // its own id.
    let reg_result2 = h.register_capability(b"can_vote_2".to_vec(),
        CredentialRequirement {
            schema_hash: schema_hash.to_repr(), issuer_pub,
            min_threshold: 1, attribute_name: b"role".to_vec(),
        }, None)
        .expect("pre-compute second register_capability");
    let cap2_key = reg_result2.capability_id.inner().to_repr().to_vec();
    // A capability whose requirement names a schema the credential does not have. `issue_capability`
    // used to fetch the credential and discard it; it now refuses the mismatch.
    let reg_result3 = h.register_capability(b"can_vote_other".to_vec(),
        CredentialRequirement {
            schema_hash: pallas::Base::from(31u64).to_repr(), issuer_pub,
            min_threshold: 1, attribute_name: b"role".to_vec(),
        }, None)
        .expect("pre-compute third register_capability");
    let cap3_id = CapabilityId(reg_result3.capability_id.inner());
    let cap3_key = reg_result3.capability_id.inner().to_repr().to_vec();
    // Pre-computed keys for verify_state closures
    // Issuer key = compute_issuer_key(pub) = poseidon_hash([x, y, 0, 0]).
    let issuer_key = poseidon_hash([
        issuer_pub.x().expect("not identity"), issuer_pub.y().expect("not identity"),
        pallas::Base::zero(), pallas::Base::zero(),
    ]).to_repr().to_vec();
    // Credential nullifier = poseidon_hash([1, credential_secret, commitment]).
    let credential_nf = IntentNullifier::from_base(poseidon_hash([
        pallas::Base::from(1u64), credential_secret, commitment,
    ]));
    let credential_nullifier = credential_nf.to_bytes().to_vec();
    let cap_key = cap_id.to_repr().to_vec();
    // The revocation marker the holder path writes, and verify must read.
    let holder_pub = PublicKey::from_secret(SecretKey::from_base(credential_secret));
    let revoke_key = capability_revoke_key(&CapabilityId(cap_id), &holder_pub)
        .expect("the holder key has coordinates")
        .to_repr()
        .to_vec();

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
                        // The issuer declares the one schema it will issue; `issue_credential`
                        // refuses anything else (the list was dead state until 2026-10-22).
                        let r = h.register_issuer(pk, name.clone(), vec![schema_hash.to_repr()])?;
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
                    let r = h.issue_credential(&[], issuer_secret, credential_secret,
                        b"role", pallas::Base::from(100u64),
                        b"tenure", pallas::Base::from(200u64),
                        pallas::Base::from(300u64), schema_hash, 0, 100000)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "IssueCredentialV1_UndeclaredSchema", is_zk: true,
                // The issuer declared `schema_hash`; this call proves a credential for a
                // different schema. `authorized_schemas` was written and read by nothing before
                // 2026-10-22, so this row succeeded; it must now be refused with
                // `SchemaNotRecognized` = Custom(8), not accepted and not some later error.
                expectation: EndpointExpectation::RejectionByEndpoint(&["ContractError(Custom(8))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let other_schema = pallas::Base::from(31u64);
                    move || {
                        let r = h.issue_credential(&[], issuer_secret, credential_secret,
                            b"role", pallas::Base::from(100u64),
                            b"tenure", pallas::Base::from(200u64),
                            pallas::Base::from(301u64), other_schema, 0, 100000)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
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
                name: "RegisterCapabilityV1_SameSchemaDifferentName", is_zk: false,
                // Same requirement, different name. Under the truncated id derivation this was
                // refused as a duplicate; the id now commits to the whole requirement plus name.
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = cap2_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "capabilities", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("second capability must be stored under its own id".into())); } Ok(()) } })),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.register_capability(b"can_vote_2".to_vec(),
                            CredentialRequirement {
                                schema_hash: schema_hash.to_repr(), issuer_pub: pk,
                                min_threshold: 1, attribute_name: b"role".to_vec(),
                            }, None)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "RegisterCapabilityV1_OtherSchema", is_zk: false,
                // A capability over a schema no credential in this spec has. Registration itself
                // does not require a credential; issuing to it is the next row's subject.
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = cap3_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "capabilities", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("third capability must be stored".into())); } Ok(()) } })),
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let r = h.register_capability(b"can_vote_other".to_vec(),
                            CredentialRequirement {
                                schema_hash: pallas::Base::from(31u64).to_repr(), issuer_pub: pk,
                                min_threshold: 1, attribute_name: b"role".to_vec(),
                            }, None)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "IssueCapabilityV1_CredentialDoesNotMatch", is_zk: false,
                // The credential is schema 30; this capability requires schema 31. The arm used to
                // discard the fetched credential and increment `issued_count`; it must now refuse
                // with SchemaNotRecognized = Custom(8) before any state is prepared.
                expectation: EndpointExpectation::RejectionByEndpoint(&["ContractError(Custom(8))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.issue_capability(cap3_id, holder_pub, credential_nf)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
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
                        let r = h.verify_capability(&[], credential_secret, cap_id,
                            pallas::Base::from(50u64),
                            b"role", pallas::Base::from(100u64),
                            b"tenure", pallas::Base::from(200u64),
                            pallas::Base::from(300u64),
                            pk, holder_pub, schema_hash, 0, 100000, true)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
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
            // The capability cycle runs before `RevokeCredentialV1` (last): revoking the credential
            // first would make every later verify refuse with `CredentialRevoked` (Custom(4)) and
            // mask the capability-revocation refusal these rows exist to exercise.
            EndpointSpec {
                name: "RevokeCapabilityV1_Holder", is_zk: false,
                // C255's proposition: the **holder** may revoke. The holder signs the revocation
                // message; the contract loads the capability and the credential it presents,
                // requires the stored holder key to equal the named one, and accepts the holder's
                // signature.
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = revoke_key.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "nullifiers", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("revocation marker must be written".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.revoke_capability(
                        SecretKey::from_base(credential_secret),
                        CapabilityId(cap_id),
                        holder_pub,
                        credential_nf,
                        pallas::Base::from(0u64),
                        b"holder-revoke".to_vec(),
                    )?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "VerifyCapabilityV1_AfterRevoke", is_zk: true,
                // The effect of revocation: the same valid credential + holder signature that
                // passed above must now be refused because the marker is read. Before the read
                // was added this verify succeeded, so this row was red; the refusal must be the
                // contract's own `CapabilityRevoked` = Custom(22), not a later check.
                expectation: EndpointExpectation::RejectionByEndpoint(&["ContractError(Custom(22))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let pk = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                    move || {
                        let holder = PublicKey::from_secret(SecretKey::from_base(credential_secret));
                        let r = h.verify_capability(&[], credential_secret, cap_id,
                            pallas::Base::from(50u64),
                            b"role", pallas::Base::from(100u64),
                            b"tenure", pallas::Base::from(200u64),
                            pallas::Base::from(300u64),
                            pk, holder, schema_hash, 0, 100000, true)?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RevokeCapabilityV1_Issuer", is_zk: false,
                // The issuer is the other authority the row and LR-22 name. The same records and
                // message are used; the issuer's key signs. (The marker is already set, so this
                // re-writes it idempotently — the point is that the authority is accepted.)
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.revoke_capability(
                        SecretKey::from_base(issuer_secret),
                        CapabilityId(cap_id),
                        holder_pub,
                        credential_nf,
                        pallas::Base::from(0u64),
                        b"issuer-revoke".to_vec(),
                    )?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "RevokeCapabilityV1_NonIssuer", is_zk: false,
                // Neither the holder nor the issuer signs. The contract must refuse, and the
                // refusal must be its signature check (`InvalidSignature` = Custom(13)), not a
                // later state error.
                expectation: EndpointExpectation::RejectionByEndpoint(&["ContractError(Custom(13))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let attacker = SecretKey::from_base(pallas::Base::from(777u64));
                    let r = h.revoke_capability(
                        attacker,
                        CapabilityId(cap_id),
                        holder_pub,
                        credential_nf,
                        pallas::Base::from(0u64),
                        b"attacker".to_vec(),
                    )?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "RevokeCredentialV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = credential_nullifier.clone(); let c = *IDENTITY_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "credentials", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("credential must be updated".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.revoke_credential(issuer_secret, credential_nf, b"test".to_vec())?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
        ],
    }
}
