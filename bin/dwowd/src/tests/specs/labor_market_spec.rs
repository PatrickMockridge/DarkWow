//! ContractTestSpec for labor_market. Tier: HARVESTABLE — 9 harness methods, all ZK.
use dwow_contract_test_harness::harness::{
    AttestationHarness, ContractHarness, IdentityHarness, LaborMarketHarness, PromissoryNoteHarness,
};
use dwow_identity_contract::model::{CapabilityId, CredentialRequirement};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    pasta_prelude::PrimeField, poseidon_hash, util::fp_mod_fv, Blind, IntentNullifier, MerkleNode,
    MerkleTree, PublicKey, SecretKey, ATTESTATION_CONTRACT_ID, IDENTITY_CONTRACT_ID,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::{ChildCall, EndpointResult, EndpointSpec, EndpointExpectation};
use crate::tests::uniform_runner::*;
use super::helpers::{mk_ep, mk_ep_rejecting};

/// `(commitment, leaf position, merkle path, asset id, commitment blind)` — copied from
/// `insurance_market_spec.rs:69`, which copies it from `escrow_spec.rs`.
type PnNote = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// The job's payment, and therefore the value the promissory-note child must move.
const PAYMENT: u64 = 5000;

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
    let (_, pos, path, asset_id, commitment_blind) = note;
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
        // child is rejected with `Custom(13)` before the parent's guard is reached. The setup below
        // issues with the same value for the same reason.
        secret: pallas::Base::from(100u64),
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
        commitment_blind: blind_seed,
    };
    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .transfer_with_value_blinds(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child.call_data, proofs: child.proofs })
}

/// The `attestation::CheckAttestationV1` child `create_job_v1` requires — **and this one needs no
/// proof.** `CheckAttestationParamsV1::encode` is the id's 32 bytes and nothing else
/// (`attestation/src/model/mod.rs:807`), so the call is the selector plus the id, and the manifest
/// gives the function no `requires_proof`. The parent only checks that the child's
/// `attestation_id` equals its own `params.attestation_id`.
fn attestation_child(attestation_id: pallas::Base) -> ChildCall {
    let mut call_data = vec![0x0du8];
    call_data.extend_from_slice(attestation_id.to_repr().as_ref());
    ChildCall { contract_id: *ATTESTATION_CONTRACT_ID, call_data, proofs: vec![] }
}

/// The `attestation::VerifyClaimV1` child `submit_deliverable_v1` requires — and this one **does**
/// need a proof, so it cannot be hand-encoded the way `attestation_child` is.
///
/// Ported from `attestation_spec.rs:99`, which is the working example and passes the same placeholder
/// witnesses; the parent checks only the selector and the contract id, so the child's own meaning is
/// the attestation contract's business.
fn verify_claim_child(claim_id: pallas::Base, attestation_id: pallas::Base) -> dwow_core::Result<ChildCall> {
    let att = AttestationHarness::spawn();
    let r = att.verify_claim(
        claim_id, attestation_id,
        pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64),
        pallas::Base::from(4u64), pallas::Base::from(5u64), [pallas::Base::from(0u64); 255],
        pallas::Base::from(6u64),
    ).map_err(|e| dwow_core::Error::Custom(format!("verify_claim: {e}")))?;
    Ok(ChildCall {
        contract_id: *ATTESTATION_CONTRACT_ID, call_data: r.call_data, proofs: vec![r.proof],
    })
}

/// The `identity::VerifyCapabilityV1` child both capability rows require — and this one **does** need
/// a proof, so it cannot be hand-encoded the way `attestation_child` is.
///
/// Ported from `insurance_market_spec.rs:411-419`; the parent checks the selector, the contract id
/// and the decoded `capability_proof.capability_id`, so which capability the child proves is the one
/// thing the two callers vary.
fn capability_child(
    s: &CapSetup,
    credential_secret: pallas::Base,
    capability_id: pallas::Base,
    schema: pallas::Base,
) -> dwow_core::Result<ChildCall> {
    let id = IdentityHarness::spawn();
    let holder = PublicKey::from_secret(SecretKey::from_base(credential_secret));
    let v = id.verify_capability(
        credential_secret, capability_id,
        pallas::Base::from(50u64),
        b"role", pallas::Base::from(100u64),
        b"tenure", pallas::Base::from(200u64),
        s.attribute_blind, s.capability_secret,
        PublicKey::from_secret(SecretKey::from_base(s.issuer_secret)),
        holder, schema, 0, EXPIRES_AT, true)
        .map_err(|e| dwow_core::Error::Custom(format!("verify_capability: {e}")))?;
    Ok(ChildCall { contract_id: *IDENTITY_CONTRACT_ID, call_data: v.call_data, proofs: vec![v.proof] })
}

pub fn labor_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(LaborMarketHarness::spawn()));
    let h: &LaborMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/labor_market/dwow_labor_market_contract.wasm");
    let employer_secret = pallas::Base::from(10u64);
    let employer_pub = PublicKey::from_secret(SecretKey::from_base(employer_secret));
    let worker_secret = pallas::Base::from(20u64);
    let worker_pub = PublicKey::from_secret(SecretKey::from_base(worker_secret));
    let job_id = pallas::Base::from(100u64);
    // **A second job, because the capability row cannot take the first one.** A job that
    // `AcceptJobV1` has already accepted has a worker and is `InProgress`, and
    // `accept_job_with_capability_v1` refuses both (`entrypoint.rs:1721-1728`) — so the capability
    // pair needs a job of its own, created by the only endpoint that can set a requirement.
    let cap_job_id = pallas::Base::from(101u64);
    // **And a third job, for the same shape one row over.** `SubmitDeliverableV1` and
    // `SubmitGitDeliverableV1` each require `JobState::InProgress` and each set `Delivered`
    // (`entrypoint.rs:792`/`:804` and `:866`/`:878`), so pairing them on one job makes the second
    // refuse `InvalidStateTransition` (`Custom(2)`) — measured on this fixture's own log at block 21.
    // It is the second-claim lesson again, one object over: **a one-shot transition needs an object of
    // its own**, and a fixture that shares one is testing less than it reads as testing.
    let job_id_git = pallas::Base::from(202u64);
    let claim_id = pallas::Base::from(200u64);
    // `SubmitGitDeliverableV1` needs its own claim — see the setup's note.
    let claim_id_git = pallas::Base::from(201u64);
    let attestation_id = pallas::Base::from(1u64);
    let dao_escrow_bulla = pallas::Base::from(60u64);
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

    ContractTestSpec {
        name: "labor_market", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({ let cell = note_cell.clone(); let more = more_notes.clone(); let caps = caps.clone(); move |chain| {
            let oh = |e: String| dwow_core::Error::Custom(e);
            let att_cid = *ATTESTATION_CONTRACT_ID;
            let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;

            // ── The attestation the job names. `create_job_v1` requires the child's
            // `attestation_id` to equal its own `params.attestation_id`, and requires the child at
            // all, so the attestation must exist before the row runs. ──
            let att = AttestationHarness::spawn();
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

            // ── The promissory note, worth exactly the job's payment, issued under the secret the
            // transfer child spends with. ──
            let pn = PromissoryNoteHarness::spawn();
            let pn_secret = pallas::Base::from(100u64);
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

            let id = IdentityHarness::spawn();
            let issuer = id.register_issuer(issuer_pub, b"employer".to_vec(), vec![])
                .map_err(|e| oh(format!("register_issuer: {e}")))?;
            smol::block_on(chain.block()?.with_call(id_cid, &id, &issuer.call_data, vec![])?.submit())?;

            let cred_a = id.issue_credential(issuer_secret, credential_secret_a,
                b"role", pallas::Base::from(100u64),
                b"tenure", pallas::Base::from(200u64),
                attribute_blind, schema_a, 0, EXPIRES_AT)
                .map_err(|e| oh(format!("issue_credential A: {e}")))?;
            let cred_b = id.issue_credential(issuer_secret, credential_secret_b,
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
            Ok(())
        }})),
        deploy_ix: None,
        endpoints: vec![
            mk_ep("CreateJobV1", true, Box::new({ let cell = note_cell.clone(); move || {
                let note = cell.lock().ok().and_then(|g| g.clone())
                    .ok_or_else(|| dwow_core::Error::Custom("setup did not run".into()))?;
                // The blind the parent re-derives: `poseidon_hash([payment_amount, job_id])`.
                let blind = poseidon_hash([pallas::Base::from(PAYMENT), job_id]);
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
                    let blind = poseidon_hash([pallas::Base::from(PAYMENT), job_id_git]);
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
                    // **The parent's own derivation, which is the *call's* and not the job's:**
                    // `poseidon_hash([job.payment_amount, params.spent_nullifier])`. It is read from the
                    // proof's public inputs because the circuit produces `spent_nullifier` — the same
                    // constraint the two rows below record. This row is what found the defect: deriving
                    // from `job_id` instead made the child's required output commitment the one
                    // `create_job_v1` had already deposited, and `promissory_note` refused it
                    // (`Duplicate commitment in output 0`, `Custom(14)`, block 25).
                    let blind = poseidon_hash([
                        pallas::Base::from(PAYMENT), r.public_inputs.spent_nullifier,
                    ]);
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
            // `accept_job_with_capability_v1`'s `ok_or_else` refused at `entrypoint.rs:1730` before
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
            // (`entrypoint.rs:1787-1822`), which is why it spends a note of its own.
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
                    let blind = poseidon_hash([pallas::Base::from(PAYMENT), cap_job_id]);
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
            // (`entrypoint.rs:1721`), and the positive row below moves `cap_job_id` to `InProgress`.
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
            mk_ep_rejecting("AcceptJobWithCapabilityV1_WrongCapability", true, &["ContractError(Custom(28))"],
                Box::new({
                    let caps = caps.clone();
                    // One clone per closure: `cap_proof` is a `Vec<u8>` and a `move` closure takes it
                    // whole, so a second row reading the original is a use-after-move (measured —
                    // `E0382` on the row below when this row was added).
                    let cap_proof = cap_proof.clone();
                    move || {
                        let s = caps.lock().ok().and_then(|g| g.clone())
                            .ok_or_else(|| dwow_core::Error::Custom("setup did not run (caps)".into()))?;
                        let r = h.accept_job_with_capability(worker_secret, worker_pub, cap_job_id, pallas::Base::from(1u64), cap_proof.clone(), cap_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        // The child proves capability B; the job requires A. Same builder, one
                        // argument different — which is what makes the pair a control rather than two
                        // rows that happen to run.
                        Ok(EndpointResult {
                            children: vec![capability_child(&s, s.credential_secret_b, s.cap_b.inner(), s.schema_b)?],
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
                    let r = h.accept_job_with_capability(worker_secret, worker_pub, cap_job_id, pallas::Base::from(1u64), cap_proof.clone(), cap_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![capability_child(&s, s.credential_secret_a, s.cap_a.inner(), s.schema_a)?],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // **This row is declared and cannot pass, and saying so is the point (`OBL-C170`).** It
            // acts on `job_id`, which `create_job_v1` made and which has **no milestones**; the
            // handler refuses exactly that with `JobDoesNotHaveMilestones`
            // (`entrypoint.rs:1540-1543`, `job.milestones.is_empty()`) *before* it reaches the state
            // check. A job that has milestones comes only from `create_job_with_milestones_v1`, whose
            // metadata arm publishes `CreateJobV2`'s five instances (`:346-359`) — so it requires a
            // proof in that namespace, and `client/` carries `create_job.rs` and no
            // `create_job_with_milestones.rs`. That is `OBL-C187`'s class from the other side: not a
            // params type that cannot be encoded, but a proof no client can build.
            //
            // The note child below is real and correctly derived — the row is wired for everything
            // except the job — so it costs nothing to leave in place for the client that unblocks it.
            mk_ep("ConfirmMilestoneV1", true, Box::new({
                let more = more_notes.clone();
                move || {
                    let note = more.lock().ok().and_then(|g| g.get(2).cloned())
                        .ok_or_else(|| dwow_core::Error::Custom("setup did not run (notes)".into()))?;
                    let r = h.confirm_milestone(employer_secret, employer_pub, job_id, 1, 1000, 1000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let blind = poseidon_hash([
                        pallas::Base::from(1000u64), r.public_inputs.spent_nullifier,
                    ]);
                    Ok(EndpointResult {
                        children: vec![pn_transfer_child(&note, 1000, blind)?],
                        call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // **`DisputeV1` is declared last, and it is the second of the two rows that cannot pass
            // yet** — the other is `ConfirmMilestoneV1`, above. It requires a
            // `dao_escrow::ProposeClaimV1` child (`0x07`), and unlike the rows before it that is not
            // a builder this fixture can borrow: it needs a real `dao_escrow` endowment with its
            // governance configured, which no spec in this tree demonstrates yet. Declaring the two
            // blocked rows last does not make them pass — it makes the rows that *can* be measured
            // measured, because the runner submits in declaration order and stops at the first
            // failure. Stated here rather than left to read as coverage (`OBL-C170`).
            mk_ep("DisputeV1", true, Box::new(move || {
                let r = h.dispute(job_id, worker_secret, pallas::Base::from(99u64), dao_escrow_bulla, worker_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
