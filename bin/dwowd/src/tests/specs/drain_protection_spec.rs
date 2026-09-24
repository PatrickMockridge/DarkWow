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
//!   * `execute` is the one endpoint whose child is an **approval** rather than a payment
//!     (`OBL-C101`): a `multisig::FinalizeV1` over the proposal id, under the fund's own group. The
//!     setup creates that group on chain — three members, threshold two, the multisig contract's own
//!     `derive_group_id` — and gathers the approvals, because the finalize child *names* them. The
//!     parent checks the group and the message; the threshold is the child's own doing, since the
//!     multisig contract counts it and a failing child fails this transaction. Two controls carry the
//!     pair: a second group's valid approval of the same proposal, and the fund's own group's valid
//!     approval of a different message. The rate-limited `transfer` then reads what `execute`
//!     recorded, and `transfer_unexecuted_proposal` is the control for that — approved, not executed,
//!     refused.
use std::sync::{Arc, Mutex};

use dwow_contract_test_harness::harness::{
    ContractHarness, DrainProtectionHarness, MultiSigHarness, PromissoryNoteHarness,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::PrimeField, poseidon_hash, MerkleNode, MerkleTree, Nullifier,
        MULTISIG_CONTRACT_ID,
    },
    pasta::pallas,
};
// The shared builders (`modules/child_calls.rs`), not a per-spec copy: this spec's own copy is what
// made the exit's child a zero-value one, and a **zero-value spend is unprovable** (see the header).
use crate::tests::modules::child_calls::{pn_transfer_child, pn_transfer_payout_child, PnNote};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// A second group, for `execute_foreign_group`: one member, threshold one, so its approval of the
/// proposal is a **valid** finalize that the fund must nevertheless refuse.
const FOREIGN_MEMBER: pallas::Base = pallas::Base::from_raw([21, 0, 0, 0]);

/// The different message the fund's own group approves, for `execute_foreign_message`.
const OTHER_MESSAGE: pallas::Base = pallas::Base::from_raw([99, 0, 0, 0]);

/// The signature nullifiers `setup` gathered, by case (`OBL-C101`). A finalize child *names* the
/// approvals it counts rather than deriving them, so the fixture has to carry what the signers'
/// calls produced.
#[derive(Default)]
struct Approvals {
    /// The fund's governance group on the proposal — the approvals that must be accepted.
    governance: Vec<Nullifier>,
    /// The fund's governance group on `OTHER_MESSAGE` — valid approvals of the wrong thing.
    other_message: Vec<Nullifier>,
    /// The foreign group's id and its approvals of the proposal — valid approvals by the wrong group.
    foreign_group: pallas::Base,
    foreign: Vec<Nullifier>,
}

pub fn drain_protection_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DrainProtectionHarness::spawn()));
    let h: &DrainProtectionHarness = harness;
    // Leaked like the contract's harness, and for a reason that shows up in the wall clock: every
    // `spawn` rebuilds the multisig contract's three proving keys, and this spec calls `sign`,
    // `create_group` and `finalize` from a setup that runs twice plus three endpoints.
    let ms: &'static MultiSigHarness = Box::leak(Box::new(MultiSigHarness::spawn()));
    let wasm = include_bytes!("../../../../../src/contract/drain_protection/dwow_drain_protection_contract.wasm");
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));
    // The approvals the governance group cast in `setup`, per case (`OBL-C101`): the fund's group on
    // the proposal (the positive path), a foreign group on the same proposal, and the fund's group on
    // a different proposal. Each is a set of signature nullifiers, captured from the `sign` calls
    // that produced them because the finalize child names them rather than re-deriving them.
    let approvals: Arc<Mutex<Approvals>> = Arc::new(Mutex::new(Approvals::default()));
    ContractTestSpec {
        name: "drain_protection", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let approvals = approvals.clone();
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
                // Three notes beyond the registered type's commitment, and their values are the
                // **inputs the children spend** — not the amounts the arms check them against. The
                // distinction is the fix this fixture needed: a spend's value must be non-zero
                // (`revoke.zk:69`), so a zero *payout* cannot be built by spending a zero note. The
                // exit's note is the member's locked note, and the exit's zero payout is satisfied by
                // a payout-plus-change child over it; the transfer's note is the amount the harness
                // sends, paid out whole. The third is the record-check control's: its child has to be
                // a **fresh** spend, or the control would be refused by the child (a spent nullifier,
                // a duplicate commitment) rather than by the parent check it exists to test.
                for (value, cb) in [(1000u64, 11u64), (100, 12), (100, 13)] {
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

                // ── The governance group, on chain (`OBL-C101`) ────────────────────────────────
                // `execute` requires a `multisig::FinalizeV1` child whose group is the fund's, whose
                // message is the proposal, and whose approvals meet the group's threshold — the last
                // of which the multisig contract checks itself, so the approvals below have to be
                // real signatures by real members. MultiSig is a genesis contract, so the calls need
                // no deployment; each is an ordinary block, as the PN notes above are.
                let ms_cid = *MULTISIG_CONTRACT_ID;
                let group_id = DrainProtectionHarness::governance_group();
                let proposal_id = h.proposal_id();
                let created = ms
                    .create_group(
                        DrainProtectionHarness::GOVERNANCE_THRESHOLD,
                        DrainProtectionHarness::governance_member_commitments(),
                    )
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                // The harness derives the id with the contract's own function and the spec derives it
                // again through the same call; if they ever disagree the fund would store a group no
                // signature could ever satisfy, and it would look like a contract refusal.
                assert_eq!(
                    created.group_id, group_id,
                    "the created group's id must be the one the fund will register",
                );
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &created.call_data, vec![created.proof])?.submit(),
                )?;

                // The fund's own group approves the proposal: the positive path, and — with
                // `GOVERNANCE_THRESHOLD` of the three members — the threshold the multisig contract
                // counts is met by real signatures.
                let mut governance = Vec::new();
                for secret in DrainProtectionHarness::GOVERNANCE_MEMBERS
                    .iter()
                    .take(DrainProtectionHarness::GOVERNANCE_THRESHOLD as usize)
                {
                    let s = ms
                        .sign(group_id, proposal_id, *secret)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(
                        chain.block()?.with_call(ms_cid, ms, &s.call_data, vec![s.proof])?.submit(),
                    )?;
                    governance.push(s.nullifier);
                }

                // The fund's own group approves a *different* message — valid approvals of the wrong
                // thing, for `execute_foreign_message`.
                let mut other_message = Vec::new();
                for secret in DrainProtectionHarness::GOVERNANCE_MEMBERS
                    .iter()
                    .take(DrainProtectionHarness::GOVERNANCE_THRESHOLD as usize)
                {
                    let s = ms
                        .sign(group_id, OTHER_MESSAGE, *secret)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(
                        chain.block()?.with_call(ms_cid, ms, &s.call_data, vec![s.proof])?.submit(),
                    )?;
                    other_message.push(s.nullifier);
                }

                // A second group — one member, threshold one — approves the *proposal*. Its approval
                // is valid; what is wrong is who gave it, for `execute_foreign_group`.
                let foreign_commitments = vec![MultiSigHarness::member_commitment(FOREIGN_MEMBER)];
                let foreign = ms
                    .create_group(1, foreign_commitments)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &foreign.call_data, vec![foreign.proof])?.submit(),
                )?;
                let f = ms
                    .sign(foreign.group_id, proposal_id, FOREIGN_MEMBER)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &f.call_data, vec![f.proof])?.submit(),
                )?;

                *approvals.lock().unwrap() = Approvals {
                    governance,
                    other_message,
                    foreign_group: foreign.group_id,
                    foreign: vec![f.nullifier],
                };
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
            // `execute` needs an approval child (`OBL-C101`): a `multisig::FinalizeV1` over the
            // proposal, under the fund's own group, with the approvals `setup` gathered. The parent
            // checks the group and the message; the *threshold* is the child's own doing — the
            // multisig contract refuses a finalize short of the group's threshold, and a failing
            // child fails its parent's transaction.
            mk_ep("execute", true, Box::new({
                let approvals = approvals.clone();
                move || {
                    let r = h.execute().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let a = approvals.lock().unwrap();
                    let f = ms
                        .finalize(DrainProtectionHarness::governance_group(), h.proposal_id(), a.governance.clone())
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let child = ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] };
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // NEGATIVE — the approval is real, and it is someone else's. The foreign group (one
            // member, threshold one) has genuinely approved the proposal, so the *child* commits:
            // what must stop it is the parent comparing the child's group against the fund's
            // registered one. If that comparison is removed this endpoint starts succeeding, and the
            // fund's governance is a signature from anyone.
            EndpointSpec {
                name: "execute_foreign_group", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let approvals = approvals.clone();
                    move || {
                        let r = h.execute().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let a = approvals.lock().unwrap();
                        let f = ms
                            .finalize(a.foreign_group, h.proposal_id(), a.foreign.clone())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // NEGATIVE — the group is right and the *message* is not: the fund's own group approved
            // something other than this proposal, so the child commits and the parent must refuse.
            // Read the cause in the `DWOW_TEST_LOGS=1` output: `[ExecuteV1] Error: the approval names
            // a different proposal`.
            EndpointSpec {
                name: "execute_foreign_message", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let approvals = approvals.clone();
                    move || {
                        let r = h.execute().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let a = approvals.lock().unwrap();
                        let f = ms
                            .finalize(DrainProtectionHarness::governance_group(), OTHER_MESSAGE, a.other_message.clone())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
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
            // NEGATIVE — the record check's control (`OBL-C101`), and the one that distinguishes
            // *approved* from *executed*. The same transfer call, the same proof, the same amount,
            // naming a proposal the fund's own group genuinely approved (`setup` signs `OTHER_MESSAGE`
            // with the same members) and which nobody ever executed. If the check is removed, this
            // endpoint starts succeeding and a rate-limited transfer needs only a proposal id.
            EndpointSpec {
                name: "transfer_unexecuted_proposal", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .transfer_naming(h.proposal_id_of(OTHER_MESSAGE))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(100u64), pallas::Base::from(1u64)]);
                        // The third note, and an output commitment blind of its own: the positive
                        // transfer's child creates a commitment with the same value in the same
                        // asset, and a second one identical to it is a PN duplicate.
                        let child = pn_transfer_child(&n[3], 100, blind_seed, pallas::Base::from(8u64), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
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
