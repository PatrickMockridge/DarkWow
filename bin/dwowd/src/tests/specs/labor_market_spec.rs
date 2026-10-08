//! ContractTestSpec for labor_market. Tier: HARVESTABLE — 9 harness methods, all ZK.
use dwow_contract_test_harness::harness::{
    AttestationHarness, ContractHarness, DaoEscrowHarness, IdentityHarness, LaborMarketHarness,
    MultiSigHarness, PromissoryNoteHarness,
};
use dwow_dao_escrow_contract::model::{governance_message, governance_role, DaoEscrowMode};
use dwow_identity_contract::model::{CapabilityId, CredentialRequirement};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    pasta_prelude::PrimeField, poseidon_hash, util::fp_mod_fv, AssetId, Blind, IntentNullifier,
    MerkleNode, MerkleTree, Nullifier, PublicKey, SecretKey, ATTESTATION_CONTRACT_ID,
    IDENTITY_CONTRACT_ID, MULTISIG_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::{ChildCall, EndpointResult, EndpointSpec, EndpointExpectation};
use crate::tests::uniform_runner::*;
use super::helpers::{mk_ep, mk_ep_rejecting_naming};

/// `(commitment, leaf position, merkle path, asset id, commitment blind)` — copied from
/// `insurance_market_spec.rs:69`, which copies it from `escrow_spec.rs`.
type PnNote = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// The job's payment, and therefore the value the promissory-note child must move.
const PAYMENT: u64 = 5000;

/// The secret every note is issued under and every transfer child spends with — **one constant,
/// because two callers have to agree on it and the disagreement is silent**.
///
/// The transfer proof rebuilds the spent leaf as `poseidon_hash([7, secret])`
/// (`promissory_note/src/client/transfer.rs:353`), so a note issued under any other secret
/// recomputes a root no recorded root contains and the child is refused `Custom(13)` before the
/// parent's guard is reached. Until 2026-09-29 this value was written out three times — in
/// `pn_transfer_child`, in `setup`'s issuance, and in what `OBL-C191`'s unit 2 later made
/// `child_blind` derive a nullifier from. It is two now: reverting that unit took the nullifier term
/// out of `child_blind` and with it the secret's third reader. Two literals that must be equal is one
/// chance fewer for drift, and the two that remain are the two that must agree.
const PN_SECRET: pallas::Base = pallas::Base::from_raw([100, 0, 0, 0]);

/// The blind every **escrow-bearing** path in this contract derives for its child:
/// `poseidon_hash([payment_amount, job_id])` — the fixture's copy of the contract's own seed, and the
/// record-keyed form the contract carries.
///
/// **It carried the spent note's nullifier as a third term for part of 2026-09-29 and no longer
/// does.** `OBL-C191`'s unit 2 added it, on the reading that a record-keyed seed made a second
/// same-valued movement impossible; reverting that unit took it out, and the reading with it. It is
/// not: `promissory_note` refuses a repeated **leaf**, whose blind is the caller-chosen
/// `TransferCallOutput.commitment_blind`, independent of the value blind any seed produces
/// (`OBL-C192`). So there is **no caller cost here** — reproducing this seed needs the amount and the
/// job id, both of which the params carry — and the obligation to give each child a distinct leaf
/// sits in `pn_transfer_child`, which derives it from the note the child spends.
///
/// The three **payout** rows do not use it: their endpoints key on `params.spent_nullifier`, a
/// per-call value their own proofs supply — the same rule reached from the other side.
fn child_blind(value: u64, job_id: pallas::Base) -> pallas::Base {
    poseidon_hash([
        pallas::Base::from(value),
        job_id,
    ])
}

/// What one milestone releases, and the value `ConfirmMilestoneV1` moves.
///
/// It reaches the harness twice — `confirm_milestone`'s `milestone_payment_amount` is the circuit's
/// instance 5 witness and its `payment_release` is the value the metadata arm publishes in that slot —
/// and the two must be equal or the proof is rejected before the contract sees the call. Naming them
/// as one constant is what keeps them equal.
const MILESTONE_PAYMENT: u64 = 1000;

/// The milestones job's total, which the create's escrow child must commit. Two milestones, because
/// `confirm_milestone_v1` is driven at index 1 and needs `milestone_index < milestone_count`.
const MS_JOB_PAYMENT: u64 = MILESTONE_PAYMENT * 2;

/// Far future, for the milestone deadlines. Nothing in this fixture reads a milestone deadline —
/// `MilestoneDeadlineNotReached` has no live caller — so it is written as a value that cannot be
/// reached rather than as a block this run will cross.
const MILESTONE_DEADLINE: u64 = 1_000_000;

/// The hand-encoded `CreateJobWithMilestonesV1` (0x08) call: selector plus
/// `CreateJobWithMilestonesParamsV1::encode()`.
///
/// **Hand-encoded because no client can build it, and the proof is borrowed rather than invented.**
/// `client/` carries `create_job.rs` and no `create_job_with_milestones.rs`, which is `OBL-C170`'s
/// recorded cell — but that cell says "a proof no client can build", and the endpoint *dispatches to
/// the `CreateJobV2` circuit* (`entrypoint.rs:362-380` publishes that circuit's five instances), so
/// `create_job_v1_proof` is exactly the proof this endpoint verifies. The five values therefore come
/// from that proof's own public inputs; writing them by hand would be five constants the host
/// compares against the proof and rejects the row for, which reads as a contract failure.
///
/// Returned with the proof because the runner submits both: `is_zk` is true for this endpoint.
fn milestones_create_call(
    h: &LaborMarketHarness,
    employer_secret: pallas::Base,
    employer_pub: PublicKey,
    attestation_id: pallas::Base,
    job_id: pallas::Base,
    payment_amount: u64,
    milestones: Vec<dwow_labor_market_contract::model::Milestone>,
) -> dwow_core::Result<(Vec<u8>, dwow_core::zk::Proof)> {
    let r = h.create_job(
        employer_secret, employer_pub, attestation_id, job_id, 0, payment_amount,
        pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64),
    ).map_err(|e| dwow_core::Error::Custom(format!("create_job (milestones proof): {e}")))?;
    let milestone_count = u32::try_from(milestones.len())
        .map_err(|_| dwow_core::Error::Custom("too many milestones for a u32 count".into()))?;
    let params = dwow_labor_market_contract::model::CreateJobWithMilestonesParamsV1 {
        proof: r.proof.as_ref().to_vec(),
        job_id,
        employer_pub_x: r.public_inputs.employer_pub_x,
        employer_pub_y: r.public_inputs.employer_pub_y,
        attestation_id: r.public_inputs.attestation_id,
        delivery_type: 0,
        payment_amount,
        payment_token: pallas::Base::from(1u64),
        payment_commit_x: pallas::Base::from(2u64),
        payment_commit_y: pallas::Base::from(3u64),
        deadline_block: MILESTONE_DEADLINE,
        milestone_count,
        milestones,
        tx_binding: r.public_inputs.tx_binding,
        tx_nonce: r.public_inputs.tx_nonce,
    };
    let mut call_data = vec![0x08u8];
    call_data.extend_from_slice(&params.encode()
        .map_err(|e| dwow_core::Error::Custom(format!("encode: {e}")))?);
    Ok((call_data, r.proof))
}

/// The milestone list both milestone creates use: `count` milestones of `MILESTONE_PAYMENT`.
fn milestones_of(count: u32) -> Vec<dwow_labor_market_contract::model::Milestone> {
    (0..count).map(|i| dwow_labor_market_contract::model::Milestone {
        index: i,
        payment_amount: MILESTONE_PAYMENT,
        deadline_block: MILESTONE_DEADLINE,
        completed: false,
        completed_at_block: None,
    }).collect()
}

/// Far future, as `insurance_market_spec.rs:139` has it: an expiry the fixture never reaches.
const EXPIRES_AT: u64 = 1_000_000;

/// The job `accept_job_with_capability_v1` takes, and the identity material behind it — copied from
/// `insurance_market_spec.rs:163-240`, which is the tree's only exercised credential-and-capability
/// setup. A struct rather than a tuple of eight, because the rows read it by name.
#[derive(Clone)]
struct CapSetup {
    cap_a: CapabilityId,
    cap_b: CapabilityId,
    schema_a: pallas::Base,
    schema_b: pallas::Base,
    credential_secret_a: pallas::Base,
    credential_secret_b: pallas::Base,
    capability_secret: pallas::Base,
    attribute_blind: pallas::Base,
    issuer_secret: pallas::Base,
}

/// The `promissory_note::transfer_v1` child `create_job_v1` requires.
///
/// Copied from `insurance_market_spec.rs:77` (itself copied from `escrow_spec.rs`), including the
/// detail that matters: the **output's** `value` and `commitment_blind` are the ones the parent
/// re-derives, because `validate_child_value_commit` compares the output's commitment against
/// `pedersen_commitment_u64(payment_amount, value_blind)` where `value_blind` is
/// `poseidon_hash([payment_amount, job_id])` (`labor_market/src/entrypoint.rs`, `create_job_v1`).
fn pn_transfer_child(note: &PnNote, value: u64, blind_seed: pallas::Base) -> dwow_core::Result<ChildCall> {
    let (note_commitment, pos, path, asset_id, commitment_blind) = note;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());
    let input = TransferCallInput {
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: *commitment_blind,
        leaf_position: *pos,
        merkle_path: path.clone(),
        // **100, and it must be**: the transfer proof rebuilds the leaf as
        // `poseidon_hash([7, secret])` (`promissory_note/src/client/transfer.rs:353`), so a note
        // issued under any other secret recomputes a root that no recorded root contains and the
        // child is rejected with `Custom(13)` before the parent's guard is reached. `PN_SECRET` is
        // the same constant `setup` issues under and `child_blind` derives the note's nullifier from.
        secret: PN_SECRET,
        ephemeral_signature_secret: pallas::Base::from(9u64),
        tx_commitment: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };
    let output = TransferCallOutput {
        recipient: poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(200u64)]),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(200u64))),
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: poseidon_hash([blind_seed, *note_commitment]),
    };
    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .transfer_with_value_blinds(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child.call_data, proofs: child.proofs, children: vec![] })
}

/// The `attestation::CheckAttestationV1` child `create_job_v1` requires — **and this one needs no
/// proof.** `CheckAttestationParamsV1::encode` is the id's 32 bytes and nothing else
/// (`attestation/src/model/mod.rs:807`), so the call is the selector plus the id, and the manifest
/// gives the function no `requires_proof`. The parent only checks that the child's
/// `attestation_id` equals its own `params.attestation_id`.
fn attestation_child(attestation_id: pallas::Base) -> ChildCall {
    let mut call_data = vec![0x0du8];
    call_data.extend_from_slice(attestation_id.to_repr().as_ref());
    ChildCall { contract_id: *ATTESTATION_CONTRACT_ID, call_data, proofs: vec![], children: vec![] }
}

/// The `attestation::VerifyClaimV1` child `submit_deliverable_v1` requires — and this one **does**
/// need a proof, so it cannot be hand-encoded the way `attestation_child` is.
///
/// Ported from `attestation_spec.rs:99`, which is the working example and passes the same placeholder
/// witnesses; the parent checks only the selector and the contract id, so the child's own meaning is
/// the attestation contract's business.
fn verify_claim_child(claim_id: pallas::Base, attestation_id: pallas::Base) -> dwow_core::Result<ChildCall> {
    let att = AttestationHarness::spawn(*ATTESTATION_CONTRACT_ID);
    let r = att.verify_claim(
        claim_id, attestation_id,
        pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64),
        pallas::Base::from(4u64), pallas::Base::from(5u64), [pallas::Base::from(0u64); 255],
        pallas::Base::from(6u64),
    ).map_err(|e| dwow_core::Error::Custom(format!("verify_claim: {e}")))?;
    Ok(ChildCall {
        contract_id: *ATTESTATION_CONTRACT_ID, call_data: r.call_data, proofs: vec![r.proof],
        children: vec![],
    })
}

/// The `identity::VerifyCapabilityV1` child both capability rows require — and this one **does** need
/// a proof, so it cannot be hand-encoded the way `attestation_child` is.
///
/// Ported from `insurance_market_spec.rs:411-419`; the parent checks the selector, the contract id
/// and the decoded `capability_proof.capability_id`, so which capability the child proves is the one
/// thing the two callers vary.
/// `OBL-C198`: the child is **prepared** here and proven by the caller, after the parent's call
/// data exists — the commitment is a derivation over the whole ordered call set, and the child's
/// proof must bind to the same value its parent does. `identity` grew the split for exactly this.
fn capability_child_prepare(
    s: &CapSetup,
    credential_secret: pallas::Base,
    capability_id: pallas::Base,
    schema: pallas::Base,
) -> dwow_core::Result<(
    dwow_sdk::tx::ContractCall,
    dwow_contract_test_harness::harness::identity::VerifyCapabilityPlan,
)> {
    let id = IdentityHarness::spawn(*IDENTITY_CONTRACT_ID);
    let holder = PublicKey::from_secret(SecretKey::from_base(credential_secret));
    let plan = id.verify_capability_prepare(
        credential_secret, capability_id,
        pallas::Base::from(50u64),
        b"role", pallas::Base::from(100u64),
        b"tenure", pallas::Base::from(200u64),
        s.attribute_blind, s.capability_secret,
        PublicKey::from_secret(SecretKey::from_base(s.issuer_secret)),
        holder, schema, 0, EXPIRES_AT, true)
        .map_err(|e| dwow_core::Error::Custom(format!("verify_capability: {e}")))?;
    let call = dwow_sdk::tx::ContractCall { contract_id: *IDENTITY_CONTRACT_ID, data: plan.call_data.clone() };
    Ok((call, plan))
}

pub fn labor_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(LaborMarketHarness::spawn()));
    let h: &LaborMarketHarness = harness;
    // Leaked, and for the reason `dao_escrow_spec.rs:217-221` gives: `spawn` rebuilds proving keys, and
    // this fixture's `setup` runs twice — once per chain — while the rows build children from both.
    // The DAO-Escrow harness is here because this fixture deploys and drives a second contract (see the
    // `DisputeV1` rows), which no other fixture in the tree does.
    // `OBL-C198`: the harness needs the id its calls carry, because the commitment covers it.
    let dao_cid_h = crate::tests::blockchain::derive_contract_id_from_name("dao_escrow");
    let dao: &'static DaoEscrowHarness = Box::leak(Box::new(DaoEscrowHarness::spawn(dao_cid_h)));
    let ms: &'static MultiSigHarness = Box::leak(Box::new(MultiSigHarness::spawn()));
    let wasm = include_bytes!("../../../../../src/contract/labor_market/dwow_labor_market_contract.wasm");
    let employer_secret = pallas::Base::from(10u64);
    let employer_pub = PublicKey::from_secret(SecretKey::from_base(employer_secret));
    let worker_secret = pallas::Base::from(20u64);
    let worker_pub = PublicKey::from_secret(SecretKey::from_base(worker_secret));
    // ── The `dao_escrow` endowment `DisputeV1` rides on (`OBL-C170`'s second bound) ──
    //
    // **Every constant here is `dao_escrow_spec.rs`'s, copied rather than invented**, because the
    // governance messages are derived by the *contract's own* functions and any value that drifted
    // would present as a governance refusal rather than as a fixture typo. The endowment's bulla is
    // the derived one — `initialize_v1` derives it from the DAO's own bulla, the owner, the asset and
    // the blind, while every endpoint that touches the endowment looks it up by its own
    // `dao_escrow_bulla` field, so the value that has to be passed is the derived one.
    let owner_secret = pallas::Base::from(12345u64);
    let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
    let dao_bulla = pallas::Base::from(1u64);
    let endowment_asset_id = pallas::Base::from(42u64);
    let bulla_blind = pallas::Base::from(9999u64);
    let nullifier_k = pallas::Scalar::from(1u64);
    let dao_capability_id = pallas::Base::from(999u64);
    let dao_capability_secret = pallas::Base::from(888u64);
    let dao_proposer_secret = pallas::Base::from(777u64);
    let endowment_bulla = dwow_dao_escrow_contract::model::DaoEscrow::derive_bulla(
        dwow_dao_escrow_contract::model::DaoEscrowBulla(dao_bulla),
        &owner_pub,
        AssetId::from_base(endowment_asset_id),
        Blind(bulla_blind),
    ).inner();
    // The claim `DisputeV1`'s child proposes, and the message its group approves. Its own id, because a
    // proposal is one-shot (`ClaimAlreadyExists`) and this fixture runs twice — once per chain — with
    // the same ids.
    let dispute_claim_id = pallas::Base::from(205u64);
    let dispute_proposal_blind = pallas::Base::from(10u64);
    // The one message the endowment's group signs. Computed with the contract's own derivation so the
    // message signed and the message checked cannot disagree.
    let msg_propose = governance_message(governance_role::PROPOSE_CLAIM, dispute_claim_id);

    let job_id = pallas::Base::from(100u64);
    // **A second job, because the capability row cannot take the first one.** A job that
    // `AcceptJobV1` has already accepted has a worker and is `InProgress`, and
    // `accept_job_with_capability_v1` refuses both (`entrypoint.rs:1857-1863`) — so the capability
    // pair needs a job of its own, created by the only endpoint that can set a requirement.
    let cap_job_id = pallas::Base::from(101u64);
    // **And a third job, for the same shape one row over.** `SubmitDeliverableV1` and
    // `SubmitGitDeliverableV1` each require `JobState::InProgress` and each set `Delivered`
    // (`entrypoint.rs:792`/`:804` and `:866`/`:878`), so pairing them on one job makes the second
    // refuse `InvalidStateTransition` (`Custom(2)`) — measured on this fixture's own log at block 21.
    // It is the second-claim lesson again, one object over: **a one-shot transition needs an object of
    // its own**, and a fixture that shares one is testing less than it reads as testing.
    let job_id_git = pallas::Base::from(202u64);
    // **A fourth job, because `CancelJobV1` needs one that is still `Created`.** `cancel_job_v1`
    // refuses every state but `Created` (`entrypoint.rs:1287`), and each of the three jobs above is
    // moved out of it by a row that has to run for a different reason: `job_id` is accepted, delivered
    // and confirmed; `job_id_git` is delivered and refunded; `cap_job_id` is accepted with its
    // capability. A cancel row sharing any of them would be refused `InvalidStateTransition`
    // (`Custom(2)`) for a reason that says nothing about the seed this row exists to exercise.
    let cancel_job_id = pallas::Base::from(203u64);
    // **The milestones job, and its own accept and deliverable rows.** `ConfirmMilestoneV1` needs a
    // job that *has* milestones (`entrypoint.rs:1644-1647`, `job.milestones.is_empty()`), that is
    // `Delivered` (`:1648`), and whose milestone index is in range — and the only endpoint that can
    // give a job milestones is `create_job_with_milestones_v1`, so the job has to be built by this
    // fixture rather than borrowed. Two milestones of `MILESTONE_PAYMENT` each, so the job's
    // `payment_amount` is their sum and `confirm_milestone_v1` at index 1 is the last one.
    let ms_job_id = pallas::Base::from(102u64);
    // The capability twin's job, for `OBL-C190`'s other repaired endpoint. One milestone, because
    // nothing in this fixture confirms it — the row exists to drive the create's child check.
    let ms_cap_job_id = pallas::Base::from(103u64);
    // `SubmitDeliverableV1_Milestones`'s claim. A third claim, because a claim can be verified once:
    // the two above are each consumed by a deliverable row on another job.
    let claim_id_ms = pallas::Base::from(204u64);
    let claim_id = pallas::Base::from(200u64);
    // `SubmitGitDeliverableV1` needs its own claim — see the setup's note.
    let claim_id_git = pallas::Base::from(201u64);
    let attestation_id = pallas::Base::from(1u64);
    let cap_proof = vec![0u8; 32];
    let cap_secret = [0u8; 32];

    // **The note the `create_job` child spends, issued once in `setup` and carried to the row.**
    //
    // The fixture passed `children: vec![]` for all ten rows, and `create_job_v1` requires two — so
    // the *first* row was refused before any of the contract's own logic ran. Measured:
    // `[create_job_v1] Error: Expected 2 child calls (promissory_note::transfer_v1 0x04 and
    // attestation::CheckAttestationV1 0x0d), got 0` → `InvalidChildrenIndexes` (`Custom(31)`).
    //
    // The issue has to happen on-chain, so it cannot be done when the spec is built — hence a `setup`
    // and a cell, the shape `insurance_market_spec.rs:156` demonstrates.
    let note_cell: Arc<Mutex<Option<PnNote>>> = Arc::new(Mutex::new(None));

    // **Three more notes, because a note is spent once.** `ConfirmDeliveryV1`, `RefundV1` and
    // `ConfirmMilestoneV1` each require a `promissory_note::transfer_v1` child, and each child spends
    // a *distinct* note — sharing one makes the second fail its nullifier check, the same lesson the
    // two attestation claims above taught in the previous increment. They are issued in `setup` for
    // the same reason the first one is: issuing is an on-chain operation, and a row's closure only
    // builds call data.
    let more_notes: Arc<Mutex<Vec<PnNote>>> = Arc::new(Mutex::new(Vec::new()));

    // The identity material the capability rows need, written by `setup` and read by
    // `CreateJobWithCapabilityV1` and `AcceptJobWithCapabilityV1`.
    let caps: Arc<Mutex<Option<CapSetup>>> = Arc::new(Mutex::new(None));

    // The endowment's group's approval of the proposal `DisputeV1`'s child makes. A `Vec` rather than an
    // `Option` because a set of approvals is what the finalize child *names*; empty means `setup` did
    // not run, and the row says so rather than building a child that would be refused for a different
    // reason.
    let dao_approvals: Arc<Mutex<Vec<Nullifier>>> = Arc::new(Mutex::new(Vec::new()));

    // **The deploy payload, and it is load-bearing twice over.** `init_contract` takes its two
    // cross-contract ids from here when the payload is non-empty (`entrypoint.rs:114-131`), and the
    // DAO-Escrow id has no other source: the contract is not genesis, so there is no constant to default
    // to, and this fixture deploys it (`setup`, below) at exactly the id this derivation produces.
    //
    // Passing a payload also moves `init_contract` off its empty-ix branch, which is where Attestation's
    // id would otherwise have been seeded — so the payload carries it too. That is the trap the register
    // names: a `create_job` row failing `IoError` is how an attestation regression presents, because the
    // two ids are decoded as one tuple and a payload carrying only the second would shift them.
    let deploy_ix = dwow_serial::serialize(&(
        *ATTESTATION_CONTRACT_ID,
        crate::tests::blockchain::derive_contract_id_from_name("dao_escrow"),
    ));

    ContractTestSpec {
        name: "labor_market", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({ let cell = note_cell.clone(); let more = more_notes.clone(); let caps = caps.clone(); let dao_approvals = dao_approvals.clone(); move |chain| {
            let oh = |e: String| dwow_core::Error::Custom(e);
            let att_cid = *ATTESTATION_CONTRACT_ID;
            let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;

            // ── The attestation the job names. `create_job_v1` requires the child's
            // `attestation_id` to equal its own `params.attestation_id`, and requires the child at
            // all, so the attestation must exist before the row runs. ──
            let att = AttestationHarness::spawn(*ATTESTATION_CONTRACT_ID);
            let attestor_secret = pallas::Base::from(30u64);
            let attestor_pub = PublicKey::from_secret(SecretKey::from_base(attestor_secret));
            let a = att.create_attestation(
                attestor_secret, attestor_pub,
                dwow_attestation_contract::model::Predicate::GreaterOrEqual,
                vec![pallas::Base::from(50u64)], b"labor".to_vec(), None, attestation_id,
            ).map_err(|e| oh(format!("create_attestation: {e}")))?;
            smol::block_on(chain.block()?.with_call(att_cid, &att, &a.call_data, vec![a.proof.clone()])?.submit())?;

            // ── The claim `submit_deliverable_v1`'s child verifies. Its row requires an
            // `attestation::VerifyClaimV1` child, and a verify needs a claim to verify, so the claim
            // is created here rather than in the row — the row's job is to present the child, not to
            // build the state behind it. `claim_id` is the spec's own constant (200). ──
            let claimant_secret = pallas::Base::from(40u64);
            let claimant_pub = PublicKey::from_secret(SecretKey::from_base(claimant_secret));
            let cl = att.create_claim(
                attestation_id, claimant_secret, claimant_pub,
                dwow_attestation_contract::model::Predicate::GreaterOrEqual,
                pallas::Base::from(2u64).to_repr().to_vec(), b"result".to_vec(), claim_id,
            ).map_err(|e| oh(format!("create_claim: {e}")))?;
            smol::block_on(chain.block()?.with_call(att_cid, &att, &cl.call_data, vec![cl.proof.clone()])?.submit())?;

            // **A second claim, because a claim can be verified once.** `SubmitDeliverableV1` and
            // `SubmitGitDeliverableV1` each require an `attestation::VerifyClaimV1` child, and sharing
            // one claim makes the second child fail `verify_claim_v1 ERROR: Claim not pending` — the
            // first row's verification moved it out of `Pending`. Measured on the fixture's own log,
            // and it is why the two rows take different ids.
            let cl2 = att.create_claim(
                attestation_id, claimant_secret, claimant_pub,
                dwow_attestation_contract::model::Predicate::GreaterOrEqual,
                pallas::Base::from(2u64).to_repr().to_vec(), b"result".to_vec(), claim_id_git,
            ).map_err(|e| oh(format!("create_claim (git): {e}")))?;
            smol::block_on(chain.block()?.with_call(att_cid, &att, &cl2.call_data, vec![cl2.proof.clone()])?.submit())?;

            // **A third claim, and the same lesson a third time.** The milestones job's deliverable row
            // requires a `VerifyClaimV1` child of its own, and both claims above are already spent by
            // the two deliverable rows — a verified claim is no longer `Pending`, so sharing one makes
            // this child fail on the attestation contract rather than the row testing what it says.
            let cl3 = att.create_claim(
                attestation_id, claimant_secret, claimant_pub,
                dwow_attestation_contract::model::Predicate::GreaterOrEqual,
                pallas::Base::from(2u64).to_repr().to_vec(), b"result".to_vec(), claim_id_ms,
            ).map_err(|e| oh(format!("create_claim (milestones): {e}")))?;
            smol::block_on(chain.block()?.with_call(att_cid, &att, &cl3.call_data, vec![cl3.proof.clone()])?.submit())?;

            // ── The promissory note, worth exactly the job's payment, issued under the secret the
            // transfer child spends with. ──
            let pn = PromissoryNoteHarness::spawn();
            let pn_secret = PN_SECRET;
            let owner_addr = poseidon_hash([pallas::Base::from(7u64), pn_secret]);
            let token = pn.register_type(
                pn_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr,
                PAYMENT, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64),
            ).map_err(|e| oh(format!("register_type: {e}")))?;
            smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token.call_data, token.token_proofs.clone())?.submit())?;
            let asset_id = token.asset_id;

            // A local mirror of the contract's own tree, so the child's Merkle path is the tree the
            // contract stores. Position 0 is the zero seed; the token sits at 1.
            let mut tree = MerkleTree::new(1);
            tree.append(MerkleNode::from_base(pallas::Base::zero()));
            tree.append(MerkleNode::from_base(token.commitment.inner()));
            let mark_token = tree.mark().ok_or_else(|| oh("tree.mark (token)".into()))?;

            let n1 = pn.issue(
                pn_secret, asset_id, owner_addr, PAYMENT,
                pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64),
            ).map_err(|e| oh(format!("issue: {e}")))?;
            smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n1.call_data, n1.proofs.clone())?.submit())?;
            tree.append(MerkleNode::from_base(n1.commitment.inner()));
            let _mark_n1 = tree.mark().ok_or_else(|| oh("tree.mark (note)".into()))?;

            let path = tree.witness(mark_token, 0).map_err(|e| oh(format!("tree.witness: {e:?}")))?;
            *cell.lock().map_err(|_| oh("note cell poisoned".into()))? = Some((
                token.commitment.inner(), u64::from(mark_token), path, asset_id,
                pallas::Base::from(6u64),
            ));

            // ── Three more notes of the same asset, one per promissory-note-consuming row ──
            //
            // The mark is taken immediately before the append, which is the convention the note above
            // establishes: `mark()` is the index the next appended leaf will take, and `witness(mark,
            // 0)` resolves that index against the tree as it stands when the call is made. Appending
            // does not invalidate the earlier paths — `commitment_roots` retains every historical root,
            // which is the property `insurance_market_spec.rs:275-279` records having mis-attributed
            // once already.
            for (amount, blind_seed) in [
                (PAYMENT, 11u64), (2500u64, 12u64), (1000u64, 13u64),
                (PAYMENT, 14u64), (PAYMENT, 15u64),
                // Indices 5 and 6, for `CancelJobV1`'s pair: the note its *create* row escrows with,
                // and the note its *cancel* row refunds into. Two, because a note is spent once — the
                // create's deposit and the cancel's payout are the two halves of the collision this
                // row exists to prove is gone, so they cannot share one.
                (PAYMENT, 16u64), (PAYMENT, 17u64),
                // Index 7 escrows the milestones job — a note worth the *sum* of its milestones, which
                // is what `create_job_with_milestones_v1` makes its child commit. Index 8 does the same
                // for the capability twin.
                (MS_JOB_PAYMENT, 18u64), (MILESTONE_PAYMENT, 19u64),
            ] {
                let blind = pallas::Base::from(blind_seed);
                let n = pn.issue(
                    pn_secret, asset_id, owner_addr, amount,
                    pallas::Base::zero(), pallas::Base::zero(), blind,
                ).map_err(|e| oh(format!("issue (extra): {e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                // **The mark is taken AFTER the append, the blind stored is the one the note was
                // ISSUED with, and the amount is the one the note HOLDS — all three corrected from a
                // run.** `mark()` returns the index of the *last appended* leaf, not the next one the
                // tree will take: the block above appends the token, marks, appends the note, and then
                // witnesses the mark — so what it witnesses is the token at index 1, and the blind it
                // stores (6) is `register_type`'s, not the note's (8). That is self-consistent, and it
                // is why the note issued beside it is never spent.
                //
                // **And the amount is the note's own value, not the value the child moves**, which is
                // read off the transfer client rather than guessed: it rebuilds the input leaf as
                // `CapAttrs { public_key = poseidon_hash([7, secret]), value, asset_id, spend_hook,
                // user_data, blind = commitment_blind }` (`client/transfer.rs:355-362`), and the parent
                // checks only the child's *output* commitment. So a note worth 5000 cannot be the input
                // to a transfer of 2500, and each note below is issued for the amount its row moves.
                // Mirroring the block above the obvious-looking way round put every extra note one leaf
                // off and at the wrong value; `promissory_note` refused the transfer with `Custom(13)`
                // at the fixture's block 20.
                tree.append(MerkleNode::from_base(n.commitment.inner()));
                let mark = tree.mark().ok_or_else(|| oh("tree.mark (extra)".into()))?;
                let p = tree.witness(mark, 0).map_err(|e| oh(format!("tree.witness (extra): {e:?}")))?;
                more.lock().map_err(|_| oh("extra-notes cell poisoned".into()))?.push((
                    n.commitment.inner(), u64::from(mark), p, asset_id, blind,
                ));
            }

            // ── Identity: an issuer, two credentials (two schemas) and two capabilities ──
            //
            // Copied from `insurance_market_spec.rs:177-240`, which is the tree's only exercised
            // version of this setup. Two capabilities rather than one, and they differ by **schema**:
            // `compute_capability_id` hashes the requirement's first eight bytes and the requirement
            // begins with `schema_hash`, so two capabilities registered under the same schema are the
            // *same* capability however they are named — and a wrong-capability control built from a
            // second name would be the same id and test nothing.
            let id_cid = *IDENTITY_CONTRACT_ID;
            let issuer_secret = pallas::Base::from(10u64);
            let credential_secret_a = pallas::Base::from(20u64);
            let credential_secret_b = pallas::Base::from(21u64);
            let schema_a = pallas::Base::from(30u64);
            let schema_b = pallas::Base::from(31u64);
            let attribute_blind = pallas::Base::from(300u64);
            let capability_secret = pallas::Base::from(777u64);
            let issuer_pub = PublicKey::from_secret(SecretKey::from_base(issuer_secret));

            let id = IdentityHarness::spawn(*IDENTITY_CONTRACT_ID);
            let issuer = id.register_issuer(issuer_pub, b"employer".to_vec(), vec![])
                .map_err(|e| oh(format!("register_issuer: {e}")))?;
            smol::block_on(chain.block()?.with_call(id_cid, &id, &issuer.call_data, vec![])?.submit())?;

            let cred_a = id.issue_credential(&[], issuer_secret, credential_secret_a,
                b"role", pallas::Base::from(100u64),
                b"tenure", pallas::Base::from(200u64),
                attribute_blind, schema_a, 0, EXPIRES_AT)
                .map_err(|e| oh(format!("issue_credential A: {e}")))?;
            let cred_b = id.issue_credential(&[], issuer_secret, credential_secret_b,
                b"role", pallas::Base::from(100u64),
                b"tenure", pallas::Base::from(200u64),
                attribute_blind, schema_b, 0, EXPIRES_AT)
                .map_err(|e| oh(format!("issue_credential B: {e}")))?;
            smol::block_on(chain.block()?.with_call(id_cid, &id, &cred_a.call_data, vec![cred_a.proof.clone()])?.submit())?;
            smol::block_on(chain.block()?.with_call(id_cid, &id, &cred_b.call_data, vec![cred_b.proof.clone()])?.submit())?;

            let reg_a = id.register_capability(b"worker_licence".to_vec(),
                CredentialRequirement {
                    schema_hash: schema_a.to_repr(), issuer_pub,
                    min_threshold: 1, attribute_name: b"role".to_vec(),
                }, None).map_err(|e| oh(format!("register_capability A: {e}")))?;
            let reg_b = id.register_capability(b"worker_licence".to_vec(),
                CredentialRequirement {
                    schema_hash: schema_b.to_repr(), issuer_pub,
                    min_threshold: 1, attribute_name: b"role".to_vec(),
                }, None).map_err(|e| oh(format!("register_capability B: {e}")))?;
            let cap_a = reg_a.capability_id;
            let cap_b = reg_b.capability_id;
            if cap_a.inner() == cap_b.inner() {
                return Err(oh("the two capabilities share an id, so this fixture cannot \
                    distinguish them — the schemas must differ".into()))
            }
            smol::block_on(chain.block()?.with_call(id_cid, &id, &reg_a.call_data, vec![])?.submit())?;
            smol::block_on(chain.block()?.with_call(id_cid, &id, &reg_b.call_data, vec![])?.submit())?;

            // Issuance. `verify_capability` loads the capability *definition* rather than this record,
            // so these are fidelity rather than a precondition — said because a reader would otherwise
            // assume they are load-bearing.
            for (cid_cap, secret, commitment) in [
                (cap_a, credential_secret_a, cred_a.public_inputs.commitment),
                (cap_b, credential_secret_b, cred_b.public_inputs.commitment),
            ] {
                let nf = IntentNullifier::from_base(poseidon_hash([
                    pallas::Base::from(1u64), secret, commitment,
                ]));
                let iss = id.issue_capability(cid_cap, issuer_pub, nf)
                    .map_err(|e| oh(format!("issue_capability: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &iss.call_data, vec![])?.submit())?;
            }

            *caps.lock().map_err(|_| oh("capability cell poisoned".into()))? = Some(CapSetup {
                cap_a, cap_b, schema_a, schema_b,
                credential_secret_a, credential_secret_b,
                capability_secret, attribute_blind, issuer_secret,
            });

            // ── DAO-Escrow: deployed here, endowed here, governed here ──
            //
            // **`dispute_v1` cannot be reached without a deployed DAO-Escrow, and DAO-Escrow is not
            // genesis.** The handler refuses a zero stored id before it looks at its child
            // (`entrypoint.rs:1059-1062`), and the only writer of that key is `init_contract` decoding
            // the deploy payload — so this fixture is the first to deploy a *second* contract to drive
            // another contract's endpoint. `dao_escrow_spec.rs` is the working example; what is ported
            // is the sequence the dispute needs and nothing else: the group, one approval set over the
            // proposal's message, the endowment, and the governance setter.
            //
            // Order is load-bearing in one place: the setter needs the endowment to exist, because
            // `update_v1` looks it up by the derived bulla.
            let dao_cid = crate::tests::blockchain::derive_contract_id_from_name("dao_escrow");
            smol::block_on(chain.deploy(
                dao, "dao_escrow",
                include_bytes!("../../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm"),
            )).map_err(|e| oh(format!("deploy dao_escrow: {e}")))?;

            let ms_cid = *MULTISIG_CONTRACT_ID;
            let group_id = DaoEscrowHarness::governance_group();
            let created = ms.create_group(
                DaoEscrowHarness::GOVERNANCE_THRESHOLD,
                DaoEscrowHarness::governance_member_commitments(),
            ).map_err(|e| oh(format!("create_group: {e}")))?;
            // The harness derives the id and the contract derives it again; if they disagreed the
            // endowment would store a group no signature could satisfy, and it would present as a
            // governance refusal rather than as a fixture bug.
            assert_eq!(
                created.group_id, group_id,
                "the created group's id must be the one the endowment will register",
            );
            smol::block_on(chain.block()?.with_call(ms_cid, ms, &created.call_data, vec![created.proof])?.submit())?;

            // One approval set, by `GOVERNANCE_THRESHOLD` of the members — real signatures, because the
            // multisig contract counts the threshold itself. **One set, because a MultiSig approval is
            // spend-once and this fixture makes one proposal**: a second set over the same message would
            // be spent by whichever finalize ran first.
            let mut approvals = Vec::new();
            for secret in DaoEscrowHarness::GOVERNANCE_MEMBERS
                .iter()
                .take(DaoEscrowHarness::GOVERNANCE_THRESHOLD as usize)
            {
                let s = ms.sign(group_id, msg_propose, *secret)
                    .map_err(|e| oh(format!("ms.sign: {e}")))?;
                smol::block_on(chain.block()?.with_call(ms_cid, ms, &s.call_data, vec![s.proof])?.submit())?;
                approvals.push(s.nullifier);
            }

            // `OBL-C198`: single-call transactions here, so the committed set is the call alone.
            let dao_init = dao.initialize(
                &[],
                nullifier_k, dao_bulla, owner_secret, endowment_asset_id, bulla_blind,
                DaoEscrowMode::Escrow, 0,
            ).map_err(|e| oh(format!("dao initialize: {e}")))?;
            smol::block_on(chain.block()?.with_call(dao_cid, dao, &dao_init.call_data, vec![dao_init.proof])?.submit())?;

            let set_group = dao.update(&[], endowment_bulla, owner_secret, owner_pub, Some(group_id))
                .map_err(|e| oh(format!("dao update (governance setter): {e}")))?;
            smol::block_on(chain.block()?.with_call(dao_cid, dao, &set_group.call_data, vec![set_group.proof])?.submit())?;

            *dao_approvals.lock().map_err(|_| oh("dao approvals cell poisoned".into()))? = approvals;

            Ok(())
        }})),
        deploy_ix: Some(deploy_ix),
        endpoints: vec![
            mk_ep("CreateJobV1", true, Box::new({ let cell = note_cell.clone(); move || {
                let note = cell.lock().ok().and_then(|g| g.clone())
                    .ok_or_else(|| dwow_core::Error::Custom("setup did not run".into()))?;
                // The blind the parent re-derives: `poseidon_hash([payment_amount, job_id])`.
                let blind = child_blind(PAYMENT, job_id);
                let r = h.create_job(employer_secret, employer_pub, attestation_id, job_id, 0, PAYMENT, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult {
                    children: vec![pn_transfer_child(&note, PAYMENT, blind)?, attestation_child(attestation_id)],
                    call_data: r.call_data, proofs: vec![r.proof] })
            }})),
            // **A second job, and the reason is measured rather than anticipated.** `SubmitDeliverableV1`
            // and `SubmitGitDeliverableV1` both require `JobState::InProgress` and both set `Delivered`,
            // so pairing them on one job makes the second refuse `InvalidStateTransition` — this
            // fixture's block 21. It spends a note of its own, because `create_job_v1` requires a
            // promissory-note transfer child and a note is spent once.
            mk_ep("CreateJobV1_Git", true, Box::new({
                let more = more_notes.clone();
                move || {
                    // The git job funds from the fifth extra note; the first job keeps the original.
                    let note = more.lock().ok().and_then(|g| g.get(4).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let blind = child_blind(PAYMENT, job_id_git);
                    // **`delivery_type: 1` is Git, and it is not decoration**: `submit_git_deliverable_v1`
                    // refuses a job created with `0` — `InvalidDeliveryType` (`Custom(17)`), measured on
                    // this fixture's block 24 — while its sibling `submit_deliverable_v1` accepts only
                    // Generic. So the two deliverable rows differ in the *job they act on* as well as the
                    // claim they verify, which is why this job carries the type in its name.
                    let r = h.create_job(employer_secret, employer_pub, attestation_id, job_id_git, 1, PAYMENT, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, PAYMENT, blind)?, attestation_child(attestation_id)],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // ── `CancelJobV1`'s pair: the job, and the cancel (`OBL-C189`) ──
            //
            // **Both rows exist because the defect is invisible without them.** `cancel_job_v1` had no
            // client, no harness method, no spec row and no circuit, so the seed it derived for its
            // refund child — the *same* seed `create_job_v1` requires of its deposit child — had never
            // been compared against anything. A job created by `create_job_v1` could not be cancelled
            // on any chain, because the commitment the cancel demanded was already on the note tree.
            //
            // The create is `CreateJobV1`'s own path, which is what makes the pair a collision: the
            // deposit child below commits exactly what the cancel row's child used to commit. The
            // cancel is hand-encoded — selector `0x07` plus `CancelJobParamsV1::encode()` — because the
            // endpoint is proof-less (its metadata arm publishes an encoded empty vector, so the host
            // requires no proof) and has no client (`OBL-C186`'s rule for the three endpoints before
            // it).
            mk_ep("CreateJobV1_Cancel", true, Box::new({
                let more = more_notes.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(5).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let blind = child_blind(PAYMENT, cancel_job_id);
                    let r = h.create_job(employer_secret, employer_pub, attestation_id, cancel_job_id, 0, PAYMENT, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, PAYMENT, blind)?, attestation_child(attestation_id)],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // **This row is the pair that pins the class, and it is the one row whose seed is the same
            // as another row's.** `CreateJobV1_Cancel` above deposits under
            // `poseidon_hash([payment_amount, job_id])`, and this row's cancel child demands the same
            // value commitment — because the contract derives that same seed for both, which is what
            // the record-keyed form means. It is green because the two children spend **different
            // notes**, and `pn_transfer_child` derives each output **leaf** blind from the note its
            // child spends (`OBL-C192`'s unit A): same seed, same value, different leaf, and the note
            // tree takes both.
            //
            // **What it catches, stated as the instrument it is.** Against a *contract-only* revert it
            // now behaves like the creates — the fixture and the contract move together and agree under
            // either rule. What it separates is the **caller**: remove the leaf derivation from
            // `pn_transfer_child` and this pair collides — the cancel's child reproduces the leaf the
            // create's deposit already put on the tree and `promissory_note` refuses it (`Custom(14)`),
            // which is precisely the refusal `OBL-C189` measured here before either reading existed. A
            // row that asserted the seed was the cause would have gone red on this change; this one is
            // green, because the seed was never the cause.
            //
            // The blind comes from `child_blind` rather than from a client because the endpoint has no
            // client, and it is the contract's own derivation stated once for all seven rows — so a
            // change to the contract's seed breaks every row that builds a child rather than silently
            // tracking it.
            mk_ep("CancelJobV1", false, Box::new({
                let more = more_notes.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(6).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let (ex, ey) = employer_pub.xy().ok_or_else(|| dwow_core::Error::Custom("employer pk is identity".into()))?;
                    let params = dwow_labor_market_contract::model::CancelJobParamsV1 {
                        proof: vec![],
                        job_id: cancel_job_id,
                        employer_pub_x: ex,
                        employer_pub_y: ey,
                    };
                    let mut call_data = vec![0x07u8];
                    call_data.extend_from_slice(&params.encode()
                        .map_err(|e| dwow_core::Error::Custom(format!("encode: {e}")))?);
                    let blind = child_blind(PAYMENT, cancel_job_id);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, PAYMENT, blind)?],
                        call_data, proofs: vec![] })
                }
            })),
            mk_ep("AcceptJobV1", true, Box::new(move || {
                let r = h.accept_job(worker_secret, worker_pub, job_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AcceptJobV1_Git", true, Box::new(move || {
                let r = h.accept_job(worker_secret, worker_pub, job_id_git).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SubmitDeliverableV1", true, Box::new(move || {
                let r = h.submit_deliverable(worker_secret, worker_pub, job_id, claim_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult {
                    children: vec![verify_claim_child(claim_id, attestation_id)?],
                    call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SubmitGitDeliverableV1", true, Box::new(move || {
                let r = h.submit_git_deliverable(worker_secret, worker_pub, job_id_git, claim_id_git).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult {
                    children: vec![verify_claim_child(claim_id_git, attestation_id)?],
                    call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ConfirmDeliveryV1", true, Box::new({
                let more = more_notes.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(0).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let r = h.confirm_delivery(employer_secret, employer_pub, job_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    // **The parent's derivation, which is the job's and not the call's:**
                    // `poseidon_hash([job.payment_amount, params.job_id])` — this endpoint's seed is
                    // `create_job_v1`'s, the record-keyed pair, and `child_blind` is the fixture's single
                    // copy of it. This row is what found the collision at block 25, and what it found was
                    // the note **leaf**: `OBL-C189` read the refusal as a property of the seed and keyed
                    // this endpoint on `params.spent_nullifier`, and reverting that leaves the seed alone
                    // and this row green, because `pn_transfer_child` derives the leaf from the note the
                    // child spends (`OBL-C192`'s unit A). The proof's `spent_nullifier` is no longer read
                    // here; it is still what the endpoint's `spent_flags` check enforces.
                    let blind = child_blind(PAYMENT, job_id);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, PAYMENT, blind)?],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // **It acts on the git job, not the first one, and that is a correction this fixture
            // earned by measurement.** `confirm_delivery_v1` requires `Delivered` and sets
            // `Confirmed` (`entrypoint.rs:971`/`:976`), while `refund_v1` requires `InProgress` *or*
            // `Delivered` and sets `Refunded` (`:1157`/`:1162`) — so the two are exclusive on one
            // job, exactly as the two deliverable rows are. The git job is left `Delivered` by
            // `SubmitGitDeliverableV1` and nothing else has touched it.
            mk_ep("RefundV1", true, Box::new({
                let more = more_notes.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(1).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let r = h.refund(job_id_git, employer_secret, 1, 2500, 2500, 5000, 200, 5000, employer_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    // **The blind is derived from the harness's own output, not guessed.** The parent
                    // computes `poseidon_hash([params.refund_amount, params.spent_nullifier])`, and
                    // `spent_nullifier` is produced by the proof — so the only place a caller can read
                    // it is the client's public inputs, which is what this is.
                    let blind = poseidon_hash([
                        pallas::Base::from(2500u64), r.public_inputs.spent_nullifier,
                    ]);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, 2500, blind)?],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // ── The writer: the only endpoint that can give a job a capability requirement ──
            //
            // Until this row existed, `job.required_capability_id` was always `None`, so
            // `accept_job_with_capability_v1`'s `ok_or_else` refused at `entrypoint.rs:1866` before
            // its comparison could run and the capability gate was **dormant**. Its sibling in
            // `tender` was the same, and `OBL-C186` records both.
            //
            // It is proof-less: `manifest.toml` gives it no `requires_proof`, and its metadata arm
            // returns an *encoded empty* `zk_public_inputs` with the reason written at
            // `entrypoint.rs:436-443` — encoded rather than bare, because a zero-byte buffer is the
            // host's documented rejection signal (`OBL-C77`). So the call is the selector plus
            // `Params::encode()`, exactly as tender's `CreateTenderWithCapabilityV1` row is built.
            //
            // It requires **one** `promissory_note::transfer_v1` child with value-commit validation
            // (`entrypoint.rs:1924-1955`), which is why it spends a note of its own.
            mk_ep("CreateJobWithCapabilityV1", false, Box::new({
                let more = more_notes.clone();
                let caps = caps.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(3).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let s = caps.lock().ok().and_then(|g| g.clone())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (caps)".into()))?;
                    let (ex, ey) = employer_pub.xy().ok_or_else(|| dwow_core::Error::Custom("employer pk is identity".into()))?;
                    let params = dwow_labor_market_contract::model::CreateJobWithCapabilityParamsV1 {
                        proof: vec![],
                        job_id: cap_job_id,
                        employer_pub_x: ex,
                        employer_pub_y: ey,
                        attestation_id,
                        delivery_type: 0,
                        payment_amount: PAYMENT,
                        payment_token: pallas::Base::from(1u64),
                        payment_commit_x: pallas::Base::from(2u64),
                        payment_commit_y: pallas::Base::from(3u64),
                        required_capability_id: s.cap_a.to_bytes(),
                        required_dag_id: None,
                    };
                    let mut call_data = vec![0x0cu8];
                    call_data.extend_from_slice(&params.encode()
                        .map_err(|e| dwow_core::Error::Custom(format!("encode: {e}")))?);
                    let blind = child_blind(PAYMENT, cap_job_id);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, PAYMENT, blind)?],
                        call_data, proofs: vec![] })
                }
            })),
            // ── THE NEGATIVE CASE, and the half `OBL-C186` was owed for this contract. It is the
            // row above's frame with exactly one argument changed — which capability the child
            // proves — so a rejection names the guard rather than something else in the frame. ──
            //
            // **Declared before the positive row, and that is a state requirement rather than a
            // style**: `accept_job_with_capability_v1` requires `JobState::Created`
            // (`entrypoint.rs:1857`), and the positive row below moves `cap_job_id` to `InProgress`.
            // A rejected row leaves the job where it was, so this order is the only one that works.
            //
            // **Before the writer row above existed this endpoint could only ever refuse** — with
            // `Custom(27)` (`CapabilityRequired`) at the `ok_or_else`, since every job carried `None`
            // — so the comparison this row drives had never run. Measured on tender, the same shape:
            // the needle is the *error code* and not a bare `Rejection`, because a row asserting only
            // `Rejection` is satisfied by any earlier failure in the frame (`OBL-C163`).
            // `CapabilityNotMet` is `Custom(28)`; `CapabilityRequired` is `Custom(27)` and
            // `InvalidCapability` is `Custom(29)`, so the three are distinguishable and the needle
            // names one of them.
            mk_ep_rejecting_naming("AcceptJobWithCapabilityV1_WrongCapability", true, &["ContractError(Custom(28))"],
                Box::new({
                    let caps = caps.clone();
                    // One clone per closure: `cap_proof` is a `Vec<u8>` and a `move` closure takes it
                    // whole, so a second row reading the original is a use-after-move (measured —
                    // `E0382` on the row below when this row was added).
                    let cap_proof = cap_proof.clone();
                    move || {
                        let s = caps.lock().ok().and_then(|g| g.clone())
                            .ok_or_else(|| dwow_core::Error::Custom("setup did not run (caps)".into()))?;
                        // The child proves capability B; the job requires A. Same builder, one
                        // argument different — which is what makes the pair a control rather than two
                        // rows that happen to run.
                        // `OBL-C198`: prepared first, proven after the parent, over the ordered set
                        // the node hashes (child first, this call last).
                        let (child_call, child_plan) = capability_child_prepare(&s, s.credential_secret_b, s.cap_b.inner(), s.schema_b)?;
                        let r = h.accept_job_with_capability(worker_secret, worker_pub, cap_job_id, pallas::Base::from(1u64), cap_proof.clone(), cap_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let job_call = dwow_sdk::tx::ContractCall {
                            contract_id: crate::tests::blockchain::derive_contract_id_from_name("labor_market"),
                            data: r.call_data.clone(),
                        };
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &job_call]);
                        let v = child_plan.prove(commitment).map_err(|e| dwow_core::Error::Custom(format!("verify_capability B: {e}")))?;
                        let child = ChildCall { contract_id: *IDENTITY_CONTRACT_ID, call_data: v.call_data, proofs: vec![v.proof], children: vec![] };
                        Ok(EndpointResult {
                            children: vec![child],
                            call_data: r.call_data, proofs: vec![r.proof] })
                    }
                })),
            // ── THE POSITIVE CASE, and the control for the row above: the same frame with the
            // capability the job actually requires must be ACCEPTED. It is the control for
            // `OBL-C186`'s dormancy — until the writer row above existed, this endpoint could only
            // ever refuse, and the pair could not be built at all. ──
            mk_ep("AcceptJobWithCapabilityV1_CorrectCapability", true, Box::new({
                let caps = caps.clone();
                let cap_proof = cap_proof.clone();
                move || {
                    let s = caps.lock().ok().and_then(|g| g.clone())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (caps)".into()))?;
                    // `OBL-C198`: prepared first, proven after the parent — see the row above.
                    let (child_call, child_plan) = capability_child_prepare(&s, s.credential_secret_a, s.cap_a.inner(), s.schema_a)?;
                    let r = h.accept_job_with_capability(worker_secret, worker_pub, cap_job_id, pallas::Base::from(1u64), cap_proof.clone(), cap_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let job_call = dwow_sdk::tx::ContractCall {
                        contract_id: crate::tests::blockchain::derive_contract_id_from_name("labor_market"),
                        data: r.call_data.clone(),
                    };
                    let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &job_call]);
                    let v = child_plan.prove(commitment).map_err(|e| dwow_core::Error::Custom(format!("verify_capability A: {e}")))?;
                    let child = ChildCall { contract_id: *IDENTITY_CONTRACT_ID, call_data: v.call_data, proofs: vec![v.proof], children: vec![] };
                    Ok(EndpointResult {
                        children: vec![child],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // ── The milestones job, and the repair `OBL-C190` owed (`create_job_with_milestones_v1`
            // had no escrow child check at all) ──
            //
            // **This row could not be built until 2026-09-29, and what blocked it was not what the
            // register said.** `OBL-C170`'s cell claimed "a proof no client can build"; the endpoint
            // dispatches to the `CreateJobV2` circuit, so `create_job_v1_proof` is its proof, and the
            // only missing piece was a builder for its params — the hand-encoded call this program has
            // used three times now (`close_tender`, `CreateJobWithCapabilityV1`, `CancelJobV1`). What
            // *did* block it is the create's own escrow check, which `OBL-C190` repaired: without it
            // the row would pass while validating nothing, which is not a test.
            //
            // Its child is the deposit every other create in this file makes, with the seed
            // `create_job_with_milestones_v1` derives — `poseidon_hash([payment_amount, job_id])` — and
            // the amount is the job's total, which is the sum of the milestones below.
            mk_ep("CreateJobWithMilestonesV1", true, Box::new({
                let more = more_notes.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(7).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let (call_data, proof) = milestones_create_call(
                        h, employer_secret, employer_pub, attestation_id, ms_job_id,
                        MS_JOB_PAYMENT, milestones_of(2),
                    )?;
                    let blind = child_blind(MS_JOB_PAYMENT, ms_job_id);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, MS_JOB_PAYMENT, blind)?],
                        call_data, proofs: vec![proof] })
                }
            })),
            // **`OBL-C190`'s negative control, and the same frame with exactly one thing changed: the
            // child list is empty.** The check this row exists to prove is reachable is the one the
            // repair added, so the needle is its own error code — `InvalidChildrenIndexes` is
            // `Custom(31)`, and `InvalidChildCall` (32) and `InvalidChildContractId` (33) are the two
            // neighbour codes a bare `Rejection` would not have distinguished it from.
            //
            // It is declared *after* its positive sibling so the pair reads as the row and its control;
            // the order costs nothing either way, because a rejected call leaves no state behind and the
            // child check is reached before the job-exists check.
            mk_ep_rejecting_naming("CreateJobWithMilestonesV1_NoChild", true, &["ContractError(Custom(31))"],
                Box::new(move || {
                    let (call_data, proof) = milestones_create_call(
                        h, employer_secret, employer_pub, attestation_id, ms_job_id,
                        MS_JOB_PAYMENT, milestones_of(2),
                    )?;
                    Ok(EndpointResult { children: vec![], call_data, proofs: vec![proof] })
                })),
            mk_ep("AcceptJobV1_Milestones", true, Box::new(move || {
                let r = h.accept_job(worker_secret, worker_pub, ms_job_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // **`SubmitDeliverableV1`, not `SubmitMilestoneV1`, and that is a measurement rather than a
            // preference.** `confirm_milestone_v1` requires the job to be `Delivered` (`:1648`), and the
            // two endpoints that set `Delivered` are `submit_deliverable_v1` and `submit_git_deliverable_v1`
            // — `submit_milestone_v1` also sets it, but its metadata arm publishes `SubmitDeliverableV2`'s
            // six instances, so it needs a proof this file has no way to build, which is `OBL-C190`'s
            // neighbour `OBL-C170` one endpoint over. `submit_deliverable_v1` needs the job to be
            // `InProgress`, `Generic` and to carry a `VerifyClaimV1` child — all three of which this job
            // and `claim_id_ms` supply.
            mk_ep("SubmitDeliverableV1_Milestones", true, Box::new(move || {
                let r = h.submit_deliverable(worker_secret, worker_pub, ms_job_id, claim_id_ms).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult {
                    children: vec![verify_claim_child(claim_id_ms, attestation_id)?],
                    call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // **The two rows that could not pass now can, and this is the first of them (`OBL-C170`).**
            // It acts on `ms_job_id`, whose two milestones `CreateJobWithMilestonesV1` made; the handler
            // refuses a job with no milestones with `JobDoesNotHaveMilestones` (`Custom(25)`) *before*
            // its state check (`:1644-1650`), which is what made the old row — pointed at `job_id` —
            // unable to pass for any reason but its own subject.
            //
            // Index 1 of 2 is the last milestone, so the handler takes the `Confirmed` arm. Nothing
            // confirms index 0 first: `confirm_milestone_v1` checks the index is in range and not
            // already completed, and does not require it to equal `current_milestone` (`:1660-1663`),
            // so a milestone can be confirmed without its predecessor.
            //
            // **That asymmetry is load-bearing, and this comment used to record it as deliberate
            // without saying why.** The why is HIGH-4, named above `confirm_delivery_v1`: the action
            // circuits share one nullifier derivation, so a job is submitted-to once. Since a
            // confirmation returns the job to `InProgress` and nothing returns it to `Delivered`,
            // requiring the index to be the *current* one would leave a job with more than one
            // milestone uncompletable — index 0 could be confirmed and then nothing further. Confirming
            // the *last* index is what makes the single confirmation a job gets terminal. Read
            // `entrypoint.rs:1665-1677` before changing this row or adding that check; `OBL-C96`
            // records the attempt that made a multi-milestone job deadlock instead.
            //
            // `MILESTONE_PAYMENT` reaches the harness as both `milestone_payment_amount` — the
            // circuit's instance 5 witness — and `payment_release` — the value the metadata arm
            // publishes in that slot — and the two must agree or the proof fails.
            //
            // Since `OBL-C96` the amount is no longer the caller's to choose: `confirm_milestone_v1`
            // requires `payment_release` to equal `job.milestones[index].payment_amount`
            // (`entrypoint.rs:1686-1691`). It does here because `milestones_of` writes
            // `MILESTONE_PAYMENT` into every milestone, and `MS_JOB_PAYMENT` is that figure times the
            // count — so this row would fail if either constant drifted.
            mk_ep("ConfirmMilestoneV1", true, Box::new({
                let more = more_notes.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(2).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let r = h.confirm_milestone(employer_secret, employer_pub, ms_job_id, 1, MILESTONE_PAYMENT, MILESTONE_PAYMENT).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let blind = poseidon_hash([
                        pallas::Base::from(MILESTONE_PAYMENT), r.public_inputs.spent_nullifier,
                    ]);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, MILESTONE_PAYMENT, blind)?],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // ── `OBL-C190`'s other repaired endpoint, and the note on the one above carries the
            // reasoning for every line here ──
            //
            // It is proof-less — its metadata arm publishes an encoded empty `zk_public_inputs`
            // (`entrypoint.rs:484-493`) — so the call is the selector plus the params' `encode()`, no
            // proof and no client, the same hand-encoded shape `CreateJobWithCapabilityV1` uses. One
            // milestone, because nothing in this fixture confirms this job; the row exists to drive the
            // create's child check and to give that check a positive case beside its control.
            mk_ep("CreateJobWithMilestonesAndCapabilityV1", false, Box::new({
                let more = more_notes.clone();
                let caps = caps.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(8).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let s = caps.lock().ok().and_then(|g| g.clone())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (caps)".into()))?;
                    let (ex, ey) = employer_pub.xy().ok_or_else(|| dwow_core::Error::Custom("employer pk is identity".into()))?;
                    let params = dwow_labor_market_contract::model::CreateJobWithMilestonesAndCapabilityParamsV1 {
                        proof: vec![],
                        job_id: ms_cap_job_id,
                        employer_pub_x: ex,
                        employer_pub_y: ey,
                        attestation_id,
                        delivery_type: 0,
                        payment_amount: MILESTONE_PAYMENT,
                        payment_token: pallas::Base::from(1u64),
                        payment_commit_x: pallas::Base::from(2u64),
                        payment_commit_y: pallas::Base::from(3u64),
                        deadline_block: MILESTONE_DEADLINE,
                        milestone_count: 1,
                        milestones: milestones_of(1),
                        required_capability_id: s.cap_a.to_bytes(),
                        required_dag_id: None,
                    };
                    let mut call_data = vec![0x0eu8];
                    call_data.extend_from_slice(&params.encode()
                        .map_err(|e| dwow_core::Error::Custom(format!("encode: {e}")))?);
                    let blind = child_blind(MILESTONE_PAYMENT, ms_cap_job_id);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, MILESTONE_PAYMENT, blind)?],
                        call_data, proofs: vec![] })
                }
            })),
            mk_ep_rejecting_naming("CreateJobWithMilestonesAndCapabilityV1_NoChild", false, &["ContractError(Custom(31))"],
                Box::new({
                    let caps = caps.clone();
                    move || {
                        let s = caps.lock().ok().and_then(|g| g.clone())
                            .ok_or_else(|| dwow_core::Error::Custom("setup did not run (caps)".into()))?;
                        let (ex, ey) = employer_pub.xy().ok_or_else(|| dwow_core::Error::Custom("employer pk is identity".into()))?;
                        let params = dwow_labor_market_contract::model::CreateJobWithMilestonesAndCapabilityParamsV1 {
                            proof: vec![],
                            job_id: ms_cap_job_id,
                            employer_pub_x: ex,
                            employer_pub_y: ey,
                            attestation_id,
                            delivery_type: 0,
                            payment_amount: MILESTONE_PAYMENT,
                            payment_token: pallas::Base::from(1u64),
                            payment_commit_x: pallas::Base::from(2u64),
                            payment_commit_y: pallas::Base::from(3u64),
                            deadline_block: MILESTONE_DEADLINE,
                            milestone_count: 1,
                            milestones: milestones_of(1),
                            required_capability_id: s.cap_a.to_bytes(),
                            required_dag_id: None,
                        };
                        let mut call_data = vec![0x0eu8];
                        call_data.extend_from_slice(&params.encode()
                            .map_err(|e| dwow_core::Error::Custom(format!("encode: {e}")))?);
                        Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                    }
                })),
            // ── THE LAST OF THE TWO ROWS `OBL-C170` RECORDED, and the only row in this tree whose
            // child has a child ──
            //
            // `dispute_v1` requires a `dao_escrow::ProposeClaimV1` child at selector `0x07` whose
            // contract id equals the stored DAO-Escrow id (`entrypoint.rs:1037-1066`) — and
            // `propose_claim_v1` itself requires a `multisig::FinalizeV1` child of its own
            // (`require_governance_child`, `dao_escrow/src/entrypoint.rs:1421-1428`). So the tree this
            // row submits is three levels deep, which no fixture could express before
            // `ChildCall::children` existed: `build_witness_tree` emitted exactly one level, and the
            // post-order walk that replaced it is the frame side of the same bound.
            //
            // **It acts on `cap_job_id`, not on `job_id`.** `dispute_v1` requires the job to be
            // `Delivered` or `InProgress` (`:1087-1090`), and by the time this row runs `job_id` is
            // `Confirmed` and `job_id_git` is `Refunded` — both moved there by rows that exist for
            // other reasons. `cap_job_id` is left `InProgress` by the capability row above and is
            // touched by nothing else, so pointing the dispute at it costs the fixture no new object
            // and gives the row a state the contract actually accepts.
            //
            // The child is built by the contract's own harness — `propose_claim` and `ms.finalize` —
            // so the proposal's params and the approval's nullifiers agree with what the contracts
            // derive; and the approval is the one `setup` cast, over this exact message.
            mk_ep("DisputeV1", true, Box::new({
                let approvals = dao_approvals.clone();
                move || {
                    let appr = approvals.lock().ok().map(|g| g.clone())
                        .filter(|a| !a.is_empty())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (dao approvals)".into()))?;
                    let r = h.dispute(cap_job_id, worker_secret, pallas::Base::from(99u64), endowment_bulla, worker_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_propose, appr)
                        .map_err(|e| dwow_core::Error::Custom(format!("finalize: {e}")))?;
                    // `OBL-C198`, AND THIS ROW IS THE CAMPAIGN'S HARD CASE, stated rather than left
                    // to be discovered: the dao call here is a **nested child**. The set the node
                    // hashes is `[ms_child, dao_call, this labor_market call]`, and a proof must
                    // bind to a commitment over that whole set — which no builder for a *child* can
                    // compute, because its parent's bytes come after it. Passing `&[]` binds this
                    // proof to `[dao_call]` alone, so the row is expected to be red until the dao
                    // harness grows a form the caller supplies the commitment to. It was already
                    // red before this change for the same reason one level up: `ms.finalize` binds
                    // over `[ms_call]`, while multisig's arm derives over the whole set.
                    let pc = dao.propose_claim(
                        &[],
                        nullifier_k, endowment_bulla, dispute_claim_id, dao_capability_id,
                        dao_capability_secret, dao_proposer_secret, 10_000,
                        pallas::Base::from(50u64), owner_pub, dispute_proposal_blind,
                    ).map_err(|e| dwow_core::Error::Custom(format!("propose_claim: {e}")))?;
                    Ok(EndpointResult {
                        children: vec![ChildCall {
                            contract_id: crate::tests::blockchain::derive_contract_id_from_name("dao_escrow"),
                            call_data: pc.call_data,
                            proofs: vec![pc.proof],
                            children: vec![ChildCall {
                                contract_id: *MULTISIG_CONTRACT_ID,
                                call_data: f.call_data,
                                proofs: vec![f.proof],
                                children: vec![],
                            }],
                        }],
                        call_data: r.call_data, proofs: vec![r.proof],
                    })
                }
            })),
        ],
    }
}
