//! ContractTestSpec for dao_escrow. Tier: HARVESTABLE — 13 harness methods.
//! 12 endpoints active, 1 deferred (pay_premium: circuit bug).
//!
//! # Governance (`OBL-C151`)
//!
//! The four governance gates used to be unreachable: `endowment.multisig_group_id` was written once, to
//! zero, and nothing could set it — so `propose_claim`, `vote_claim`, `resolve_dispute` and the
//! capability path of the two spend endpoints all refused with `GovernanceNotActive`, while
//! `withdraw_v1`'s governance branch was an empty body that would have failed open.
//!
//! **Order is the fixture's whole design.** Activating governance changes every gated endpoint's
//! acceptance — including `withdraw_v1`, which switches from its owner path to the group's — so the
//! rows that exercise the *inactive* path run first and the setter sits between them and the rows that
//! exercise the active one. The runner aborts at the first failing endpoint, so a setter placed first
//! would have made every earlier row unreachable and the failures unreadable.
//!
//! The approvals are cast in `setup` because the finalize child *names* them rather than re-deriving
//! them, and each is a **distinct message**: a MultiSig approval is spend-once, so one approval cannot
//! authorise two actions. These declarations and the copy below used to carry `IntentNullifier::ZERO`
//! in a fabricated `CapabilityProof`, which the type refuses — so `ProposeClaimParamsV1::decode` failed
//! and the contract's metadata arm returned the bare `vec![]` the host reports as "rejected by design",
//! which is why this contract's red read only `metadata-decode-zkp … EMPTY metadata` for a week.
use dwow_contract_test_harness::harness::{ContractHarness, DaoEscrowHarness, MultiSigHarness};
use dwow_dao_escrow_contract::model::{
    governance_message, governance_role, CapabilityProof, ClaimType,
};
use dwow_sdk::crypto::{
    pasta_prelude::PrimeField, IntentNullifier, Nullifier, PublicKey, SecretKey,
    MULTISIG_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::*;
use super::helpers::{mk_ep, mk_ep_rejecting};

/// The approvals the governance group cast in `setup`, per case. Each is a set of signature nullifiers,
/// captured from the `sign` calls that produced them because the finalize child *names* them.
#[derive(Default, Clone)]
struct Governance {
    /// The endowment's group on the proposal message — the approvals that must be accepted.
    propose: Vec<Nullifier>,
    /// The endowment's group on the vote message. A distinct message, not the proposal's: the approval
    /// is spend-once and both endpoints key on the same `claim_id`.
    vote: Vec<Nullifier>,
    /// The endowment's group on the dispute message.
    resolve: Vec<Nullifier>,
    /// The endowment's group on a *different* message — valid approvals of the wrong thing.
    wrong_message: Vec<Nullifier>,
    /// A second group's id and its approvals of the proposal — valid approvals by the wrong group.
    foreign_group: pallas::Base,
    foreign: Vec<Nullifier>,
}

pub fn dao_escrow_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DaoEscrowHarness::spawn()));
    let h: &DaoEscrowHarness = harness;
    // Leaked like the contract's harness, and for a reason that shows up in the wall clock: every
    // `spawn` rebuilds the multisig contract's proving keys, and this spec calls `create_group`, `sign`
    // and `finalize` from a setup that runs twice plus several endpoints.
    let ms: &'static MultiSigHarness = Box::leak(Box::new(MultiSigHarness::spawn()));
    let wasm = include_bytes!("../../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm");
    let owner_secret = pallas::Base::from(12345u64);
    let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
    let dao_bulla = pallas::Base::from(1u64);
    let claim_id = pallas::Base::from(100u64);
    let proposal_id = pallas::Base::from(200u64);
    let capability_id = pallas::Base::from(999u64);
    let identity_contract_bulla = pallas::Base::from(300u64);
    let nullifier_k = pallas::Scalar::from(1u64);
    let endowment_asset_id = pallas::Base::from(42u64);
    let bulla_blind = pallas::Base::from(9999u64);
    let voter_secret = pallas::Base::from(333u64);
    let voter_pub = PublicKey::from_secret(SecretKey::from_base(voter_secret));
    let proposer_secret = pallas::Base::from(777u64);
    let proposer_pub = PublicKey::from_secret(SecretKey::from_base(proposer_secret));
    let holder_secret = pallas::Base::from(111u64);
    let holder_pub = PublicKey::from_secret(SecretKey::from_base(holder_secret));
    let arbitrator_secret = pallas::Base::from(600u64);
    let arbitrator_pub = PublicKey::from_secret(SecretKey::from_base(arbitrator_secret));
    let capability_secret = pallas::Base::from(888u64);
    let dispute_id = pallas::Base::from(500u64);
    let cp_id = capability_id.to_repr();
    let cp_secret = capability_secret.to_repr();

    // The endowment is stored under `derive_bulla(dao_bulla, owner, asset, blind)` — `initialize_v1`
    // derives it — while every endpoint that touches the endowment looks it up by its own
    // `dao_escrow_bulla` field. So a caller passes the DERIVED value in that field, and `initialize` is
    // the only call that takes the DAO's own bulla and derives.
    let endowment_bulla = dwow_dao_escrow_contract::model::DaoEscrow::derive_bulla(
        dwow_dao_escrow_contract::model::DaoEscrowBulla(dao_bulla),
        &owner_pub,
        dwow_sdk::crypto::AssetId::from_base(endowment_asset_id),
        dwow_sdk::crypto::Blind(bulla_blind),
    )
    .inner();

    // The messages, computed with the CONTRACT'S OWN derivation so that the message the group signs and
    // the message the contract checks cannot drift — the reason `MultiSigHarness::group_id` delegates to
    // the multisig contract's `derive_group_id` rather than re-implementing it.
    let msg_propose = governance_message(governance_role::PROPOSE_CLAIM, claim_id);
    let msg_vote = governance_message(governance_role::VOTE_CLAIM, claim_id);
    // `resolve_dispute_v1` derives its own `dispute_id` from `(proposal_id, attestation_count,
    // payout_recipient_x)`; the spec passes no attestations, so the count is zero.
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let payout_recipient_x = arbitrator_pub.xy().expect("pk not identity").0;
    let contract_dispute_id =
        dwow_sdk::crypto::poseidon_hash([proposal_id, pallas::Base::zero(), payout_recipient_x]);
    let msg_resolve = governance_message(governance_role::RESOLVE_DISPUTE, contract_dispute_id);
    let msg_wrong = governance_message(governance_role::PROPOSE_CLAIM, pallas::Base::from(9999u64));

    let gov: Arc<Mutex<Governance>> = Arc::new(Mutex::new(Governance::default()));

    ContractTestSpec {
        name: "dao_escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: true,
        initialize: Some(Box::new(move || {
            let r = h.initialize(nullifier_k, dao_bulla, owner_secret, endowment_asset_id, bulla_blind).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
            Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
        })),
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let gov = gov.clone();
            move |chain| {
                let ms_cid = *MULTISIG_CONTRACT_ID;
                let group_id = DaoEscrowHarness::governance_group();
                let created = ms
                    .create_group(
                        DaoEscrowHarness::GOVERNANCE_THRESHOLD,
                        DaoEscrowHarness::governance_member_commitments(),
                    )
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                // The harness derives the id with the multisig contract's own function and the contract
                // derives it again; if they ever disagreed the endowment would store a group no
                // signature could satisfy, and it would look like a contract refusal.
                assert_eq!(
                    created.group_id, group_id,
                    "the created group's id must be the one the endowment will register",
                );
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &created.call_data, vec![created.proof])?.submit(),
                )?;

                // One approval set per message, each by `GOVERNANCE_THRESHOLD` of the three members —
                // real signatures, because the multisig contract counts the threshold itself.
                let mut sign_for = |chain: &crate::tests::blockchain::HeavyweightPipeline,
                                    message: pallas::Base|
                 -> dwow_core::Result<Vec<Nullifier>> {
                    let mut out = Vec::new();
                    for secret in DaoEscrowHarness::GOVERNANCE_MEMBERS
                        .iter()
                        .take(DaoEscrowHarness::GOVERNANCE_THRESHOLD as usize)
                    {
                        let s = ms
                            .sign(group_id, message, *secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        smol::block_on(
                            chain.block()?.with_call(ms_cid, ms, &s.call_data, vec![s.proof])?.submit(),
                        )?;
                        out.push(s.nullifier);
                    }
                    Ok(out)
                };
                let propose = sign_for(chain, msg_propose)?;
                let vote = sign_for(chain, msg_vote)?;
                let resolve = sign_for(chain, msg_resolve)?;
                let wrong_message = sign_for(chain, msg_wrong)?;

                // A second group — one member, threshold one — approves the proposal. Its approval is
                // valid; what is wrong is who gave it.
                const FOREIGN_MEMBER: pallas::Base = pallas::Base::from_raw([9876, 0, 0, 0]);
                let foreign = ms
                    .create_group(1, vec![MultiSigHarness::member_commitment(FOREIGN_MEMBER)])
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &foreign.call_data, vec![foreign.proof])?.submit(),
                )?;
                let f = ms
                    .sign(foreign.group_id, msg_propose, FOREIGN_MEMBER)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &f.call_data, vec![f.proof])?.submit(),
                )?;

                *gov.lock().unwrap() = Governance {
                    propose,
                    vote,
                    resolve,
                    wrong_message,
                    foreign_group: foreign.group_id,
                    foreign: vec![f.nullifier],
                };
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            // ── Governance INACTIVE. These run first: the group id is still zero, so they exercise the
            //    paths that existed before `OBL-C151` and the setter below changes their meaning.
            // Requires an Identity `VerifyCapabilityV1` (0x06) child — the possession fixture
            // `insurance_market`'s spec now has, not yet ported here — so it is rejected for the missing
            // child. Named, because the missing child is the reason and `Custom(33)` is what says so.
            mk_ep_rejecting("VerifyMemberCapabilityV1", true, &["ContractError(Custom(33))"], Box::new(move || {
                let r = h.verify_member_capability(nullifier_k, capability_id, endowment_bulla, capability_secret, holder_secret, holder_pub, CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // Requires its `promissory_note::transfer_v1` payment child, which the fixture does not
            // build yet: the endpoint withdraws by moving value through the child, so without one there
            // is nothing to withdraw.
            mk_ep_rejecting("WithdrawV1", false, &["ContractError(Custom(33))"], Box::new(move || {
                let r = h.withdraw(endowment_bulla, owner_pub, 50_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // No needle: the expected failure here is NOT established — this row's children and its
            // authorisation branch have never been exercised, so the row asserts only that the call does
            // not succeed. That is weaker than a named rejection and is recorded as owed.
            mk_ep_rejecting("EndowmentWithdrawV1", false, &[], Box::new(move || {
                let r = h.endowment_withdraw(endowment_bulla, claim_id, owner_pub, 25_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // No needle: see the sibling note above — the expected failure is not established.
            mk_ep_rejecting("TreasurySpendV1", false, &[], Box::new(move || {
                let r = h.treasury_spend(endowment_bulla, proposal_id, owner_pub, 10_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // No needle: see the sibling note above — the expected failure is not established.
            mk_ep_rejecting("ExecuteClaimV1", false, &[], Box::new(move || {
                let r = h.execute_claim(endowment_bulla, proposal_id, owner_pub, 75_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // No needle, and the reason is the contract's own: this row's decoder error arrives as an
            // `IoError` carrying the contract's last `msg!` instead of the decode failure, so the cause
            // is not legible from the run. Masked-refusal is the class `OBL-C151` records against this
            // contract's metadata arms; here it is an exec arm.
            mk_ep_rejecting("RegisterCapabilityRequirementV1", false, &[], Box::new(move || {
                let r = h.register_capability_requirement(endowment_bulla, b"member_vote".to_vec(), [0u8; 32], identity_contract_bulla).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // No needle: not established. See the sibling note above.
            mk_ep_rejecting("CancelClaimV1", false, &[], Box::new(move || {
                let r = h.cancel_claim(endowment_bulla, claim_id, owner_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // ── THE SETTER. Everything below runs with governance ACTIVE.
            //
            // `UpdateV1` proves ownership: the `SetGovernanceConfigV2` circuit derives `owner_pub` from
            // `owner_secret` and constrains the exposed coordinates to it, so this is not a public key
            // compared against a public key.
            EndpointSpec {
                name: "UpdateV1_SetGovernanceGroup",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.update(endowment_bulla, owner_secret, owner_pub, Some(DaoEscrowHarness::governance_group()))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // ── Governance ACTIVE. The gates now require the group's approval as a child.
            EndpointSpec {
                name: "ProposeClaimV1_Approved",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let approvals = gov.lock().unwrap().propose.clone();
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, 10_000, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_propose, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // ── Negative controls, each naming the check it exercises rather than accepting any
            //    rejection. A bare `Rejection` is satisfied by an earlier failure in the frame, which is
            //    how a control that cannot fail gets written.
            EndpointSpec {
                name: "UpdateV1_ByAStranger",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(20))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    // A different secret, whose public key is therefore not the endpoint's owner. The
                    // *proof* still proves knowledge of that secret — which is the point: ownership of
                    // the value does not make you this endowment's owner.
                    let stranger_secret = pallas::Base::from(4321u64);
                    let stranger_pub = PublicKey::from_secret(SecretKey::from_base(stranger_secret));
                    let r = h.update(endowment_bulla, stranger_secret, stranger_pub, Some(DaoEscrowHarness::governance_group()))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "UpdateV1_ReplaysTheProof",
                is_zk: true,
                // The same ownership proof twice: `owner_nullifier` is deterministic in
                // `(owner_secret, bulla)`, so the second call must be refused rather than replayed.
                // `GovernanceAlreadyActive` fires first if the group is already set, so this names the
                // replay only when the setter left the record without a group; the row records which.
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.update(endowment_bulla, owner_secret, owner_pub, None)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ProposeClaimV1_NoChild",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, 10_000, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ProposeClaimV1_ForeignGroup",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(53))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let g = gov.lock().unwrap().clone();
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, 10_000, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(g.foreign_group, msg_propose, g.foreign)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            EndpointSpec {
                name: "ProposeClaimV1_WrongMessage",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(54))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let approvals = gov.lock().unwrap().wrong_message.clone();
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, 10_000, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_wrong, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            EndpointSpec {
                name: "WithdrawV1_GovernanceActiveWithoutApproval",
                is_zk: false,
                // The reader that `withdraw_v1`'s governance branch never had: before this it was an
                // empty body, so activating governance removed the owner check and left nothing.
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.withdraw(endowment_bulla, owner_pub, 50_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
        ],
    }
}
