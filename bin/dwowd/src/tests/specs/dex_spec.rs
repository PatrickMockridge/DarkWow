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

/// Build a PN TransferV1 (0x04) child call spending an issued note. Dex has no
/// `validate_child_value_commit` — the child's FuncRef (contract_id + 0x04) is the
/// ZK public input — so `blind_seed` is arbitrary. `note` is
/// (coin commitment, leaf pos, merkle path, asset_id, coin_blind).
fn pn_transfer_child(
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
) -> dwow_core::Result<ChildCall> {
    let (_, pos, path, asset_id, coin_blind) = note;
    let value_blind = Blind(fp_mod_fv(poseidon_hash([pallas::Base::from(value), pallas::Base::from(1u64)])).unwrap());
    let input = TransferCallInput {
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: *coin_blind,
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
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: pallas::Base::from(7u64),
    };
    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .transfer_with_value_blinds(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall {
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        call_data: child.call_data,
        proofs: child.proofs,
    })
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
    let harness = Box::leak(Box::new(DexHarness::spawn()));
    let h: &DexHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/dex/dwow_dex_contract.wasm");
    let s = pallas::Base::from(100u64);
    let ot = pallas::Base::from(1u64);
    let rt = pallas::Base::from(2u64);
    let sig = || SecretKey::from_bytes([1u8; 32]).unwrap();

    // Deterministic locks (match CreateSwapCallData::new_deterministic / AcceptSwap).
    // Both parties use secret `s`, so pubkey and blind are identical.
    let blind = poseidon_hash([s, pallas::Base::from(1u64)]);
    let pub_key = poseidon_hash([pallas::Base::from(7u64), s]);
    let alice_lock = poseidon_hash([pallas::Base::from(4u64), pub_key, ot, pallas::Base::from(1000u64), pallas::Base::zero(), pallas::Base::zero(), blind]);

    // Four distinct swaps, distinguished by Bob's request amount (swap_id = H(4, alice_lock, rt, bob_amount)).
    let mk_bob_lock = |amount: u64| {
        poseidon_hash([pallas::Base::from(4u64), pub_key, rt, pallas::Base::from(amount), pallas::Base::zero(), pallas::Base::zero(), blind])
    };
    let mk_swap_id = |bob_lock: pallas::Base, amount: u64| {
        poseidon_hash([pallas::Base::from(4u64), alice_lock, rt, pallas::Base::from(amount)])
    };

    // Swap E (Create/Accept/Execute endpoints, bob=500), B (fee, bob=600), C (slippage, bob=700), D (cancel, bob=800).
    let (bob_lock_e, swap_id_e) = (mk_bob_lock(500), mk_swap_id(mk_bob_lock(500), 500));
    let (bob_lock_b, swap_id_b) = (mk_bob_lock(600), mk_swap_id(mk_bob_lock(600), 600));
    let (bob_lock_c, swap_id_c) = (mk_bob_lock(700), mk_swap_id(mk_bob_lock(700), 700));
    let (bob_lock_d, swap_id_d) = (mk_bob_lock(800), mk_swap_id(mk_bob_lock(800), 800));

    let otc_func_id = FuncRef { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, func_code: 0x04 }.to_func_id().inner();

    // Issued PN capabilities (Alice 1000 ot, Bob 500 rt per Execute endpoint).
    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "dex",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            move |chain| {
                let cid = crate::tests::blockchain::derive_contract_id_from_name("dex");
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = pub_key;
                let issue_secret = s;

                // Register two token types: ot (1000) and rt (500).
                let token_ot = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, 1000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token_ot.call_data, token_ot.token_proofs.clone())?.submit())?;
                let asset_id_ot = token_ot.asset_id;

                let token_rt = pn
                    .register_type(issue_secret, pallas::Base::from(4u64), pallas::Base::from(5u64), owner_addr, 500, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token_rt.call_data, token_rt.token_proofs.clone())?.submit())?;
                let asset_id_rt = token_rt.asset_id;

                // Merkle tree mirrors the PN coin tree: zero guard + each commitment in order.
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
                // Two more notes of each type (one per remaining Execute endpoint).
                for (asset_id, value) in [(asset_id_ot, 1000u64), (asset_id_rt, 500u64), (asset_id_ot, 1000u64), (asset_id_rt, 500u64)] {
                    let n = pn
                        .issue(issue_secret, asset_id, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path = tree.witness(mark, 0).expect("w issue");
                    issued.push((n.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(6u64)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Pre-create the swaps the Execute/Cancel endpoints target.
                // Swap B (accepted, for ExecuteSwapFee) and C (accepted, for Slippage), D (created, for Cancel).
                for (bob_lock, bob_amount, accept) in [
                    (bob_lock_b, 600u64, true),
                    (bob_lock_c, 700u64, true),
                    (bob_lock_d, 800u64, false),
                ] {
                    let create = h.create_swap(s, ot, 1000, rt, bob_amount, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(cid, h, &create.call_data, vec![create.proof.clone()])?.submit())?;
                    if accept {
                        let a = h.accept_swap(create.public_inputs.swap_id, alice_lock, s, rt, bob_amount, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        smol::block_on(chain.block()?.with_call(cid, h, &a.call_data, vec![a.proof.clone()])?.submit())?;
                    }
                    let _ = bob_lock;
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
                    let r = h.create_swap(s, ot, 1000, rt, 500, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            mk_ep("AcceptSwapV1", true, Box::new(move || {
                let r = h.accept_swap(swap_id_e, alice_lock, s, rt, 500, sig()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
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
                    move || {
                        let r = h.execute_swap(s, ot, 1000, alice_lock, s, rt, 500, bob_lock_e, 499, otc_func_id, otc_func_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let c0 = pn_transfer_child(&n[0], 1000)?;
                        let c1 = pn_transfer_child(&n[1], 500)?;
                        Ok(EndpointResult { children: vec![c0, c1], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            mk_ep("CancelSwapV1", true, Box::new(move || {
                let r = h.cancel_swap(swap_id_d, alice_lock, s, ot, 1000, rt, 800).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            EndpointSpec {
                name: "ExecuteSwapFeeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: dex_vs_swap(swap_id_b),
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h.execute_swap_fee(s, ot, pallas::Base::from(1000u64), alice_lock, s, rt, pallas::Base::from(600u64), bob_lock_b, pallas::Base::from(599u64), pallas::Base::from(30u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let c0 = pn_transfer_child(&n[2], 1000)?;
                        let c1 = pn_transfer_child(&n[3], 500)?;
                        Ok(EndpointResult { children: vec![c0, c1], call_data: r.call_data, proofs: vec![r.proof] })
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
                    move || {
                        let r = h.execute_swap_slippage(s, ot, pallas::Base::from(1000u64), alice_lock, s, rt, pallas::Base::from(700u64), bob_lock_c, pallas::Base::from(699u64), pallas::Base::from(50u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let c0 = pn_transfer_child(&n[4], 1000)?;
                        let c1 = pn_transfer_child(&n[5], 500)?;
                        Ok(EndpointResult { children: vec![c0, c1], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            mk_ep("SetTransparencyLevelV1", false, Box::new(move || {
                let r = h.set_transparency_level(0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("UpdateConfigV1", false, Box::new(move || {
                let r = h.update_config(200, 50).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
