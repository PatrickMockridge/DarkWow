//! ContractTestSpec for escrow. FundV1 requires PN transfer_v1 (0x04) + Purse deposit_v1 (0x01)
//! children; ClaimV1 requires PN transfer_v1 (0x04) + Box take_v1 (0x02) children; RefundV1
//! requires one PN transfer_v1 (0x04) child. Claim/Refund validate the child value_commit against
//! `poseidon_hash([escrow.value, escrow.id])`.
//!
//! # `OBL-C198`: every row here is a nested-child case, and the order is the whole point
//!
//! The set the node hashes is `[child_a, child_b, this escrow call]`, so every proof in the
//! transaction must bind to the commitment over the *whole* set — a value that includes bytes
//! appearing after a child in DFS post-order. The three steps therefore run in one order and no
//! other:
//!
//! 1. the escrow call's data (`*_prepare`), which exposes the derived values a child is built
//!    around. `CreateEscrowV1` is the hard case and a genuine *cycle*, not merely an ordering: its
//!    child is a box whose `contents_commit` is a function of the escrow id this very call
//!    produces, and the commitment is a function of the child's data;
//! 2. the children's calls — `pn_transfer_prepare`, and the purse/box harnesses'
//!    `deposit_prepare` / `put_prepare` / `take_prepare`;
//! 3. ONE commitment over the ordered set (`*_prove`), which the escrow's proof and each child's
//!    proof all bind to.
//!
//! The purse and box two-phase forms were this migration's prerequisite and landed in
//! `757ca4a2f0`; the escrow harness's own split followed with it.

use dwow_contract_test_harness::harness::{
    BoxHarness, BoxPutPlan, BoxTakePlan, ContractHarness, EscrowHarness, PromissoryNoteHarness,
    PurseDepositPlan, PurseHarness,
};
use dwow_sdk::crypto::{
    poseidon_hash, pasta_prelude::PrimeField, MerkleNode, MerkleTree, PublicKey, SecretKey,
    BOX_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID, PURSE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::pn_transfer_prepare;
use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointResult, EndpointSpec, EndpointExpectation,
};

// `pn_transfer_child` stood here. The PN child now comes from
// `modules::child_calls::pn_transfer_prepare`, which stops short of the proof so the caller can
// bind it to the commitment over the whole ordered set (`OBL-C198`); the body was the same helper.

/// A `Purse::DepositV1` child's call data, **unproven** (`OBL-C198`).
///
/// **This can be used ONCE per fixture, and that constrains everything below it.**
/// `PurseHarness::deposit_prepare` builds its params from constants — `os = 42`, `purse_id = 1`,
/// `state_nonce = 0` — so the child's nullifier is `poseidon(1, 42, 1, 0)` for *every* call, whatever
/// the amount, and the purse contract refuses the second one with `Duplicate nullifier`. It also builds
/// its expected root from a fresh single-leaf tree, so it models one link of a purse's state chain
/// rather than the chain itself.
///
/// The consequence for this spec: `FundV1` requires a purse-deposit child, so **at most one escrow can
/// be funded per run**. That is why the refund row is absent (see the note where it stood) and why the
/// wrong-box row uses an unfunded escrow. Lifting it needs the harness to carry purse state across
/// deposits — a unit of its own, recorded in `OBL-C170`.
fn purse_deposit_plan(amount: u64) -> dwow_core::Result<PurseDepositPlan> {
    PurseHarness::spawn(*PURSE_CONTRACT_ID)
        .deposit_prepare(amount)
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))
}

/// A `Box::PutV1` child's call data, **unproven** — a box whose `contents_commit` is `contents`.
///
/// Putting the claim box rides in the **create** row rather than in `setup`, because the commitment
/// `ClaimV1` requires is derived from the escrow's own id and `setup` runs before the id exists. It needs
/// no purse deposit, so the one-deposit constraint above does not touch it.
fn box_put_plan(contents: pallas::Base) -> dwow_core::Result<BoxPutPlan> {
    BoxHarness::spawn().put_prepare(contents).map_err(|e| dwow_core::Error::Custom(format!("{e}")))
}

/// A `Box::TakeV1` child's call data, **unproven** — the box whose `contents_commit` is `contents`.
///
/// It takes the contents as an argument rather than defaulting, because **the escrow now requires a
/// specific one**: `ClaimV1` compares the child's `contents_commit` against the commitment the escrow
/// derived for itself, so a take of the fixture's default box would be refused — and that refusal is
/// one of the rows below.
fn box_take_plan(contents: pallas::Base) -> dwow_core::Result<BoxTakePlan> {
    BoxHarness::spawn().take_prepare(contents).map_err(|e| dwow_core::Error::Custom(format!("{e}")))
}

pub fn escrow_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the harness must know the id its calls will carry, because the transaction
    // commitment is derived over the call *including* the contract id. `deploy_with_ix` assigns
    // `derive_contract_id_from_name(name)` — a pure function of the name — so the spec computes
    // exactly the id the pipeline will use rather than a placeholder.
    let cid = crate::tests::blockchain::derive_contract_id_from_name("escrow");
    let harness = Box::leak(Box::new(EscrowHarness::spawn(cid)));
    let h: &EscrowHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/escrow/dwow_escrow_contract.wasm");

    let buyer_sk = pallas::Base::from(10u64);
    let buyer_pk = PublicKey::from_secret(SecretKey::from_base(buyer_sk));
    let seller_sk = pallas::Base::from(20u64);
    let seller_pk = PublicKey::from_secret(SecretKey::from_base(seller_sk));
    let value: u64 = 5000;
    let asset_id = pallas::Base::from(1u64);
    let timeout: u64 = 1000;
    let seed = [0u8; 32];
    let issue_secret = pallas::Base::from(100u64);

    let escrow_a: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "escrow",
        is_genesis: false,
        contract_id: cid,
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            move |chain| {
                let cid = crate::tests::blockchain::derive_contract_id_from_name("escrow");
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let tid = token0.asset_id;

                let mut issued = Vec::new();
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                issued.push((token0.commitment.inner(), u64::from(mark0), tree.witness(mark0, 0).expect("w0"), tid, pallas::Base::from(6u64)));

                // Two: one per spending row (`FundV1`, `ClaimV1`). A note is not
                // reusable — `pn_transfer_prepare` spends the one it is given — so a shared index surfaces
                // as a promissory-note double-spend rather than as whatever the row was about. The count
                // is exactly the number of rows that fund or claim; `setup` funds nothing, because the
                // fixture can fund only one escrow and a row must be the one to do it.
                for idx in 0..2 {
                    let n = pn
                        .issue(issue_secret, tid, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64 + idx as u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    issued.push((n.commitment.inner(), u64::from(mark), tree.witness(mark, 0).expect("wi"), tid, pallas::Base::from(8u64 + idx as u64)));
                }
                *notes.lock().unwrap() = Some(issued);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "CreateEscrowV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let escrow_a = escrow_a.clone();
                    move || {
                        // `OBL-C198`: this row is the cycle. The child's contents are a function of
                        // the id *this* call produces, so the parent's data must exist before the
                        // child can be built, and the commitment — which both proofs bind to — is a
                        // function of the child's data. Prepare, build the child, prove the set.
                        let plan = h.create_escrow_prepare(buyer_sk, buyer_pk, seller_pk, value, asset_id, timeout, seed)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        // The claim box is put here, in the same call tree as the escrow's creation,
                        // because its contents are a function of the id this call produces. The box the
                        // `ClaimV1` row then takes is this one, and the commitment it must carry is the
                        // one the escrow derived for itself — a take of any other box is refused.
                        let contents = dwow_escrow_contract::model::Escrow::derive_claim_box_contents(
                            dwow_escrow_contract::model::EscrowId(plan.public_inputs.commitment),
                            value,
                        );
                        let box_plan = box_put_plan(contents)?;
                        let box_call = dwow_sdk::tx::ContractCall { contract_id: *BOX_CONTRACT_ID, data: box_plan.call_data.clone() };
                        let r = h.create_escrow_prove(plan, &[box_call.clone()])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let box_proof = box_plan.prove(r.commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *escrow_a.lock().unwrap() = Some(r.public_inputs.commitment);
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *BOX_CONTRACT_ID, call_data: box_call.data, proofs: vec![box_proof.proof], children: vec![] }],
                            call_data: r.call_data,
                            proofs: vec![r.proof],
                        })
                    }
                }),
            },
            EndpointSpec {
                name: "FundV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let escrow_a = escrow_a.clone();
                    move || {
                        let ea = escrow_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("escrow A not created".into()))?;
                        // `OBL-C198`: parent's data, then the two children's, then ONE commitment
                        // over the ordered set — DFS post-order, so both children precede the escrow
                        // call — which all three proofs bind to.
                        let plan = h.fund_escrow_prepare(ea, value, pallas::Scalar::from(100u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(
                            &n[0], value, poseidon_hash([pallas::Base::from(value), ea, pallas::Base::from(1u64)]), pallas::Base::zero(),
                        )?;
                        let purse_plan = purse_deposit_plan(value)?;
                        let purse_call = dwow_sdk::tx::ContractCall { contract_id: *PURSE_CONTRACT_ID, data: purse_plan.call_data.clone() };
                        let r = h.fund_escrow_prove(plan, &[child_call.clone(), purse_call.clone()])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let pn_debris = child_plan.prove(r.commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let purse_proof = purse_plan.prove(r.commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child_pn = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data, proofs: pn_debris.proofs, children: vec![] };
                        let child_purse = ChildCall { contract_id: *PURSE_CONTRACT_ID, call_data: purse_call.data, proofs: vec![purse_proof.proof], children: vec![] };
                        Ok(EndpointResult { children: vec![child_pn, child_purse], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // **The positive row for the parent-side box binding.** Escrow A was created by the row above
            // — which also put the box at `derive_claim_box_contents(A, value)` in the same call tree —
            // and funded by the row before this one. So the take carries exactly the commitment A's record
            // holds, and the endpoint proceeds. This row is what makes its sibling below a *distinguishable*
            // failure rather than a blanket one.
            EndpointSpec {
                name: "ClaimV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let escrow_a = escrow_a.clone();
                    move || {
                        let ea = escrow_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("escrow A not created".into()))?;
                        let (sx, sy) = seller_pk.xy().expect("pk");
                        let seller_commitment = poseidon_hash([pallas::Base::from(4u64), sx, sy]);
                        // `OBL-C198`: as `FundV1` — parent's data, then both children, then ONE
                        // commitment over the ordered set that all three proofs bind to.
                        let plan = h.claim_escrow_prepare(ea, seller_sk, seller_pk, seller_commitment, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(
                            &n[1], value, poseidon_hash([pallas::Base::from(value), ea]), pallas::Base::zero(),
                        )?;
                        let contents = dwow_escrow_contract::model::Escrow::derive_claim_box_contents(
                            dwow_escrow_contract::model::EscrowId(ea),
                            value,
                        );
                        let box_plan = box_take_plan(contents)?;
                        let box_call = dwow_sdk::tx::ContractCall { contract_id: *BOX_CONTRACT_ID, data: box_plan.call_data.clone() };
                        let r = h.claim_escrow_prove(plan, &[child_call.clone(), box_call.clone()])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let pn_debris = child_plan.prove(r.commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let box_proof = box_plan.prove(r.commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child_pn = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data, proofs: pn_debris.proofs, children: vec![] };
                        let child_box = ChildCall { contract_id: *BOX_CONTRACT_ID, call_data: box_call.data, proofs: vec![box_proof.proof], children: vec![] };
                        Ok(EndpointResult { children: vec![child_pn, child_box], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // **Two rows have no place here, and both for the same measured reason.** `OBL-C170` carries
            // it, and it is the *harness* rather than this contract:
            //
            //   * **`ClaimV1_WrongBox`** — the row that would refuse a take of somebody else's box, which
            //     is the check `OBL-C169` added. `BoxHarness::take_prepare` derives
            //     `nf = poseidon(1, os=42, box_id=1, state_nonce=1)` — **independent of `contents`** — so
            //     one take is possible per fixture, and the row above has taken it. Measured, not
            //     inferred: the attempt is refused by *box* with `[box::take] Error: Duplicate nullifier`
            //     before this contract's exec runs at all, so the parent's check can never be the one that
            //     fires.
            //   * **`RefundV1`** — the endpoint needs an escrow in `Funded` state, which needs a
            //     `purse::deposit_v1` child, and `PurseHarness::deposit_prepare`'s nullifier is constant too. One
            //     funded escrow per fixture, and `ClaimV1` above consumes it.
            //
            // Both rows stood here and are removed rather than left to panic or to be refused by the
            // wrong contract. **What that means for the binding**: `ClaimV1`'s *success* path is verified
            // — the row above proves the endpoint still completes with a matching box in place — but the
            // **refusal** path is verified by reading only, because the fixture cannot build a second
            // take. That is stated in `OBL-C169` rather than glossed.
        ],
    }
}
