//! ContractTestSpec for drain_protection.
//!
//! **Every endpoint's proof is real** (`OBL-C88`): the harness proves through the contract's own
//! client (`create_authority_proof` for the eight authority circuits, `create_exit_proof` for
//! `exit`) and the params carry the same public inputs the proof was made with, where they used to
//! be `empty_witnesses` fabrications.
//!
//! The endpoints are a **flow**, and each part of it was produced by a failure rather than
//! designed up front:
//!
//!   * `initialize` first — it creates the fund every other call acts on.
//!   * the two `update_config` endpoints next, because they are `OBL-C97`'s control pair: the same
//!     call from the fund's authority and from a stranger. The legitimate one also gives the fund a
//!     `multisig_group_id`, which `initialize` stores as zero and `execute` requires (`OBL-C98`).
//!   * `propose` derives the proposal id that `vote` and `execute` name
//!     (`poseidon_hash([fund.id, message_hash])`).
//!   * `exit` needs a **payment child** — a `promissory_note::transfer_v1` — so this spec carries a
//!     PN setup and a child builder, as the subscription spec will need to. It **passes** end to
//!     end: the pool has no members and `total_funds` is zero, so the exit's payout is zero by
//!     arithmetic and its child is a zero-value transfer.
//!   * `transfer` builds its child the same way and does **not** pass yet: the block is rejected at
//!     the L2 proof verify with `invalid proof: call[0] namespace 'Revoke_V2'`, before any
//!     drain_protection code runs. That is a fixture question, not the contract's, and it is
//!     recorded in `OBL-C88` with the diagnostic that did not settle it.
use std::sync::{Arc, Mutex};

use dwow_contract_test_harness::harness::{
    ContractHarness, DrainProtectionHarness, PromissoryNoteHarness,
};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::{
    crypto::{
        pasta_prelude::PrimeField, poseidon_hash, util::fp_mod_fv, Blind, MerkleNode, MerkleTree,
        PublicKey, SecretKey,
    },
    pasta::pallas,
};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// A note tuple: `(commitment, leaf position, merkle path, asset id, commitment blind)`.
type Note = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// Build a `promissory_note::transfer_v1` child spending one note. Copied from
/// `stablecoin_spec.rs` — each spec carries its own, which is the repository's convention.
fn pn_transfer_child(
    note: &Note,
    value: u64,
    blind_seed: pallas::Base,
    spend_hook: pallas::Base,
) -> dwow_core::Result<ChildCall> {
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
        spend_hook,
        user_data: pallas::Base::zero(),
        commitment_blind: pallas::Base::from(7u64),
    };
    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .transfer_with_value_blinds(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall {
        contract_id: *dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID,
        call_data: child.call_data,
        proofs: child.proofs,
    })
}

pub fn drain_protection_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DrainProtectionHarness::spawn()));
    let h: &DrainProtectionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm");
    let notes: Arc<Mutex<Option<Vec<Note>>>> = Arc::new(Mutex::new(None));
    ContractTestSpec {
        name: "drain_protection", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            move |chain| {
                let pn = PromissoryNoteHarness::spawn();
                let pn_cid = *dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID;
                let owner_secret = pallas::Base::from(100u64);
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), owner_secret]);
                let issue_secret = pallas::Base::from(100u64);
                let token = pn
                    .register_type(
                        issue_secret,
                        pallas::Base::from(2u64),
                        pallas::Base::from(3u64),
                        owner_addr,
                        10000,
                        pallas::Base::zero(),
                        pallas::Base::zero(),
                        pallas::Base::from(6u64),
                    )
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain
                        .block()?
                        .with_call(pn_cid, &pn, &token.call_data, token.token_proofs.clone())?
                        .submit(),
                )?;
                let asset_id = token.asset_id;
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token.commitment.inner()));
                let mark = tree.mark().expect("tree.mark");
                let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("tree.witness");
                let mut issued =
                    vec![(token.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(6u64))];
                // Two notes beyond the registered type's commitment, and their **values are the
                // contract's expected payouts**: a `promissory_note::transfer_v1` spends an input
                // note and pays it out whole (the helper below builds one output of the same
                // value), and `verify_value_conservation` forbids any other shape — so the note's
                // value must be what the arm checks the child against. The exit's payout is zero by
                // arithmetic (no members, no funds), so its note is worth zero; the transfer's is
                // the amount the harness sends.
                for (value, cb) in [(0u64, 11u64), (100, 12)] {
                    let n = pn
                        .issue(issue_secret, asset_id, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(cb))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(
                        chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit(),
                    )?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().expect("tree.mark");
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("tree.witness");
                    issued.push((n.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(cb)));
                }
                *notes.lock().unwrap() = Some(issued);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            mk_ep("initialize", true, Box::new(move || {
                let r = h.initialize().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // The two `update_config` endpoints sit here, immediately after the fund they act on and
            // before the proposal flow, because they are `OBL-C97`'s control pair and a control has
            // to be read as a pair: the same call from the fund's authority and from a stranger.
            mk_ep("update_config", true, Box::new(move || {
                let r = h.update_config().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // NEGATIVE — the endpoint above is what makes this one a control. The stranger holds a
            // **real** proof of the real circuit, built by the contract's own client; the only
            // difference from `update_config` is which secret the published point derives from.
            // Every other precondition is identical — the fund exists, it is unlocked, and the call
            // changes nothing else — so if the host stops comparing the point against the fund's
            // registered `spend_authority`, this endpoint starts succeeding and the control is gone.
            // Read the rejection's cause in the `DWOW_TEST_LOGS=1` output: it must be
            // `InvalidSpendAuthority` ("authority is not the fund's registered spend authority"),
            // not a proof-verification failure and not a state precondition.
            EndpointSpec {
                name: "update_config_by_stranger", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.update_config_as_stranger().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            mk_ep("propose", true, Box::new(move || {
                let r = h.propose().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("vote", true, Box::new(move || {
                let r = h.vote().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("execute", true, Box::new(move || {
                let r = h.execute().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("exit", true, Box::new({
                let notes = notes.clone();
                move || {
                    let r = h.exit().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let n = notes.lock().unwrap();
                    let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                    // The pool has neither members nor funds — `initialize` stores an empty member
                    // list and `total_funds: 0`, and nothing raises either — so the exit's payout is
                    // **zero by arithmetic**: `member_weight * total_funds / max(total_weight, 1)`
                    // is 0 of 0, times the 66.67% the contract keeps after its 33.33% haircut. The
                    // child must therefore pay zero, with the blind the contract derives from the
                    // same two values (`poseidon_hash([exit_value, fund_id])`, `entrypoint.rs:691`).
                    let blind_seed = poseidon_hash([pallas::Base::zero(), pallas::Base::from(1u64)]);
                    let child = pn_transfer_child(&n[1], 0, blind_seed, pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("transfer", true, Box::new({
                let notes = notes.clone();
                move || {
                    let r = h.transfer().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let n = notes.lock().unwrap();
                    let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                    // `transfer_process_instruction_v1` requires exactly one child and checks it
                    // against `params.amount` with the blind `poseidon_hash([amount, fund_id])`
                    // (`entrypoint.rs:680-683`); the fixture's `amount` is 100, and the note spent
                    // here is worth 100 for the conservation reason the setup states.
                    let blind_seed = poseidon_hash([pallas::Base::from(100u64), pallas::Base::from(1u64)]);
                    let child = pn_transfer_child(&n[2], 100, blind_seed, pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("lock", true, Box::new(move || {
                let r = h.lock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("unlock", true, Box::new(move || {
                let r = h.unlock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
