//! ContractTestSpec for labor_market. Tier: HARVESTABLE — 9 harness methods, all ZK.
use dwow_contract_test_harness::harness::{
    AttestationHarness, ContractHarness, LaborMarketHarness, PromissoryNoteHarness,
};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    pasta_prelude::PrimeField, poseidon_hash, util::fp_mod_fv, Blind, MerkleNode, MerkleTree,
    PublicKey, SecretKey, ATTESTATION_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::{ChildCall, EndpointResult, EndpointSpec, EndpointExpectation};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// `(commitment, leaf position, merkle path, asset id, commitment blind)` — copied from
/// `insurance_market_spec.rs:69`, which copies it from `escrow_spec.rs`.
type PnNote = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// The job's payment, and therefore the value the promissory-note child must move.
const PAYMENT: u64 = 5000;

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

pub fn labor_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(LaborMarketHarness::spawn()));
    let h: &LaborMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/labor_market/dwow_labor_market_contract.wasm");
    let employer_secret = pallas::Base::from(10u64);
    let employer_pub = PublicKey::from_secret(SecretKey::from_base(employer_secret));
    let worker_secret = pallas::Base::from(20u64);
    let worker_pub = PublicKey::from_secret(SecretKey::from_base(worker_secret));
    let job_id = pallas::Base::from(100u64);
    let claim_id = pallas::Base::from(200u64);
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

    ContractTestSpec {
        name: "labor_market", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({ let cell = note_cell.clone(); move |chain| {
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
            mk_ep("AcceptJobV1", true, Box::new(move || {
                let r = h.accept_job(worker_secret, worker_pub, job_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SubmitDeliverableV1", true, Box::new(move || {
                let r = h.submit_deliverable(worker_secret, worker_pub, job_id, claim_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("SubmitGitDeliverableV1", true, Box::new(move || {
                let r = h.submit_git_deliverable(worker_secret, worker_pub, job_id, claim_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ConfirmDeliveryV1", true, Box::new(move || {
                let r = h.confirm_delivery(employer_secret, employer_pub, job_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("DisputeV1", true, Box::new(move || {
                let r = h.dispute(job_id, worker_secret, pallas::Base::from(99u64), dao_escrow_bulla, worker_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RefundV1", true, Box::new(move || {
                let r = h.refund(job_id, employer_secret, 1, 2500, 2500, 5000, 200, 5000, employer_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AcceptJobWithCapabilityV1", true, Box::new(move || {
                let r = h.accept_job_with_capability(worker_secret, worker_pub, job_id, pallas::Base::from(1u64), cap_proof.clone(), cap_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ConfirmMilestoneV1", true, Box::new(move || {
                let r = h.confirm_milestone(employer_secret, employer_pub, job_id, 1, 1000, 1000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
