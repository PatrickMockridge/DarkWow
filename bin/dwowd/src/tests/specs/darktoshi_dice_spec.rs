//! ContractTestSpec for darktoshi_dice. Spec: heavyweight-spec.md §5.9.
//!
//! Money flow: CommitBetV1 locks the bet (1:1 PN child); RevealRollV1 reveals the secret nonce and
//! derives the roll from block-hash entropy (no child); SettleBetV1 pays out (payout child — 0 on a
//! house win); HouseCloseV1 collects an abandoned bet (child).
//!
//! `OBL-C198`: the PN children are built by `modules::child_calls`' `pn_transfer_prepare` /
//! `pn_transfer_payout_prepare`, which stop short of the proof. A child's proof now has to bind to
//! the same transaction commitment its parent does, and that value is only known once the parent's
//! call data exists — so the call data, the child, the commitment and the proof are resolved in
//! that order. The unsplit `pn_transfer_child` / `pn_transfer_payout_child` beside them are for
//! callers whose parent does not derive.
//!
//! NOTE: the dice state machine makes SettleBetV1 and HouseCloseV1 mutually exclusive on the same
//! bet (both consume the `Revealed` state). A single linear run can only green one of them. We green
//! CommitBet → RevealRoll → SettleBet (the main path), and assert HouseCloseV1 is REJECTED (the bet
//! is already `SettledHouse`). `target = 1` maximizes the house-win probability (the settle path
//! requires `roll >= target`, i.e. a player loss).
//!
//! The HouseCloseV1 row now *names* that reason and the contract that gives it
//! (`RejectionByEndpoint` with `Custom(3)`), which it could not say before: until `OBL-C192`'s unit A
//! the child reused the value seed as its leaf blind, so the block was refused at `call_idx=0` by
//! `promissory_note` and this endpoint's own state check was never reached — the row was green, and
//! green for a reason it did not name.

use dwow_contract_test_harness::harness::{DarkToshiDiceHarness, PromissoryNoteHarness};
use dwow_sdk::crypto::{
    poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::{
    pn_transfer_payout_prepare, pn_transfer_prepare, PnNote,
};
use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};

pub fn darktoshi_dice_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the harness must know the id its call will carry, because the transaction
    // commitment is derived over the call *including* the contract id. `deploy_with_ix` assigns
    // `derive_contract_id_from_name(name)` — a pure function of the name — so the spec computes
    // exactly the id the pipeline will use rather than a placeholder.
    let dice_cid = crate::tests::blockchain::derive_contract_id_from_name("darktoshi_dice");
    let harness = Box::leak(Box::new(DarkToshiDiceHarness::spawn(dice_cid)));
    let h: &DarkToshiDiceHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/darktoshi_dice/dwow_darktoshi_dice_contract.wasm");

    let player_secret = pallas::Base::from(1u64);
    let player_pub = PublicKey::from_secret(SecretKey::from_base(player_secret));
    let house_secret = pallas::Base::from(10u64);
    let issue_secret = pallas::Base::from(100u64);

    let bet_value: u64 = 1000;
    let target: u8 = 1; // minimize player-win probability (settle requires a loss)
    let secret_nonce = pallas::Base::from(99u64);
    let blind = pallas::Base::from(3u64);

    // bet_id stashed from the CommitBet result.
    let bet_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "darktoshi_dice",
        is_genesis: false,
        // `OBL-C198`: the real id, not the stale `[0u8;32]` placeholder — the commitment is
        // derived over the call, and the call carries this id.
        contract_id: dice_cid,
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // note 0: CommitBetV1 lock (1000)
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, bet_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let asset_id = token0.asset_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("w0");
                let mut issued = vec![
                    (token0.commitment.inner(), u64::from(mark0), path0, asset_id, pallas::Base::from(6u64)),
                ];

                // note 1: SettleBetV1 payout (1000 locked), note 2: HouseCloseV1 (1000 locked)
                for (commitment_blind, value) in [(7u64, bet_value), (8u64, bet_value)] {
                    let n = pn
                        .issue(issue_secret, asset_id, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(commitment_blind))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w");
                    issued.push((n.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(commitment_blind)));
                }
                *notes.lock().unwrap() = Some(issued);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "CommitBetV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bet_id = bet_id.clone();
                    move || {
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let asset_id = n[0].3;
                        // `OBL-C198`: this endpoint's child is seeded from `bet_id`, which only
                        // this call's data determines, and the commitment is a function of the
                        // child's data. So: the parent's call data first (no proof), the child
                        // built around the `bet_id` it derives, then ONE commitment over the
                        // ordered set (child first, parent last) with both proofs bound to it.
                        let plan = h.commit_bet_prepare(player_pub, bet_value, target, secret_nonce, blind, asset_id, 200u32)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let id = plan.public_inputs.bet_id;
                        *bet_id.lock().unwrap() = Some(id);
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), id]);
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(&n[0], bet_value, blind_seed, pallas::Base::zero())?;
                        let r = h.commit_bet_prove(plan, &[child_call.clone()])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(r.commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data, proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RevealRollV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let bet_id = bet_id.clone();
                    move || {
                        let id = bet_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet not committed".into()))?;
                        // `OBL-C198`: no child on this endpoint, so the committed set is the call
                        // alone — which is exactly `build_witness`'s single-call transaction.
                        let r = h.reveal_roll(&[], id, secret_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SettleBetV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bet_id = bet_id.clone();
                    move || {
                        let id = bet_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet not committed".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let asset_id = n[0].3;
                        let (px, py) = player_pub.xy().expect("pk not identity");
                        let block_hash = pallas::Base::from(42u64); // free witness (roll_hash is not cross-checked)
                        // `OBL-C198`: the child's call data first (no proof), the parent second,
                        // one commitment over the ordered set, the child proven against it.
                        // payout = 0 on a house win (target=1 ⇒ roll >= 1)
                        let blind_seed = poseidon_hash([pallas::Base::from(0u64), id]);
                        let (child_call, child_plan, child_nonce) = pn_transfer_payout_prepare(&n[1], bet_value, 0, blind_seed)?;
                        let r = h.settle_bet(&[child_call.clone()], id, px, py, pallas::Base::from(bet_value), pallas::Base::from(target as u64), secret_nonce, blind, asset_id, block_hash)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(r.commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data, proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "HouseCloseV1",
                is_zk: true,
                // The row names the check it is about, and it names the *contract* it comes from:
                // before `OBL-C192`'s unit A this block was refused at `call_idx=0` by the child's
                // `Duplicate commitment in output 0` and the endpoint never executed — a bare
                // `Rejection` was satisfied either way. `Custom(3)` is `DiceError::InvalidStateTransition`.
                expectation: EndpointExpectation::RejectionByEndpoint(&["ContractError(Custom(3))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bet_id = bet_id.clone();
                    move || {
                        let id = bet_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet not committed".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        // `OBL-C198`: child call data first, parent second, one commitment over
                        // the ordered set, the child proven against it. The endpoint is expected
                        // to be REJECTED, but that happens after verification, so both proofs
                        // still have to bind to the transaction they are in.
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), id]);
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(&n[2], bet_value, blind_seed, pallas::Base::zero())?;
                        let r = h.house_close(&[child_call.clone()], id, house_secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(r.commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data, proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
