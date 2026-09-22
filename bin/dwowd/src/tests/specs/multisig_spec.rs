//! ContractTestSpec for multisig. Spec: heavyweight-spec.md §5.6.
//! HAZOP remediation: 3-of-5 group, threshold enforcement, replay protection.
//! OBL-Z11: the signer is bound to the group by a member commitment, so a non-member naming a
//! member's identity cannot sign — there is a rejection endpoint for exactly that.

use dwow_contract_test_harness::harness::{ContractHarness, MultiSigHarness};
use dwow_sdk::crypto::{MULTISIG_CONTRACT_ID, pasta_prelude::PrimeField};
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
    // The group stores commitments, not keys: H(DOMAIN_MEMBER_COMMITMENT, secret).
    let members: Vec<pallas::Base> = secrets.iter()
        .map(|&s| MultiSigHarness::member_commitment(s))
        .collect();
    let threshold: u8 = 3;
    let message_hash = pallas::Base::from(42u64);

    let group_id = MultiSigHarness::group_id(threshold, &members);
    let gid_bytes = group_id.to_repr().to_vec();

    // Member 0's nullifier for this message — the signature record the tree must hold, and the one
    // finalize must delete.
    let nf0 = MultiSigHarness::signer_nullifier(secrets[0], group_id, message_hash);
    let nf0_bytes = nf0.to_bytes().to_vec();
    let nf0b2 = nf0_bytes.clone();
    let gb2 = gid_bytes.clone();

    // The approvals each finalize names. The host can no longer recompute these — they are derived
    // from secrets it does not have — so it checks each against the signature records instead.
    let nf1 = MultiSigHarness::signer_nullifier(secrets[1], group_id, message_hash);
    let nf2 = MultiSigHarness::signer_nullifier(secrets[2], group_id, message_hash);

    ContractTestSpec {
        name: "multisig",
        is_genesis: true,
        contract_id: cid,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
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
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SignV1_member1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let nb = nf0_bytes.clone();
                    let c = cid;
                    move |chain| {
                        let result = chain.query_contract_state(c, "signatures", &nb)?;
                        if result.is_none() { return Err(dwow_core::Error::Custom("WARN [multisig::SignV1]: signature nullifier must exist in signatures tree".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, secrets[0])?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SignV1_member2", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, secrets[1])?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // MUST REJECT: only 2/3 signatures
            EndpointSpec {
                name: "FinalizeV1_insufficient", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.finalize(group_id, message_hash, vec![nf0, nf1])?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // MUST REJECT: OBL-Z11's attack, end to end. Secret 99 is in no group. The proof is
            // real — it opens the commitment H(DOMAIN_MEMBER_COMMITMENT, 99) — but that commitment
            // is not one the group holds, so the host refuses. Before this fix the attacker did not
            // even need a proof over their own secret: they named a member's *public key*, which is
            // in the group record, and the nullifier recorded was that member's. Repeating over the
            // members forged threshold approval outright; if this endpoint ever flips to Success,
            // that is back.
            EndpointSpec {
                name: "SignV1_non_member", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, pallas::Base::from(99u64))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SignV1_member3", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, secrets[2])?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // MUST SUCCEED with 3/3, verify signatures DELETED (HAZOP H-5)
            EndpointSpec {
                name: "FinalizeV1_sufficient", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let nb = nf0b2.clone();
                    let c = cid;
                    move |chain| {
                        let result = chain.query_contract_state(c, "signatures", &nb)?;
                        if result.is_some() { return Err(dwow_core::Error::Custom("WARN [multisig::FinalizeV1]: consumed signatures must be DELETED (HAZOP H-5)".into())); }
                        Ok(())
                    }
                })),
                generate: Box::new(move || {
                    let r = h.finalize(group_id, message_hash, vec![nf0, nf1, nf2])?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
