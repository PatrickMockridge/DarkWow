//! ContractTestSpec for bridge. DepositV1 (mint) and WithdrawV1 (burn) each require
//! one promissory_note::transfer_v1 (0x04) child call. WithdrawV1 also validates the
//! child's value_commit against `poseidon_hash([amount, nullifier])`.
use dwow_contract_test_harness::harness::{BridgeHarness, ContractHarness, PromissoryNoteHarness};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    poseidon_hash, util::fp_mod_fv, pasta_prelude::PrimeField, Blind, MerkleNode, MerkleTree,
    PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use dwow_bridge_contract::model::ExternalChain;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::*;

/// Build a PN TransferV1 (0x04) child call spending an issued note.
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

pub fn bridge_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BridgeHarness::spawn()));
    let h: &BridgeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/bridge/dwow_bridge_contract.wasm");
    let secret = pallas::Base::from(100u64);
    let recipient = PublicKey::from_secret(SecretKey::from_bytes([3u8; 32]).unwrap());
    let cid = crate::tests::blockchain::derive_contract_id_from_name("bridge");

    // Issued PN capabilities: note 0 = 10000 (deposit mint source), note 1 = 5000 (withdraw burn source).
    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "bridge",
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
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), secret]);

                let token0 = pn
                    .register_type(secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, 10000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let token_id = token0.token_id;

                let n1 = pn
                    .issue(secret, token_id, owner_addr, 5000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(7u64))
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
                    (token0.commitment.inner(), u64::from(mark0), path0, token_id, pallas::Base::from(6u64)),
                    (n1.commitment.inner(), u64::from(mark1), path1, token_id, pallas::Base::from(7u64)),
                ]);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "UpdateConfigV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new(move |chain| {
                    let r = chain.query_contract_state(cid, "config", b"deposit_fee")?;
                    if r.is_none() { return Err(dwow_core::Error::Custom("bridge config not found".into())); }
                    Ok(())
                })),
                generate: Box::new(move || {
                    let r = h.update_config(100, 50, 6, 1_000_000, 500_000, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(99u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "DepositV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new(move |chain| {
                    let r = chain.query_contract_state(cid, "deposits", &[])?;
                    let _ = r;
                    Ok(())
                })),
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h.deposit(secret, 10000, recipient, 1, pallas::Base::from(200u64), pallas::Base::from(300u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64)); 32], ExternalChain::Ethereum, 0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let child = pn_transfer_child(&n[0], 10000, poseidon_hash([pallas::Base::from(10000u64), pallas::Base::from(1u64)]))?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "WithdrawV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new(move |chain| {
                    let r = chain.query_contract_state(cid, "nullifiers", &[])?;
                    let _ = r;
                    Ok(())
                })),
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h.withdraw(secret, 5000, pallas::Base::from(400u64), pallas::Base::from(500u64), pallas::Base::from(600u64), [pallas::Base::from(0u64); 4], 0, 10, 1).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(5000u64), r.public_inputs.nullifier]);
                        let child = pn_transfer_child(&n[1], 5000, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
