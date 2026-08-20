//! ContractTestSpec for betting_stake. Spec: heavyweight-spec.md §5.9.
//!
//! Money flow: InitializeV1 creates the table (no child); StakeV1 locks capital (1:1 PN child);
//! UpdateRiskV1 records a house-edge payout (no child — adds `accumulated_earnings` for the claim
//! step); ClaimEarningsV1 pays out the earnings share (payout child); UnstakeV1 withdraws stake +
//! earnings (payout+change child). Uses the shared `modules::child_calls` helpers.
//!
//! Note: endpoint order is Initialize → Stake → UpdateRisk → Claim → Unstake (the state machine
//! needs `accumulated_earnings > 0` before ClaimEarningsV1 can succeed, so UpdateRisk precedes it).

use dwow_contract_test_harness::harness::{BettingStakeHarness, ClaimStakeInfo, UnstakeStakeInfo};
use dwow_sdk::crypto::{
    poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::{
    pn_transfer_child, pn_transfer_payout_child, PnNote,
};
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};

pub fn betting_stake_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BettingStakeHarness::spawn()));
    let h: &BettingStakeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/betting_stake/dwow_betting_stake_contract.wasm");

    let staker_secret = SecretKey::from_bytes([1u8; 32]).unwrap();
    let staker_pub = PublicKey::from_secret(staker_secret.clone());
    let issue_secret = pallas::Base::from(100u64);

    let betting_contract_id = pallas::Base::from(1u64);
    let nonce = pallas::Base::zero();
    let amount: u64 = 1000;

    // Deterministic ids (nonce=0, matching the harness). The table_id is derived in the InitializeV1
    // exec and the stake_id in the StakeV1 exec, both with domain 4 + the zero nonce.
    let table_id = poseidon_hash([pallas::Base::from(4u64), betting_contract_id, nonce]);
    let (sx, sy) = staker_pub.xy().expect("pk not identity");
    let stake_id = poseidon_hash([
        pallas::Base::from(4u64),
        table_id,
        sx,
        sy,
        pallas::Base::from(amount),
        nonce,
    ]);

    // Deterministic payouts (UpdateRiskV1: payout 5000, house_share 5000 → staker_loss 0,
    // house_edge_earnings = (5000 * 200)/10000 = 100). ClaimEarningsV1 claims 100; UnstakeV1
    // pays out stake (1000) + earnings share (100) = 1100.
    let claimable: u64 = 100;
    let unstake_payout: u64 = 1100;

    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "betting_stake",
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
                let pn = dwow_contract_test_harness::harness::PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // note 0: StakeV1 lock (1000)
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, amount, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
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

                // note 1: ClaimEarningsV1 payout (1000 locked), note 2: UnstakeV1 payout (2000 locked)
                for (coin_blind, value) in [(7u64, 1000u64), (8u64, 2000u64)] {
                    let n = pn
                        .issue(issue_secret, asset_id, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(coin_blind))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w");
                    issued.push((n.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(coin_blind)));
                }
                *notes.lock().unwrap() = Some(issued);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "InitializeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.initialize(betting_contract_id, 200, 1)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "StakeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let sk = staker_secret.clone();
                    move || {
                        let r = h.stake(table_id, staker_pub, sk.clone(), amount, pallas::Base::zero(), pallas::Base::zero())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), stake_id]);
                        let child = pn_transfer_child(&n[0], amount, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "UpdateRiskV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    // house_share == payout_amount ⇒ staker_loss = 0, so total_stake is preserved and
                    // house_edge_earnings = (5000 * 200)/10000 = 100 accrues for the claim step.
                    let r = h.update_risk(table_id, betting_contract_id, 5000, 5000, 1000, 0, 200, 1)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ClaimEarningsV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let sk = staker_secret.clone();
                    move || {
                        let info = ClaimStakeInfo {
                            table_id,
                            staker_pub,
                            current_amount: amount,
                            accumulated_earnings: 0,
                            asset_id: pallas::Base::zero(),
                            nonce: 0u64,
                        };
                        let r = h.claim_earnings(stake_id, &info, sk.clone())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(claimable), stake_id]);
                        let child = pn_transfer_payout_child(&n[1], 1000, claimable, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "UnstakeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let sk = staker_secret.clone();
                    move || {
                        let info = UnstakeStakeInfo {
                            table_id,
                            staker_pub,
                            original_amount: amount,
                            current_amount: amount,
                            accumulated_earnings: claimable,
                            asset_id: pallas::Base::zero(),
                            nonce: 0u64,
                        };
                        let r = h.unstake(stake_id, &info, sk.clone(), pallas::Base::zero(), pallas::Base::zero())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(unstake_payout), stake_id]);
                        let child = pn_transfer_payout_child(&n[2], 2000, unstake_payout, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
