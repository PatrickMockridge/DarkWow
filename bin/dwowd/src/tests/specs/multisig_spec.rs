//! ContractTestSpec for multisig. Spec: heavyweight-spec.md §5.6.
//! HAZOP remediation: 3-of-5 group, threshold enforcement, replay protection.

use dwow_contract_test_harness::harness::{ContractHarness, MultiSigHarness};
use dwow_sdk::crypto::{
    MULTISIG_CONTRACT_ID, PublicKey, SecretKey,
    pasta_prelude::PrimeField, poseidon_hash,
};
use dwow_sdk::pasta::pallas;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn multisig_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(MultiSigHarness::spawn()));
    let h: &MultiSigHarness = harness;
    let cid = *MULTISIG_CONTRACT_ID;

    // 5 signers with deterministic secrets
    let secrets = [
        pallas::Base::from(3u64),
        pallas::Base::from(4u64),
        pallas::Base::from(5u64),
        pallas::Base::from(6u64),
        pallas::Base::from(7u64),
    ];
    let pubkeys: Vec<PublicKey> = secrets.iter()
        .map(|&s| PublicKey::from_secret(SecretKey::from_base(s)))
        .collect();
    let threshold: u8 = 3;
    let members = pubkeys.clone();
    let message_hash = pallas::Base::from(42u64);

    // Pre-compute group_id
    let (fx, fy) = pubkeys[0].xy().expect("pk not identity");
    let t = pallas::Base::from(threshold as u64);
    let n = pallas::Base::from(pubkeys.len() as u64);
    let group_id = poseidon_hash([fx, fy, t, n]);
    let gid_bytes = group_id.to_repr().to_vec();

    // Pre-compute nullifier for member 1 (used in verify_state — cloned per closure)
    let nf1 = {
        let (px, py) = pubkeys[0].xy().expect("pk");
        poseidon_hash([group_id, message_hash, px, py])
    };
    let nf1_bytes = nf1.to_repr().to_vec();
    let nf1b2 = nf1_bytes.clone();
    let gb2 = gid_bytes.clone();

    ContractTestSpec {
        name: "multisig",
        is_genesis: true,
        contract_id: cid,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "CreateGroupV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let gb = gid_bytes.clone();
                    let c = cid;
                    move |chain| {
                        let result = chain.query_contract_state(c, "groups", &gb)?;
                        if result.is_none() { return Err(dwow_core::Error::Custom("WARN [multisig::CreateGroupV1]: group must be stored in groups tree".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new({
                    let m = members.clone();
                    move || {
                        let r = h.create_group(threshold, m.clone())?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SignV1_member1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let nb = nf1_bytes.clone();
                    let c = cid;
                    move |chain| {
                        let result = chain.query_contract_state(c, "signatures", &nb)?;
                        if result.is_none() { return Err(dwow_core::Error::Custom("WARN [multisig::SignV1]: signature nullifier must exist in signatures tree".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, secrets[0])?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SignV1_member2", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, secrets[1])?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // MUST REJECT: only 2/3 signatures
            EndpointSpec {
                name: "FinalizeV1_insufficient", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.finalize(group_id, message_hash)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SignV1_member3", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, secrets[2])?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // MUST SUCCEED with 3/3, verify signatures DELETED (HAZOP H-5)
            EndpointSpec {
                name: "FinalizeV1_sufficient", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let nb = nf1_bytes.clone();
                    let c = cid;
                    move |chain| {
                        let result = chain.query_contract_state(c, "signatures", &nb)?;
                        if result.is_some() { return Err(dwow_core::Error::Custom("WARN [multisig::FinalizeV1]: consumed signatures must be DELETED (HAZOP H-5)".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(move || {
                    let r = h.finalize(group_id, message_hash)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
