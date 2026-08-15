use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness, StablecoinHarness};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    poseidon_hash, util::fp_mod_fv, BaseBlind, Blind, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// Build a PN TransferV1 child call spending an issued note, with the output
/// value_commit blind derived from `blind_seed` so it matches the parent's
/// validate_child_value_commit(amount, blind_seed). `note` is
/// (coin commitment, leaf pos, merkle path, token_id, coin_blind).
fn pn_transfer_child(
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
    blind_seed: pallas::Base,
    spend_hook: pallas::Base,
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
        recipient: pallas::Base::zero(),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::zero())),
        value,
        token_id: *token_id,
        spend_hook,
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

pub fn stablecoin_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(StablecoinHarness::spawn()));
    let h: &StablecoinHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/stablecoin/dwow_stablecoin_contract.wasm");
    let sk = pallas::Base::from(10u64);
    let cid = dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp");

    // Issued PN capabilities (one per child endpoint), shared between setup and the
    // child-call endpoints: (coin commitment, leaf pos, merkle path, token_id, coin_blind).
    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "stablecoin",
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
                let pn = PromissoryNoteHarness::spawn();
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let owner_secret = pallas::Base::from(100u64);
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), owner_secret]);
                let issue_secret = pallas::Base::from(1u64);
                let token0 = pn
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
                        .with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?
                        .submit(),
                )?;
                let token_id = token0.token_id;
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().expect("tree.mark");
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("tree.witness");
                let mut issued = vec![(token0.commitment.inner(), u64::from(mark0), path0, token_id, pallas::Base::from(6u64))];
                for (value, cb) in [(5000u64, 11u64), (1000, 12), (1000, 13), (500, 14), (5500, 15)] {
                    let n = pn
                        .issue(issue_secret, token_id, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(cb))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(
                        chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit(),
                    )?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().expect("tree.mark");
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("tree.witness");
                    issued.push((n.commitment.inner(), u64::from(mark), path, token_id, pallas::Base::from(cb)));
                }
                *notes.lock().unwrap() = Some(issued);
                Ok(())
            }
        })),
        endpoints: vec![
            EndpointSpec {
                name: "OpenPositionV1",
                is_zk: true,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h.open_position(sk, 10000, 5000, pallas::Base::from(1u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(10000u64), r.position_commitment]);
                        let child = pn_transfer_child(&n[0], 10000, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
                generate_with_coinbase: None,
                verify_state: Some(Box::new(move |chain| {
                    let r = chain.query_contract_state(cid, "positions", &[])?;
                    if r.is_none() {
                        return Err(dwow_core::Error::Custom("stablecoin positions not found".into()));
                    }
                    Ok(())
                })),
                expectation: EndpointExpectation::Success,
            },
            EndpointSpec {
                name: "MintStableV1",
                is_zk: true,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h.mint_stable(sk, 10000, 5000, 1000,
                            BaseBlind::from_u64(100u64), BaseBlind::from_u64(200u64),
                            pallas::Base::from(1u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(1000u64), r.public_inputs.new_commitment]);
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("stablecoin");
                        let child = pn_transfer_child(&n[3], 1000, blind_seed, cid.inner())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
                generate_with_coinbase: None,
                verify_state: Some(Box::new(move |chain| {
                    let r = chain.query_contract_state(cid, "stablecoin", &[])?;
                    if r.is_none() {
                        return Err(dwow_core::Error::Custom("stablecoin state not found".into()));
                    }
                    Ok(())
                })),
                expectation: EndpointExpectation::Success,
            },
            mk_ep("LiquidateV1", true, Box::new({
                let notes = notes.clone();
                move || {
                    let r = h.liquidate(sk, 10000, 5000, 200, 1000, 500,
                        BaseBlind::from_u64(100u64), BaseBlind::from_u64(200u64),
                        pallas::Base::from(1u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let n = notes.lock().unwrap();
                    let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                    let collateral_seized = 5000u64 + (5000u64 * 1000) / 10000;
                    let blind_seed = poseidon_hash([
                        pallas::Base::from(collateral_seized),
                        poseidon_hash([pallas::Base::from(5000u64), pallas::Base::from(10000u64)]),
                    ]);
                    let child = pn_transfer_child(&n[5], collateral_seized, blind_seed, pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("GovernanceReportV1", true, Box::new(move || {
                let r = h.governance_report(sk, 10000, 5000, 10, 3600, 42)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AccrueInterestV1", true, Box::new(move || {
                let r = h.accrue_interest(sk, 5000, 10, 3600)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AddCollateralV1", true, Box::new({
                let notes = notes.clone();
                move || {
                    use dwow_stablecoin_contract::model::{DepositCollateralParams, CollateralType};
                    use dwow_sdk::crypto::intent::IntentCommitment;
                    let params = DepositCollateralParams {
                        deposit_commitment: IntentCommitment::from_base(pallas::Base::from(1u64)),
                        collateral_amount: 5000,
                        collateral_type: CollateralType::Xmr,
                        proof: vec![],
                        fee: 0,
                        zk_public_inputs: vec![pallas::Base::from(1u64), pallas::Base::from(2u64)],
                    };
                    let r = h.add_collateral(&params)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let n = notes.lock().unwrap();
                    let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                    let blind_seed = poseidon_hash([pallas::Base::from(5000u64), pallas::Base::from(1u64)]);
                    let child = pn_transfer_child(&n[1], 5000, blind_seed, pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("RemoveCollateralV1", true, Box::new({
                let notes = notes.clone();
                move || {
                    use dwow_stablecoin_contract::model::WithdrawCollateralParams;
                    use dwow_sdk::crypto::intent::{IntentCommitment, IntentNullifier};
                    let params = WithdrawCollateralParams {
                        withdrawal_nullifier: IntentNullifier::from_base(pallas::Base::from(1u64)),
                        new_commitment: IntentCommitment::from_base(pallas::Base::from(2u64)),
                        withdraw_amount: 1000,
                        proof: vec![],
                        fee: 0,
                        zk_public_inputs: vec![pallas::Base::from(1u64)],
                    };
                    let r = h.remove_collateral(&params)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let n = notes.lock().unwrap();
                    let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                    let blind_seed = poseidon_hash([pallas::Base::from(1000u64), pallas::Base::from(1u64)]);
                    let child = pn_transfer_child(&n[2], 1000, blind_seed, pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("RepayStableV1", true, Box::new({
                let notes = notes.clone();
                move || {
                    use dwow_stablecoin_contract::model::RepayStableParams;
                    use dwow_sdk::crypto::intent::IntentCommitment;
                    let params = RepayStableParams {
                        repay_commitment: IntentCommitment::from_base(pallas::Base::from(1u64)),
                        repay_amount: 500,
                        proof: vec![],
                        fee: 0,
                        zk_public_inputs: vec![pallas::Base::from(1u64)],
                    };
                    let r = h.repay_stable(&params)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let n = notes.lock().unwrap();
                    let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                    let blind_seed = poseidon_hash([pallas::Base::from(500u64), pallas::Base::from(1u64)]);
                    let child = pn_transfer_child(&n[4], 500, blind_seed, pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("UpdateConfigV1", true, Box::new(move || {
                use dwow_stablecoin_contract::model::UpdateConfigParams;
                let params = UpdateConfigParams {
                    min_collateralization_ratio: 15000,
                    liquidation_threshold: 12000,
                    liquidation_penalty: 500,
                    base_rate: 500,
                    pi_kp: 0, pi_ki: 0,
                    twap_window: 3600,
                    price_deviation_threshold: 500,
                    gov_pub_x: pallas::Base::from(1u64),
                    gov_pub_y: pallas::Base::from(2u64),
                    config_nullifier: pallas::Base::from(3u64),
                };
                let r = h.update_config(&params)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
