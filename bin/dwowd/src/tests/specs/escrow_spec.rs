//! ContractTestSpec for escrow. FundV1 requires PN transfer_v1 (0x04) + Purse deposit_v1 (0x01)
//! children; ClaimV1 requires PN transfer_v1 (0x04) + Box take_v1 (0x02) children; RefundV1
//! requires one PN transfer_v1 (0x04) child. Claim/Refund validate the child value_commit against
//! `poseidon_hash([escrow.value, escrow.id])`.

use dwow_contract_test_harness::harness::{
    BoxHarness, ContractHarness, EscrowHarness, PromissoryNoteHarness, PurseHarness,
};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    poseidon_hash, util::fp_mod_fv, pasta_prelude::PrimeField, Blind, MerkleNode, MerkleTree,
    PublicKey, SecretKey, BOX_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID, PURSE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointResult, EndpointSpec, EndpointExpectation,
};

fn pn_transfer_child(
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
    blind_seed: pallas::Base,
) -> dwow_core::Result<ChildCall> {
    let (_, pos, path, token_id, coin_blind) = note;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());
    let input = TransferCallInput {
        value,
        token_id: *token_id,
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
        token_id: *token_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: blind_seed,
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

fn purse_deposit_child(amount: u64) -> dwow_core::Result<ChildCall> {
    let purse = PurseHarness::spawn();
    let r = purse.deposit(amount).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall { contract_id: *PURSE_CONTRACT_ID, call_data: r.call_data, proofs: vec![r.proof] })
}

fn box_take_child() -> dwow_core::Result<ChildCall> {
    let bx = BoxHarness::spawn();
    let r = bx.take().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall { contract_id: *BOX_CONTRACT_ID, call_data: r.call_data, proofs: vec![r.proof] })
}

pub fn escrow_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(EscrowHarness::spawn()));
    let h: &EscrowHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/escrow/dwow_escrow_contract.wasm");

    let buyer_sk = pallas::Base::from(10u64);
    let buyer_pk = PublicKey::from_secret(SecretKey::from_base(buyer_sk));
    let seller_sk = pallas::Base::from(20u64);
    let seller_pk = PublicKey::from_secret(SecretKey::from_base(seller_sk));
    let value: u64 = 5000;
    let token_id = pallas::Base::from(1u64);
    let timeout: u64 = 1000;
    let seed = [0u8; 32];
    let issue_secret = pallas::Base::from(100u64);

    let escrow_a: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let escrow_b: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "escrow",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let escrow_b = escrow_b.clone();
            move |chain| {
                let cid = crate::tests::blockchain::derive_contract_id_from_name("escrow");
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let tid = token0.token_id;

                let mut issued = Vec::new();
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                issued.push((token0.commitment.inner(), u64::from(mark0), tree.witness(mark0, 0).expect("w0"), tid, pallas::Base::from(6u64)));

                for idx in 0..3 {
                    let n = pn
                        .issue(issue_secret, tid, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64 + idx as u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    issued.push((n.commitment.inner(), u64::from(mark), tree.witness(mark, 0).expect("wi"), tid, pallas::Base::from(8u64 + idx as u64)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Put a box (id 1) so the ClaimV1 Box::take child can consume it.
                let box_h = BoxHarness::spawn();
                let bx_put = box_h.put().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(*BOX_CONTRACT_ID, &box_h, &bx_put.call_data, vec![bx_put.proof.clone()])?.submit())?;

                // Pre-create + fund escrow B for the RefundV1 endpoint (distinct timeout).
                let r = h.create_escrow(buyer_sk, buyer_pk, seller_pk, value, token_id, timeout + 1, seed)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &r.call_data, vec![r.proof.clone()])?.submit())?;
                let eb = r.public_inputs.commitment;
                *escrow_b.lock().unwrap() = Some(eb);
                let f = h.fund_escrow(eb, value, pallas::Scalar::from(100u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let n = notes.lock().unwrap();
                let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                let child_pn = pn_transfer_child(&n[0], value, poseidon_hash([pallas::Base::from(value), eb, pallas::Base::from(1u64)]))?;
                let child_purse = purse_deposit_child(value)?;
                smol::block_on(chain.block()?.with_call_tree(cid, &f.call_data, vec![f.proof.clone()], vec![(child_pn.contract_id, child_pn.call_data.clone(), child_pn.proofs.clone()), (child_purse.contract_id, child_purse.call_data.clone(), child_purse.proofs.clone())])?.submit())?;
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
                        let r = h.create_escrow(buyer_sk, buyer_pk, seller_pk, value, token_id, timeout, seed)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *escrow_a.lock().unwrap() = Some(r.public_inputs.commitment);
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
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
                        let r = h.fund_escrow(ea, value, pallas::Scalar::from(100u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let child_pn = pn_transfer_child(&n[1], value, poseidon_hash([pallas::Base::from(value), ea, pallas::Base::from(1u64)]))?;
                        let child_purse = purse_deposit_child(value)?;
                        Ok(EndpointResult { children: vec![child_pn, child_purse], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
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
                        let r = h.claim_escrow(ea, seller_sk, seller_pk, seller_commitment, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let child_pn = pn_transfer_child(&n[2], value, poseidon_hash([pallas::Base::from(value), ea]))?;
                        let child_box = box_take_child()?;
                        Ok(EndpointResult { children: vec![child_pn, child_box], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RefundV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let escrow_b = escrow_b.clone();
                    let notes = notes.clone();
                    move || {
                        let eb = escrow_b.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("escrow B not pre-created".into()))?;
                        let (bx, by) = buyer_pk.xy().expect("pk");
                        let r = h.refund_escrow(eb, timeout + 1, timeout + 2, buyer_sk, buyer_pk, bx, by, buyer_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let child_pn = pn_transfer_child(&n[3], value, poseidon_hash([pallas::Base::from(value), eb]))?;
                        Ok(EndpointResult { children: vec![child_pn], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
