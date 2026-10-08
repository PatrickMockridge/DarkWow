//! ContractTestSpec for baccarat. Spec: heavyweight-spec.md §5.9.
//!
//! Money flow: CommitBetV1 locks a bet (1:1 PN child); DrawCardsV1 resolves the
//! outcome from block-hash entropy (no child — its `verify_state` reads `Bet.outcome`
//! and stashes the payout); SettleBetV1 pays the outcome-dependent payout
//! (multi-output payout+change child); HouseCloseV1 takes the abandoned bet's value
//! (1:1 child, deterministic). Uses the shared `modules::child_calls` helpers.

use dwow_baccarat_contract::model::{calculate_payout, BetType, Outcome};
use dwow_contract_test_harness::harness::{BaccaratHarness, PromissoryNoteHarness};
use dwow_sdk::crypto::{
    poseidon_hash, pasta_prelude::PrimeField, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::{
    pn_transfer_prepare, pn_transfer_payout_prepare, PnNote,
};
use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};

/// The `ContractCall` a prepared parent frames as. `OBL-C198`'s commitment is taken over the call
/// set the transaction will carry — children first, the parent last, post-order — so the fixture
/// must hand the derivation the same shape the node hashes.
fn parent_call(cid: dwow_sdk::crypto::ContractId, data: &[u8]) -> dwow_sdk::tx::ContractCall {
    dwow_sdk::tx::ContractCall { contract_id: cid, data: data.to_vec() }
}

pub fn baccarat_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BaccaratHarness::spawn()));
    let h: &BaccaratHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/baccarat/dwow_baccarat_contract.wasm");

    let player_pub = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());
    let issue_secret = pallas::Base::from(100u64);
    let bet_value: u64 = 1000;
    let asset_id = pallas::Base::from(1u64);
    let house_secret = pallas::Base::from(10u64);
    let house_pub = PublicKey::from_secret(SecretKey::from_base(house_secret));
    let (house_pub_x, house_pub_y) = house_pub.xy().expect("pk not identity");
    // Deterministic ZK value-blind for commit_bet (avoid OsRng → PI-7 determinism).
    let value_blind = pallas::Scalar::from(42u64);

    // bet A: committed by CommitBetV1, drawn by DrawCardsV1, settled by SettleBetV1.
    // bet B: pre-created in setup, house-closed by HouseCloseV1 (abandoned).
    let secret_nonce_a = pallas::Base::from(99u64);
    let blind_a = pallas::Base::from(3u64);
    let secret_nonce_b = pallas::Base::from(98u64);
    let blind_b = pallas::Base::from(4u64);

    let bet_a: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let bet_b: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let payout_a: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    // Issued PN capabilities, value 1000 each (coin_blinds 6..=9).
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "baccarat",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let bet_b = bet_b.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // note 0 (token type + first commitment), then notes 1..=3.
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, bet_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let asset_id = token0.asset_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero())); // guard leaf @ pos 0
                tree.append(MerkleNode::from_base(token0.commitment.inner())); // note 0 @ pos 1
                let mark0 = tree.mark().unwrap();
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("w0");

                let mut issued = vec![
                    (token0.commitment.inner(), u64::from(mark0), path0, asset_id, pallas::Base::from(6u64)),
                ];
                for coin_blind in [7u64, 8u64, 9u64, 10u64] {
                    let n = pn
                        .issue(issue_secret, asset_id, owner_addr, bet_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(coin_blind))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w");
                    issued.push((n.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(coin_blind)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Pre-create bet B (abandoned) with a 1:1 lock child, for HouseCloseV1.
                // `OBL-C198`: the parent's call data first, the child built around the `bet_id` it
                // derives, then ONE commitment over the ordered set with both proofs bound to it.
                // `with_fee_collect()` here is a **no-op** — it acts only when the transaction
                // carries a FeeV3 call, and this one carries none — so the frame is exactly
                // `[child, parent]`.
                let cid = crate::tests::blockchain::derive_contract_id_from_name("baccarat");
                let plan_b = h.commit_bet_prepare(player_pub, bet_value, BetType::Player, secret_nonce_b, blind_b, asset_id, 200, 1, value_blind)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let bet_id_b = plan_b.bet_id;
                let call_b = plan_b.call_data.clone();
                let n = notes.lock().unwrap();
                let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                let blind_seed_b = poseidon_hash([pallas::Base::from(bet_value), bet_id_b]);
                let (child_call_b, child_plan_b, child_nonce_b) = pn_transfer_prepare(&n[1], bet_value, blind_seed_b, pallas::Base::zero())?;
                let commitment_b = dwow_sdk::crypto::util::tx_commitment([&child_call_b, &parent_call(cid, &call_b)]);
                let (proof_b, _) = plan_b.prove(commitment_b, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let debris_b = child_plan_b.prove(commitment_b, child_nonce_b).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let child_b = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call_b.data.clone(), proofs: debris_b.proofs, children: vec![] };
                smol::block_on(chain.block()?.with_call_tree(
                    cid, &call_b, vec![proof_b],
                    vec![child_b],
                )?.with_fee_collect()?.submit())?;
                *bet_b.lock().unwrap() = Some(bet_id_b);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "CommitBetV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bet_a = bet_a.clone();
                    move || {
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("baccarat");
                        let plan = h.commit_bet_prepare(player_pub, bet_value, BetType::Player, secret_nonce_a, blind_a, asset_id, 200, 1, value_blind)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        let bet_id = plan.bet_id;
                        *bet_a.lock().unwrap() = Some(bet_id);
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), bet_id]);
                        // `OBL-C198`: one commitment over `[child, parent]`, both proofs bound to it.
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(&n[2], bet_value, blind_seed, pallas::Base::zero())?;
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &parent_call(cid, &call_data)]);
                        let (proof, _) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(commitment, child_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data.clone(), proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data, proofs: vec![proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "DrawCardsV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let bet_a = bet_a.clone();
                    let payout_a = payout_a.clone();
                    move |chain| {
                        let id = bet_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet A not committed".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("baccarat");
                        let bytes = chain.query_contract_state(cid, "bets", &id.to_repr())?
                            .ok_or_else(|| dwow_core::Error::Custom("bet A not found in bets tree".into()))?;
                        let bet = dwow_baccarat_contract::model::Bet::decode(&bytes)
                            .map_err(|e| dwow_core::Error::Custom(format!("Bet::decode: {e}")))?;
                        let outcome: Outcome = bet.outcome.ok_or_else(|| dwow_core::Error::Custom("bet A has no outcome".into()))?;
                        let payout = calculate_payout(&bet, outcome);
                        *payout_a.lock().unwrap() = Some(payout);
                        Ok(())
                    }
                })),
                generate: Box::new({
                    let bet_a = bet_a.clone();
                    move || {
                        let id = bet_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet A not committed".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("baccarat");
                        let plan = h.draw_cards_prepare(id, secret_nonce_a, poseidon_hash([pallas::Base::from(7u64), secret_nonce_a]))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        // `OBL-C198`: no child here, so the frame is this call alone.
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&parent_call(cid, &call_data)]);
                        let (proof, _) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data, proofs: vec![proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SettleBetV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bet_a = bet_a.clone();
                    let payout_a = payout_a.clone();
                    move || {
                        let id = bet_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet A not committed".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("baccarat");
                        let payout = payout_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("payout not stashed".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(payout), id]);
                        let plan = h.settle_bet_prepare(id, secret_nonce_a, player_pub, bet_value, BetType::Player, asset_id, blind_a)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        let (child_call, child_plan, child_nonce) = pn_transfer_payout_prepare(&n[3], bet_value, payout, blind_seed)?;
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &parent_call(cid, &call_data)]);
                        let (proof, _) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(commitment, child_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data.clone(), proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data, proofs: vec![proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "HouseCloseV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bet_b = bet_b.clone();
                    move || {
                        let id = bet_b.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet B not pre-created".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("baccarat");
                        let plan = h.house_close_prepare(id, house_secret, house_pub_x, house_pub_y)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), id]);
                        // The leaf blind is derived from the spent note inside
                        // `pn_transfer_prepare`, so this call site no longer chooses it.
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(&n[4], bet_value, blind_seed, pallas::Base::zero())?;
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &parent_call(cid, &call_data)]);
                        let (proof, _) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(commitment, child_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data.clone(), proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data, proofs: vec![proof] })
                    }
                }),
            },
        ],
    }
}
