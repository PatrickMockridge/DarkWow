use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness, StablecoinHarness};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    poseidon_hash, util::fp_mod_fv, pasta_prelude::PrimeField, BaseBlind, Blind, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use dwow_stablecoin_contract::model::{DeadManAction, DeadManSwitchConfig, InitializeParams, StablecoinModel};
use dwow_stablecoin_contract::{
    CDP_BASE_RATE, CDP_LIQUIDATION_PENALTY, CDP_LIQUIDATION_THRESHOLD,
    CDP_MIN_COLLATERALIZATION_RATIO, CDP_PI_KI, CDP_PI_KP, CDP_PRICE_DEVIATION_THRESHOLD,
    CDP_PRICE_FEED_TWAP_WINDOW,
};
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// Build a PN TransferV1 child call spending an issued note, with the output
/// value_commit blind derived from `blind_seed` so it matches the parent's
/// validate_child_value_commit(amount, blind_seed). `note` is
/// (coin commitment, leaf pos, merkle path, asset_id, commitment_blind).
fn pn_transfer_child(
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
    blind_seed: pallas::Base,
    spend_hook: pallas::Base,
) -> dwow_core::Result<ChildCall> {
    let (_, pos, path, asset_id, commitment_blind) = note;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());
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
        tx_nonce: pallas::Base::zero(),
    };
    let output = TransferCallOutput {
        recipient: poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(200u64)]),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(200u64))),
        value,
        asset_id: *asset_id,
        spend_hook,
        user_data: pallas::Base::zero(),
        commitment_blind: pallas::Base::from(7u64),
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
    // child-call endpoints: (coin commitment, leaf pos, merkle path, asset_id, commitment_blind).
    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    // Runtime-captured commitments for verify_state (cross-block state check).
    let position_commitment: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let mint_commitment: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    // Deployment init ix: store the PN contract id so validate_child_contract_id passes.
    let deploy_ix = InitializeParams {
        model: StablecoinModel::PooledDebt,
        min_collateralization_ratio: CDP_MIN_COLLATERALIZATION_RATIO,
        liquidation_threshold: CDP_LIQUIDATION_THRESHOLD,
        liquidation_penalty: CDP_LIQUIDATION_PENALTY,
        base_rate: CDP_BASE_RATE,
        pi_kp: CDP_PI_KP,
        pi_ki: CDP_PI_KI,
        twap_window: CDP_PRICE_FEED_TWAP_WINDOW,
        price_deviation_threshold: CDP_PRICE_DEVIATION_THRESHOLD,
        collateral_params: vec![],
        dead_man_switch: DeadManSwitchConfig {
            enabled: false,
            timeout_blocks: 0,
            action: DeadManAction::DisableMinting,
            last_action_block: 0,
        },
        token_authority_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(1u64))),
        create_token: false,
        token_symbol: [0u8; 32],
        deployer_auth: pallas::Base::zero(),
        promissory_note_contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
    }
    .encode();

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
                let issue_secret = pallas::Base::from(100u64);
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
                let asset_id = token0.asset_id;
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().expect("tree.mark");
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("tree.witness");
                let mut issued = vec![(token0.commitment.inner(), u64::from(mark0), path0, asset_id, pallas::Base::from(6u64))];
                for (value, cb) in [(5000u64, 11u64), (1000, 12), (1000, 13), (500, 14), (5500, 15)] {
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
                Ok(())
            }
        })),
        deploy_ix: Some(deploy_ix),
        endpoints: vec![
            EndpointSpec {
                name: "OpenPositionV1",
                is_zk: true,
                generate: Box::new({
                    let notes = notes.clone();
                    let position_commitment = position_commitment.clone();
                    move || {
                        let r = h.open_position(sk, 10000, 5000, pallas::Base::from(1u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *position_commitment.lock().unwrap() = Some(r.position_commitment);
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(10000u64), r.position_commitment]);
                        let child = pn_transfer_child(&n[0], 10000, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let position_commitment = position_commitment.clone();
                    move |chain| {
                        let key = position_commitment.lock().unwrap()
                            .ok_or_else(|| dwow_core::Error::Custom("position commitment not captured".into()))?
                            .to_repr();
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("stablecoin");
                        let r = chain.query_contract_state(cid, "positions", &key)?;
                        if r.is_none() {
                            return Err(dwow_core::Error::Custom("stablecoin positions not found".into()));
                        }
                        Ok(())
                    }
                })),
                expectation: EndpointExpectation::Success,
            },
            EndpointSpec {
                name: "MintStableV1",
                is_zk: true,
                generate: Box::new({
                    let notes = notes.clone();
                    let mint_commitment = mint_commitment.clone();
                    move || {
                        let r = h.mint_stable(sk, 10000, 5000, 1000,
                            BaseBlind::from_u64(100u64), BaseBlind::from_u64(200u64),
                            pallas::Base::from(1u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *mint_commitment.lock().unwrap() = Some(r.public_inputs.new_commitment);
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(1000u64), r.public_inputs.new_commitment]);
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("stablecoin");
                        let child = pn_transfer_child(&n[3], 1000, blind_seed, cid.inner())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let mint_commitment = mint_commitment.clone();
                    move |chain| {
                        let key = mint_commitment.lock().unwrap()
                            .ok_or_else(|| dwow_core::Error::Custom("mint commitment not captured".into()))?
                            .to_repr();
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("stablecoin");
                        let r = chain.query_contract_state(cid, "stablecoin", &key)?;
                        if r.is_none() {
                            return Err(dwow_core::Error::Custom("stablecoin state not found".into()));
                        }
                        Ok(())
                    }
                })),
                expectation: EndpointExpectation::Success,
            },
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
            mk_ep("GovernanceReportV1", true, Box::new(move || {
                let r = h.governance_report(sk, 10000, 500, 10, 3600, 42)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AccrueInterestV1", true, Box::new(move || {
                let r = h.accrue_interest(sk, 500, 10, 3600)
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
                        withdrawal_nullifier: IntentNullifier::from_base(pallas::Base::from(3u64)),
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
                    let blind_seed = poseidon_hash([pallas::Base::from(1000u64), pallas::Base::from(3u64)]);
                    let child = pn_transfer_child(&n[2], 1000, blind_seed, pallas::Base::zero())?;
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
                    gov_pub_x: pallas::Base::zero(),
                    gov_pub_y: pallas::Base::zero(),
                    config_nullifier: poseidon_hash([pallas::Base::from(1u64), pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero()]),
                };
                let r = h.update_config(&params)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
