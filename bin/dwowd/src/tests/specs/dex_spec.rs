use dwow_contract_test_harness::harness::{ContractHarness, DexHarness, PromissoryNoteHarness};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    poseidon_hash, util::fp_mod_fv, pasta_prelude::PrimeField, Blind, FuncRef, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};
use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// Assemble the PN TransferV1 (0x04) child's call data and **stop, before the proof exists**.
/// `note` is (commitment, leaf pos, merkle path, asset_id, commitment_blind).
///
/// Returns the ordered-set call, the plan to prove once the commitment is known, and the nonce the
/// plan binds (zero — this contract's calls carry no nonce field).
///
/// **This used to build the proof here**, through `transfer_with_value_blinds`, which binds each
/// call's *own* commitment. A child that binds its own commitment is refused the moment its parent's
/// arm derives the frame's — the composition red `OBL-C170`/`OBL-C198` measure, and the reason the
/// PromissoryNote harness grew the `*_prepare` half (`dao_escrow_spec.rs::pn_transfer_prepare` is the
/// working copy this one follows). Nothing noticed, because this spec had no caller: it was the
/// first run of `dex_test_spec()` that reached the endpoints and reported
/// `invalid proof: call[0] namespace 'Revoke_V2'` at block acceptance.
fn pn_transfer_prepare(
    pn: &PromissoryNoteHarness,
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
) -> dwow_core::Result<(
    dwow_sdk::tx::ContractCall,
    dwow_promissory_note_contract::client::transfer::TransferCallPlan,
    pallas::Base,
)> {
    let (note_commitment, pos, path, asset_id, commitment_blind) = note;
    let leaf_seed = poseidon_hash([pallas::Base::from(value), pallas::Base::from(1u64)]);
    let value_blind = Blind(fp_mod_fv(leaf_seed).unwrap());
    let nonce = pallas::Base::zero();
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
        tx_nonce: nonce,
    };
    let output = TransferCallOutput {
        recipient: poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(200u64)]),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(200u64))),
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: poseidon_hash([leaf_seed, *note_commitment]),
    };
    let plan = pn
        .transfer_prepare(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    let mut call_data = vec![0x04u8];
    call_data.extend_from_slice(
        &plan.params().encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?,
    );
    let call = dwow_sdk::tx::ContractCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, data: call_data };
    Ok((call, plan, nonce))
}

/// verify_state: check the swap exists in the dex "swaps" tree under the derived cid.
fn dex_vs_swap(swap_id: pallas::Base) -> Option<Box<dyn Fn(&HeavyweightPipeline) -> dwow_core::Result<()> + 'static>> {
    let cid = crate::tests::blockchain::derive_contract_id_from_name("dex");
    let key = swap_id.to_repr().to_vec();
    Some(Box::new(move |chain: &HeavyweightPipeline| {
        let r = chain.query_contract_state(cid, "swaps", &key)?;
        if r.is_none() {
            return Err(dwow_core::Error::Custom("dex swap not found".into()));
        }
        Ok(())
    }))
}

pub fn dex_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the harness must know the id its calls will carry, because the transaction
    // commitment is derived over the call *including* the contract id. `deploy_with_ix` assigns
    // `derive_contract_id_from_name(name)` — a pure function of the name — so the spec computes
    // exactly the id the pipeline will use rather than a placeholder.
    let cid = crate::tests::blockchain::derive_contract_id_from_name("dex");
    let harness = Box::leak(Box::new(DexHarness::spawn(cid)));
    let h: &DexHarness = harness;
    // Leaked and shared, the way `dao_escrow_spec.rs` does it and for the same two reasons: every
    // `spawn` rebuilds the PromissoryNote proving keys and this spec registers two token types and
    // issues six notes, and the rows' child transfers must be built by the *same* harness that did
    // the registrations (it is the harness that records the registry tree, `registered`).
    let pn: &'static PromissoryNoteHarness = Box::leak(Box::new(PromissoryNoteHarness::spawn()));
    let wasm = include_bytes!("../../../../../src/contract/dex/dwow_dex_contract.wasm");
    let s = pallas::Base::from(100u64);
    let ot = pallas::Base::from(1u64);
    let rt = pallas::Base::from(2u64);
    let sig = || SecretKey::from_bytes([1u8; 32]).unwrap();

    // Deterministic locks (match CreateSwapCallData::new_deterministic / AcceptSwap).
    // Bob uses secret `s`, so his pubkey and blind are the same in every swap he accepts.
    let blind = poseidon_hash([s, pallas::Base::from(1u64)]);
    let pub_key = poseidon_hash([pallas::Base::from(7u64), s]);

    // **Alice (the proposer) needs a distinct identity per swap, and until 2026-10-12 she did not
    // have one.** `create_swap` derives `proposer_nullifier = H(1, secret, lock_commitment)` and
    // `create_swap`'s apply stores it as a spend-once marker (`db_mark_spent`); both `execute_swap*`'s
    // and `cancel_swap`'s apply delete it (`db_del`); and `cancel_swap`'s exec refuses a swap whose
    // proposer marker is gone (`db_contains_key`). All four swaps offered `ot/1000` under the one
    // secret `s`, so they shared one lock and therefore one nullifier: the first execution spends the
    // shared marker, and wherever the rows execute before they cancel, the `CancelSwapV1` row then
    // reports `[CancelSwapV1] Error: Nullifier does not match any participant`. Four independent
    // propositions are four independent proposers — each gets its own secret, and so its own lock and
    // nullifier. Bob keeps `s`: his locks already differ by request amount.
    let alice_lock_of = |secret: pallas::Base| {
        let blind = poseidon_hash([secret, pallas::Base::from(1u64)]);
        let pub_key = poseidon_hash([pallas::Base::from(7u64), secret]);
        poseidon_hash([pallas::Base::from(4u64), pub_key, ot, pallas::Base::from(1000u64), pallas::Base::zero(), pallas::Base::zero(), blind])
    };
    let s_e = pallas::Base::from(101u64);
    let s_b = pallas::Base::from(102u64);
    let s_c = pallas::Base::from(103u64);
    let s_d = pallas::Base::from(104u64);
    let (alice_lock_e, alice_lock_b, alice_lock_c, alice_lock_d) =
        (alice_lock_of(s_e), alice_lock_of(s_b), alice_lock_of(s_c), alice_lock_of(s_d));

    // Four distinct swaps, distinguished by Alice's lock and Bob's request amount
    // (swap_id = H(4, alice_lock, rt, bob_amount)).
    let mk_bob_lock = |amount: u64| {
        poseidon_hash([pallas::Base::from(4u64), pub_key, rt, pallas::Base::from(amount), pallas::Base::zero(), pallas::Base::zero(), blind])
    };
    let mk_swap_id = |alice_lock: pallas::Base, amount: u64| {
        poseidon_hash([pallas::Base::from(4u64), alice_lock, rt, pallas::Base::from(amount)])
    };

    // Swap E (Create/Accept/Execute endpoints, bob=500), B (fee, bob=600), C (slippage, bob=700),
    // D (cancel, bob=800). D is never accepted, so it needs no Bob's-side lock.
    let (bob_lock_e, swap_id_e) = (mk_bob_lock(500), mk_swap_id(alice_lock_e, 500));
    let (bob_lock_b, swap_id_b) = (mk_bob_lock(600), mk_swap_id(alice_lock_b, 600));
    let (bob_lock_c, swap_id_c) = (mk_bob_lock(700), mk_swap_id(alice_lock_c, 700));
    let swap_id_d = mk_swap_id(alice_lock_d, 800);

    let otc_func_id = FuncRef { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, func_code: 0x04 }.to_func_id().inner();

    // Issued PN capabilities (Alice 1000 ot, Bob 500 rt per Execute endpoint).
    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "dex",
        is_genesis: false,
        contract_id: cid,
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let pn = pn;
            move |chain| {
                let cid = crate::tests::blockchain::derive_contract_id_from_name("dex");
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let owner_addr = pub_key;
                let issue_secret = s;

                // Register two token types: ot (1000) and rt (500).
                let token_ot = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, 1000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, pn, &token_ot.call_data, token_ot.token_proofs.clone())?.submit())?;
                let asset_id_ot = token_ot.asset_id;

                let token_rt = pn
                    .register_type(issue_secret, pallas::Base::from(4u64), pallas::Base::from(5u64), owner_addr, 500, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, pn, &token_rt.call_data, token_rt.token_proofs.clone())?.submit())?;
                let asset_id_rt = token_rt.asset_id;

                // Merkle tree mirrors the PN commitment tree: zero guard + each commitment in order.
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token_ot.commitment.inner()));
                let mark_ot = tree.mark().unwrap();
                let path_ot: Vec<MerkleNode> = tree.witness(mark_ot, 0).expect("w ot");
                tree.append(MerkleNode::from_base(token_rt.commitment.inner()));
                let mark_rt = tree.mark().unwrap();
                let path_rt: Vec<MerkleNode> = tree.witness(mark_rt, 0).expect("w rt");
                let mut issued = vec![
                    (token_ot.commitment.inner(), u64::from(mark_ot), path_ot, asset_id_ot, pallas::Base::from(6u64)),
                    (token_rt.commitment.inner(), u64::from(mark_rt), path_rt, asset_id_rt, pallas::Base::from(6u64)),
                ];
                // Two more notes of each type (one per remaining Execute endpoint), plus a fifth for
                // the `CancelSwapV1` row's refund child — that endpoint requires exactly one
                // `promissory_note::transfer_v1` child and every other note is spent by the rows
                // above it.
                //
                // **Each needs its own blind, and this loop did not give them one until 2026-10-12.**
                // It issued all four with the constant `6` — the same blind the two registered types
                // above used — so note 1 repeated `token_ot`'s commitment and notes 3 and 4 repeated
                // notes 1 and 2. `promissory_note::issue_v1` refuses a commitment it already holds
                // (`Custom(14)`, "Commitment already exists"), so the setup could not reach its second
                // block: the spec failed there and never ran an endpoint. Nothing had run it — this
                // spec had no caller before `test_heavyweight_dex_governance` — which is why a defect
                // this plain sat in it.
                for (idx, (asset_id, value)) in [(asset_id_ot, 1000u64), (asset_id_rt, 500u64), (asset_id_ot, 1000u64), (asset_id_rt, 500u64), (asset_id_ot, 1000u64)]
                    .into_iter()
                    .enumerate()
                {
                    let blind = pallas::Base::from(7u64 + idx as u64);
                    let n = pn
                        .issue(issue_secret, asset_id, value, pallas::Base::zero(), pallas::Base::zero(), blind)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path = tree.witness(mark, 0).expect("w issue");
                    issued.push((n.commitment.inner(), u64::from(mark), path, asset_id, blind));
                }
                *notes.lock().unwrap() = Some(issued);

                // Pre-create the swaps the Execute/Cancel endpoints target.
                // Swap B (accepted, for ExecuteSwapFee) and C (accepted, for Slippage), D (created, for Cancel).
                // Each is proposed by Alice under her own secret/lock (`s_b`/`s_c`/`s_d`), so the three
                // create calls do not contend for one spend-once nullifier; Bob (`s`) accepts B and C.
                for (proposer_secret, alice_lock, bob_amount, accept) in [
                    (s_b, alice_lock_b, 600u64, true),
                    (s_c, alice_lock_c, 700u64, true),
                    (s_d, alice_lock_d, 800u64, false),
                ] {
                    let create = h.create_swap(&[], proposer_secret, ot, 1000, rt, bob_amount, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(cid, h, &create.call_data, vec![create.proof.clone()])?.submit())?;
                    if accept {
                        let a = h.accept_swap(&[], create.public_inputs.swap_id, alice_lock, s, rt, bob_amount, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        smol::block_on(chain.block()?.with_call(cid, h, &a.call_data, vec![a.proof.clone()])?.submit())?;
                    }
                }
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "CreateSwapV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: dex_vs_swap(swap_id_e),
                generate: Box::new(move || {
                    let r = h.create_swap(&[], s_e, ot, 1000, rt, 500, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            mk_ep("AcceptSwapV1", true, Box::new(move || {
                let r = h.accept_swap(&[], swap_id_e, alice_lock_e, s, rt, 500, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            EndpointSpec {
                name: "ExecuteSwapV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: dex_vs_swap(swap_id_e),
                generate: Box::new({
                    let notes = notes.clone();
                    let pn = pn;
                    move || {
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        // `OBL-C198`: both children's call data first, so the parent's proof binds
                        // them, then each child is proven against the commitment the parent returns.
                        let (c0, c0_plan, c0_nonce) = pn_transfer_prepare(pn, &n[0], 1000)?;
                        let (c1, c1_plan, c1_nonce) = pn_transfer_prepare(pn, &n[1], 500)?;
                        let r = h.execute_swap(&[c0.clone(), c1.clone()], s_e, ot, 1000, alice_lock_e, s, rt, 500, bob_lock_e, 499, otc_func_id, otc_func_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let d0 = c0_plan.prove(r.commitment, c0_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let d1 = c1_plan.prove(r.commitment, c1_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![
                                ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: c0.data, proofs: d0.proofs, children: vec![] },
                                ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: c1.data, proofs: d1.proofs, children: vec![] },
                            ],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            mk_ep("CancelSwapV1", true, Box::new({
                let notes = notes.clone();
                let pn = pn;
                move || {
                    let n = notes.lock().unwrap();
                    let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                    // `cancel_swap_v1` requires exactly one `promissory_note::transfer_v1` child —
                    // the refund path — and refuses a call with none (`InvalidChildrenIndexes`). This
                    // row passed `&[]` and could never have run: it is the fourth defect this spec's
                    // first execution found, and all four are the same sentence — the rows were
                    // written against a fixture that had never been executed.
                    let (child, child_plan, child_nonce) = pn_transfer_prepare(pn, &n[6], 1000)?;
                    let r = h.cancel_swap(&[child.clone()], swap_id_d, alice_lock_d, s_d, ot, 1000, rt, 800).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let debris = child_plan.prove(r.commitment, child_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child.data, proofs: debris.proofs, children: vec![] }],
                        call_data: r.call_data, proofs: vec![r.proof],
                    })
                }
            })),
            EndpointSpec {
                name: "ExecuteSwapFeeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: dex_vs_swap(swap_id_b),
                generate: Box::new({
                    let notes = notes.clone();
                    let pn = pn;
                    move || {
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let (c0, c0_plan, c0_nonce) = pn_transfer_prepare(pn, &n[2], 1000)?;
                        let (c1, c1_plan, c1_nonce) = pn_transfer_prepare(pn, &n[3], 500)?;
                        let r = h.execute_swap_fee(&[c0.clone(), c1.clone()], s_b, ot, pallas::Base::from(1000u64), alice_lock_b, s, rt, pallas::Base::from(600u64), bob_lock_b, pallas::Base::from(599u64), pallas::Base::from(30u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let d0 = c0_plan.prove(r.commitment, c0_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let d1 = c1_plan.prove(r.commitment, c1_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![
                                ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: c0.data, proofs: d0.proofs, children: vec![] },
                                ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: c1.data, proofs: d1.proofs, children: vec![] },
                            ],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            EndpointSpec {
                name: "ExecuteSwapSlippageV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: dex_vs_swap(swap_id_c),
                generate: Box::new({
                    let notes = notes.clone();
                    let pn = pn;
                    move || {
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let (c0, c0_plan, c0_nonce) = pn_transfer_prepare(pn, &n[4], 1000)?;
                        let (c1, c1_plan, c1_nonce) = pn_transfer_prepare(pn, &n[5], 500)?;
                        // The tolerance must be one the circuit accepts. `execute_swap_slippage.zk`
                        // (PHASE 6) proves `(BPS - slippage_bps) * alice_amount <= fill_amount * BPS`,
                        // `BPS = 10000` — the fill may fall short of Alice's *offer* by at most
                        // `slippage_bps`. This row fills 699 of Alice's 1000, a 30.1% shortfall, so the
                        // tolerance must be >= 3010; the 50 bps it carried is refuted at `L2 proof
                        // verify` as an *unsatisfiable witness* (`right = 9950 * 1000 = 9_950_000`,
                        // `left = 699 * 10000 = 6_990_000`, `right <= left` false) — the exec itself had
                        // already returned `Swap executed successfully with slippage`, so this is not an
                        // instance mismatch. 3500 clears the bound with margin and stays under the
                        // circuit's 100% cap. (The fee row needs no such value: `execute_swap_fee.zk`
                        // carries only `fill <= alice_amount` and `fill < bob_amount`, both of which
                        // 599/1000 and 599/600 satisfy.)
                        let r = h.execute_swap_slippage(&[c0.clone(), c1.clone()], s_c, ot, pallas::Base::from(1000u64), alice_lock_c, s, rt, pallas::Base::from(700u64), bob_lock_c, pallas::Base::from(699u64), pallas::Base::from(3500u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let d0 = c0_plan.prove(r.commitment, c0_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let d1 = c1_plan.prove(r.commitment, c1_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![
                                ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: c0.data, proofs: d0.proofs, children: vec![] },
                                ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: c1.data, proofs: d1.proofs, children: vec![] },
                            ],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            mk_ep("SetTransparencyLevelV1", true, Box::new(move || {
                let r = h.set_transparency_level(&[], 0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("UpdateConfigV1", true, Box::new(move || {
                let r = h.update_config(&[], 200, 50).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
