//! ContractTestSpec for bridge-core. DepositV1 requires one promissory_note::issue_v1 (0x02)
//! child call (mint the wrapped PN against the external deposit). WithdrawV1 requires one
//! promissory_note::redeem_v1 (0x01) child call (burn the wrapped PN → zero-value receipt).
use dwow_contract_test_harness::harness::{BridgeHarness, ContractHarness, PromissoryNoteHarness};
use dwow_sdk::crypto::{
    poseidon_hash, pasta_prelude::PrimeField, MerkleNode, MerkleTree,
    PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use dwow_bridge_contract::model::ExternalChain;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::*;

/// Replicate the bridge's deterministic wrapped-token derivation (mint-authority Option 1).
fn derive_issue_secret(bridge_cid: pallas::Base, chain: ExternalChain) -> pallas::Base {
    poseidon_hash([
        bridge_cid,
        pallas::Base::from(chain as u64),
        pallas::Base::from(0x62726964u64), // "brid"
    ])
}

fn derive_token_blind(chain: ExternalChain) -> pallas::Base {
    poseidon_hash([
        pallas::Base::from(chain as u64),
        pallas::Base::from(0x626c6e64u64), // "blnd"
    ])
}

fn derive_wrapped_asset_id(bridge_cid: pallas::Base, chain: ExternalChain) -> pallas::Base {
    let token_auth_parent = poseidon_hash([pallas::Base::from(7u64), derive_issue_secret(bridge_cid, chain)]);
    poseidon_hash([
        pallas::Base::from(2u64),
        token_auth_parent,
        pallas::Base::zero(),
        derive_token_blind(chain),
    ])
}

/// Build a PN IssueV1 (0x02) child call minting the wrapped PN to the depositor.
fn pn_issue_child(
    bridge_cid: pallas::Base,
    chain: ExternalChain,
    recipient: pallas::Base,
    value: u64,
) -> dwow_core::Result<ChildCall> {
    let asset_id = derive_wrapped_asset_id(bridge_cid, chain);
    let issue_secret = derive_issue_secret(bridge_cid, chain);
    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .issue(
            issue_secret,
            asset_id,
            recipient,
            value,
            bridge_cid,           // spend_hook = bridge
            pallas::Base::zero(), // user_data
            pallas::Base::from(7u64), // commitment_blind
        )
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall {
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        call_data: child.call_data,
        proofs: child.proofs,
    })
}

/// Build a PN RedeemV1 (0x01) child call burning a wrapped PN.
fn pn_redeem_child(
    bridge_cid: pallas::Base,
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
) -> dwow_core::Result<ChildCall> {
    let (_, pos, path, asset_id, commitment_blind) = note;
    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .redeem(
            value,
            *asset_id,
            bridge_cid,           // spend_hook = bridge
            pallas::Base::zero(), // user_data
            *commitment_blind,
            pallas::Base::from(100u64), // secret (the wrapped PN owner secret)
            bridge_cid,           // receipt recipient (issuer-visible)
            *pos,
            path.clone(),
        )
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
    let bridge_cid = cid.inner();

    // Issued wrapped-PN capabilities (tracked so WithdrawV1 can redeem them).
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

                // Register the wrapped token type for Ethereum (deterministic mint authority).
                let asset_id = derive_wrapped_asset_id(bridge_cid, ExternalChain::Ethereum);
                let issue_secret = derive_issue_secret(bridge_cid, ExternalChain::Ethereum);
                let token_blind = derive_token_blind(ExternalChain::Ethereum);
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::zero(), token_blind, owner_addr, 10000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;

                // Issue the initial wrapped PN that WithdrawV1 will later redeem.
                let n1 = pn
                    .issue(issue_secret, asset_id, owner_addr, 5000, bridge_cid, pallas::Base::zero(), pallas::Base::from(7u64))
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
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
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
                        let r = h.deposit(secret, 10000, recipient, 1, pallas::Base::from(200u64), ExternalChain::Ethereum, 0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = pn_issue_child(bridge_cid, ExternalChain::Ethereum, poseidon_hash([pallas::Base::from(7u64), secret]), 10000)?;
                        drop(notes.lock().unwrap());
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
                        let r = h.withdraw(secret, 5000, pallas::Base::from(400u64), 1, 10).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let child = pn_redeem_child(bridge_cid, &n[1], 5000)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
