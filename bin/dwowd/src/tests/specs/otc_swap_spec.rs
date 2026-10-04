//! ContractTestSpec for otc_swap. FundSwapV1 and ExecuteSwapV1 each require one
//! promissory_note::transfer_v1 (0x04) child call whose output value_commit is
//! `send_value` with a per-endpoint blind seed (Fund: [send_value, swap_id];
//! Execute: [send_value, recv_value, swap_id]). CreateSwap/CancelSwap need no child.

use dwow_contract_test_harness::harness::{ContractHarness, OtcSwapHarness, PromissoryNoteHarness};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    poseidon_hash, util::fp_mod_fv, pasta_prelude::PrimeField, Blind, MerkleNode, MerkleTree,
    PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointResult, EndpointSpec, EndpointExpectation,
};

/// `OBL-C198`: build the PN TransferV1 (0x04) child call's **data and plan, without proving** —
/// the commitment is a derivation over the whole call set, so the blind-output proof is made only
/// after the parent's call data exists. Returns the ordered-set call, the plan to prove once the
/// commitment is known, and the nonce the plan binds.
fn pn_transfer_prepare(
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
    blind_seed: pallas::Base,
) -> dwow_core::Result<(
    dwow_sdk::tx::ContractCall,
    dwow_promissory_note_contract::client::transfer::TransferCallPlan,
    pallas::Base,
)> {
    let (note_commitment, pos, path, asset_id, commitment_blind) = note;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());
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
        commitment_blind: poseidon_hash([blind_seed, *note_commitment]),
    };
    let pn = PromissoryNoteHarness::spawn();
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

pub fn otc_swap_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the harness must know the id its call will carry, because the transaction
    // commitment is derived over the call *including* the contract id. `deploy_with_ix` assigns
    // `derive_contract_id_from_name(name)` — a pure function of the name — so the spec computes
    // exactly the id the pipeline will use rather than a placeholder.
    let otc_swap_cid = crate::tests::blockchain::derive_contract_id_from_name("otc_swap");
    let harness = Box::leak(Box::new(OtcSwapHarness::spawn(otc_swap_cid)));
    let h: &OtcSwapHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/otc_swap/dwow_otc_swap_contract.wasm");

    let alice_sk = pallas::Base::from(1u64);
    let bob_sk = pallas::Base::from(2u64);
    let alice_pub = PublicKey::from_secret(SecretKey::from_base(alice_sk));
    let bob_pub = PublicKey::from_secret(SecretKey::from_base(bob_sk));
    let send_value: u64 = 1000;
    let recv_value: u64 = 500;
    let send_asset_id = pallas::Base::from(3u64);
    let recv_asset_id = pallas::Base::from(4u64);
    let issue_secret = pallas::Base::from(100u64);

    // swap A id (CreateSwap endpoint → Fund + Execute). swap B id (setup pre-create → Cancel).
    let swap_a: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let swap_b: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    // Issued PN capabilities: note 0 (Fund child) + note 1 (Execute child), each value 1000.
    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "otc_swap",
        is_genesis: false,
        // `OBL-C198`: the real id, not the stale `[0u8;32]` placeholder — the commitment is derived
        // over the call including the id, and `deploy_with_ix` assigns `derive_contract_id_from_name`.
        contract_id: otc_swap_cid,
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let swap_b = swap_b.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, 1000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let asset_id = token0.asset_id;

                let n1 = pn
                    .issue(issue_secret, asset_id, owner_addr, 1000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(7u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n1.call_data, n1.proofs.clone())?.submit())?;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("w0");
                tree.append(MerkleNode::from_base(n1.commitment.inner()));
                let mark1 = tree.mark().unwrap();
                let path1: Vec<MerkleNode> = tree.witness(mark1, 0).expect("w1");
                *notes.lock().unwrap() = Some(vec![
                    (token0.commitment.inner(), u64::from(mark0), path0, asset_id, pallas::Base::from(6u64)),
                    (n1.commitment.inner(), u64::from(mark1), path1, asset_id, pallas::Base::from(7u64)),
                ]);

                // Pre-create swap B (timeout 200) for the CancelSwapV1 endpoint.
                let cid = crate::tests::blockchain::derive_contract_id_from_name("otc_swap");
                let r = h.create_swap(alice_sk, alice_pub, bob_pub, send_value, send_asset_id, recv_value, recv_asset_id, 200)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &r.call_data, vec![r.proof.clone()])?.submit())?;
                *swap_b.lock().unwrap() = Some(r.swap_id);
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
                verify_state: None,
                generate: Box::new({
                    let swap_a = swap_a.clone();
                    move || {
                        let r = h.create_swap(alice_sk, alice_pub, bob_pub, send_value, send_asset_id, recv_value, recv_asset_id, 100)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *swap_a.lock().unwrap() = Some(r.swap_id);
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "FundSwapV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let swap_a = swap_a.clone();
                    move || {
                        let id = swap_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("swap A not created".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(send_value), id]);
                        // `OBL-C198`: the child's call data is built first (no proof), then the
                        // parent's. The parent derives ONE commitment over the ordered set (child
                        // first, parent last) and the child is proven against that same value, so
                        // both proofs bind to the transaction they are in.
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(&n[0], send_value, blind_seed)?;
                        let r = h.fund_swap(&[child_call.clone()], send_value, pallas::Scalar::from(100u64), id, 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(r.commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data, proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "ExecuteSwapV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let swap_a = swap_a.clone();
                    move || {
                        let id = swap_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("swap A not created".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(send_value), pallas::Base::from(recv_value), id]);
                        // `OBL-C198`: as in FundSwap — child call data first, parent second, one
                        // commitment over the ordered set, the child proven against it.
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(&n[1], send_value, blind_seed)?;
                        let r = h.execute_swap(&[child_call.clone()], id, bob_sk, bob_pub, alice_pub, bob_pub)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(r.commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data, proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "CancelSwapV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let swap_b = swap_b.clone();
                    move || {
                        let id = swap_b.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("swap B not pre-created".into()))?;
                        let r = h.cancel_swap(id, alice_sk, alice_pub, 200, 201, alice_pub)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
