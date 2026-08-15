//! ContractTestSpec for slot. Spec: heavyweight-spec.md §5.9.
//!
//! Money flow: InitializeV1 sets up config; CommitSpinV1 locks a bet (1:1 PN child);
//! RevealSpinV1 draws reel positions from block-hash entropy (no child — its
//! `verify_state` reads `Spin.result` and computes the payout); SettleSpinV1 pays the
//! outcome-dependent payout (payout+change child); CancelSpinV1 sweeps the house take
//! (payout+change child). Uses the shared `modules::child_calls` helpers.

use dwow_contract_test_harness::harness::{PromissoryNoteHarness, SlotHarness};
use dwow_slot_contract::model::{
    calculate_house_take, calculate_payout, calculate_wins, video_paytable, Payline, Spin,
    SpinResult,
};
use dwow_sdk::crypto::{
    poseidon_hash, pasta_prelude::PrimeField, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::{
    pn_transfer_child, pn_transfer_payout_child, PnNote,
};
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};

pub fn slot_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(SlotHarness::spawn()));
    let h: &SlotHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/slot/dwow_slot_contract.wasm");

    let player_pub = PublicKey::from_secret(SecretKey::from_bytes([3u8; 32]).unwrap());
    let issue_secret = pallas::Base::from(100u64);
    let bet_value: u64 = 1000;
    let paylines_played: u32 = 1;
    let house_edge: u32 = 500;
    let confirmation_depth: u8 = 1;
    let token_id = pallas::Base::from(1u64);
    let value_blind = pallas::Scalar::from(42u64);

    let secret_nonce_a = pallas::Base::from(99u64);
    let blind_a = pallas::Base::from(3u64);
    let secret_nonce_b = pallas::Base::from(98u64);
    let blind_b = pallas::Base::from(4u64);

    // Config (must match the initialize's video-slot config) for payout calc.
    let reels = Arc::new(video_paytable::default_reels());
    let payline = Arc::new(Payline::horizontal_top(5));
    let paytable = Arc::new(video_paytable::create());

    let spin_a: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let spin_b: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let positions: Arc<Mutex<Option<Vec<u64>>>> = Arc::new(Mutex::new(None));
    let payout_a: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    // Issued PN capabilities: [1] commit A, [2] pre-create B, [3] settle A (large),
    // [4] cancel B.
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "slot",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let spin_b = spin_b.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, bet_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let token_id = token0.token_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("w0");
                let mut issued = vec![
                    (token0.commitment.inner(), u64::from(mark0), path0, token_id, pallas::Base::from(6u64)),
                ];

                // notes 1..=4: commit A (1000), pre-create B (1000), settle A (100000), cancel B (1000)
                for (coin_blind, value) in [(7u64, bet_value), (8u64, bet_value), (9u64, 100_000u64), (10u64, bet_value)] {
                    let n = pn
                        .issue(issue_secret, token_id, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(coin_blind))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w");
                    issued.push((n.commitment.inner(), u64::from(mark), path, token_id, pallas::Base::from(coin_blind)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Initialize config.
                let cid = crate::tests::blockchain::derive_contract_id_from_name("slot");
                let init = h.initialize().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &init.call_data, vec![])?.submit())?;

                // Pre-create spin B (abandoned) for CancelSpinV1.
                let r_b = h.commit_spin(player_pub, bet_value, paylines_played, secret_nonce_b, blind_b, house_edge, confirmation_depth, token_id, value_blind)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let n = notes.lock().unwrap();
                let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                let blind_seed_b = poseidon_hash([pallas::Base::from(bet_value), r_b.public_inputs.spin_id]);
                let child_b = pn_transfer_child(&n[2], bet_value, blind_seed_b, blind_seed_b, pallas::Base::zero())?;
                smol::block_on(chain.block()?.with_call_tree(
                    cid, &r_b.call_data, vec![r_b.proof.clone()],
                    vec![(child_b.contract_id, child_b.call_data, child_b.proofs)],
                )?.with_fee_collect()?.submit())?;
                *spin_b.lock().unwrap() = Some(r_b.public_inputs.spin_id);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "commit_spin",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let spin_a = spin_a.clone();
                    move || {
                        let r = h.commit_spin(player_pub, bet_value, paylines_played, secret_nonce_a, blind_a, house_edge, confirmation_depth, token_id, value_blind)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *spin_a.lock().unwrap() = Some(r.public_inputs.spin_id);
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), r.public_inputs.spin_id]);
                        let child = pn_transfer_child(&n[1], bet_value, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "reveal_spin",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let spin_a = spin_a.clone();
                    let positions = positions.clone();
                    let payout_a = payout_a.clone();
                    let reels = reels.clone();
                    let payline = payline.clone();
                    let paytable = paytable.clone();
                    move |chain| {
                        let id = spin_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("spin A not committed".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("slot");
                        let bytes = chain.query_contract_state(cid, "spins", &id.to_repr())?
                            .ok_or_else(|| dwow_core::Error::Custom("spin A not found".into()))?;
                        let spin = Spin::decode(&bytes).map_err(|e| dwow_core::Error::Custom(format!("Spin::decode: {e}")))?;
                        let result: SpinResult = spin.result.ok_or_else(|| dwow_core::Error::Custom("spin A has no result".into()))?;
                        let wins = calculate_wins(&result, &reels, std::slice::from_ref(&payline), &paytable);
                        let payout = calculate_payout(bet_value, &wins, house_edge);
                        *positions.lock().unwrap() = Some(result.positions);
                        *payout_a.lock().unwrap() = Some(payout);
                        Ok(())
                    }
                })),
                generate: Box::new({
                    let spin_a = spin_a.clone();
                    move || {
                        let id = spin_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("spin A not committed".into()))?;
                        let r = h.reveal_spin(id, secret_nonce_a)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "settle_spin",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let spin_a = spin_a.clone();
                    let positions = positions.clone();
                    let payout_a = payout_a.clone();
                    let reels = reels.clone();
                    let payline = payline.clone();
                    let paytable = paytable.clone();
                    move || {
                        let id = spin_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("spin A not committed".into()))?;
                        let pos = positions.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("no positions".into()))?;
                        let p = payout_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("no payout".into()))?;
                        let p3 = [pos.get(0).copied().unwrap_or(0), pos.get(1).copied().unwrap_or(0), pos.get(2).copied().unwrap_or(0)];
                        let result = SpinResult::new(pos.clone());
                        let wins = calculate_wins(&result, &reels, std::slice::from_ref(&payline), &paytable);
                        let r = h.settle_bet(player_pub, bet_value, paylines_played, secret_nonce_a, blind_a, token_id, p3, wins.len() as u64, p)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(p), id]);
                        let child = pn_transfer_payout_child(&n[3], 100_000, p, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "cancel_spin",
                is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let spin_b = spin_b.clone();
                    move || {
                        let id = spin_b.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("spin B not pre-created".into()))?;
                        let r = h.cancel_spin(id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let house_take = calculate_house_take(bet_value, house_edge);
                        let blind_seed = poseidon_hash([pallas::Base::from(house_take), id]);
                        let child = pn_transfer_payout_child(&n[4], bet_value, house_take, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
        ],
    }
}
