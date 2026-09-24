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
//!   * `exit` needs a **payment child** — a `promissory_note::transfer_v1`. It failed, and the
//!     failure was measured, not inferred: the block at height 10 (the `exit` block — the exec
//!     order in the `DWOW_TEST_LOGS=1` output is Initialize → UpdateConfig → UpdateConfig →
//!     Propose → Vote → Execute → Exit) was rejected *after* `[ExitV1::apply] Exit recorded`, at
//!     the L2 proof verify, `invalid proof: call[0] namespace 'Revoke_V2'`. The cause was this
//!     fixture, and it was a real one: the exit's payout is zero by arithmetic (no members, no
//!     funds), and the fixture had concluded that a zero *payout* needs a zero-valued *note* spent.
//!     **A zero-value spend is unprovable** — `revoke.zk:69` constrains `less_than_strict(ZERO,
//!     value)`, so its proof can never be satisfied, and `zero_cond(value, coin)` would fold the
//!     empty leaf where the client folds the real commitment. A zero *payout* needs no such thing:
//!     `validate_child_value_commit` accepts **any** output whose `value_commit` equals
//!     `pedersen(payout, blind)`, so a payout-plus-change child satisfies the parent while spending a
//!     non-zero note — which is exactly what `pn_transfer_payout_child` is for, and what the
//!     gambling specs already use for their zero payouts (`darktoshi_dice_spec.rs:156`).
//!   * `transfer` builds its child the same way and was **never reached** in the run that recorded the
//!     exit's failure: the runner aborts at the first failing endpoint, and `exit` precedes it. Its
//!     child is a plain one-in/one-out transfer of the `params.amount` the fixture sends, which is
//!     non-zero, so it is provable — and in the run after the fix it is accepted.
//!   * the `unlock`/`lock`/`unlock` trio at the end is ordered by the contract, not by taste: the
//!     accepted `unlock` needs an unlocked fund, and the rejection one needs a locked fund, so the
//!     timelock branch can only be measured from the far side of a `lock`.
use std::sync::{Arc, Mutex};

use dwow_contract_test_harness::harness::{
    ContractHarness, DrainProtectionHarness, PromissoryNoteHarness,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, MerkleNode, MerkleTree},
    pasta::pallas,
};
// The shared builders (`modules/child_calls.rs`), not a per-spec copy: this spec's own copy is what
// made the exit's child a zero-value one, and a **zero-value spend is unprovable** (see the header).
use crate::tests::modules::child_calls::{pn_transfer_child, pn_transfer_payout_child, PnNote};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn drain_protection_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DrainProtectionHarness::spawn()));
    let h: &DrainProtectionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm");
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));
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
                // Two notes beyond the registered type's commitment, and their values are the
                // **inputs the children spend** — not the amounts the arms check them against. The
                // distinction is the fix this fixture needed: a spend's value must be non-zero
                // (`revoke.zk:69`), so a zero *payout* cannot be built by spending a zero note. The
                // exit's note is the member's locked note, and the exit's zero payout is satisfied by
                // a payout-plus-change child over it; the transfer's note is the amount the harness
                // sends, paid out whole.
                for (value, cb) in [(1000u64, 11u64), (100, 12)] {
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
                    // parent looks for an output whose value commitment is
                    // `pedersen(0, fp_mod_fv(blind))` with `blind = poseidon_hash([exit_value,
                    // fund_id])` (`entrypoint.rs:691-694`, `validation.rs:63-69`), and a payout
                    // child over the member's note provides exactly that as its first output while
                    // the rest returns as change — the only shape that leaves the spent note
                    // non-zero, and therefore the only one that can be proven at all.
                    let blind_seed = poseidon_hash([pallas::Base::zero(), pallas::Base::from(1u64)]);
                    let child = pn_transfer_payout_child(&n[1], 1000, 0, blind_seed)?;
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
                    // (`entrypoint.rs:757-761`); the fixture's `amount` is 100, and the note spent
                    // here is worth 100, so one input and one output of 100 conserve value with the
                    // shared blind the helper is handed.
                    let blind_seed = poseidon_hash([pallas::Base::from(100u64), pallas::Base::from(1u64)]);
                    let child = pn_transfer_child(&n[2], 100, blind_seed, pallas::Base::from(7u64), pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // `unlock` before `lock`, and then the control that makes the pair readable. The fund is
            // unlocked when the first of these runs, so the contract takes the branch where the
            // timelock does not apply and the call is accepted — `[UnlockV1::apply] Fund unlocked` in
            // the `DWOW_TEST_LOGS=1` output is the evidence, not the endpoint's green. Order is the
            // fixture's business and this order is forced: the runner advances one block per endpoint,
            // while `unlock` needs 1440 blocks *past* the lock's expiry (`entrypoint.rs:817-824`), so
            // an accepted unlock can only ever be the unlocked-fund branch in a linear run.
            mk_ep("unlock", true, Box::new(move || {
                let r = h.unlock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("lock", true, Box::new(move || {
                let r = h.lock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // NEGATIVE — the control for the endpoint above, and the reason the pair is ordered this
            // way: this is the *same* call one block after a 6000-block lock, and it must be refused.
            // Read the cause in the `DWOW_TEST_LOGS=1` output: it must be
            // `DrainProtectionError::UnlockTimelockNotExpired` — `Custom(6)` in the host's encoding
            // (`error.rs:89-91`), whose message names the blocks still needed — and not a proof
            // failure, not the authority check, and not a state precondition reached earlier. If the
            // timelock check is removed this endpoint starts succeeding and the control is gone.
            EndpointSpec {
                name: "unlock_before_timelock", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.unlock().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
