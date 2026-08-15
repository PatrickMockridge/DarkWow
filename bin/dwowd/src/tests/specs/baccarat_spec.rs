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
    pn_transfer_child, pn_transfer_payout_child, PnNote,
};
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};

pub fn baccarat_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BaccaratHarness::spawn()));
    let h: &BaccaratHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/baccarat/dwow_baccarat_contract.wasm");

    let player_pub = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());
    let issue_secret = pallas::Base::from(100u64);
    let bet_value: u64 = 1000;
    let token_id = pallas::Base::from(1u64);
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

                // note 0 (token type + first coin), then notes 1..=3.
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, bet_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let token_id = token0.token_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero())); // guard leaf @ pos 0
                tree.append(MerkleNode::from_base(token0.commitment.inner())); // note 0 @ pos 1
                let mark0 = tree.mark().unwrap();
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("w0");

                let mut issued = vec![
                    (token0.commitment.inner(), u64::from(mark0), path0, token_id, pallas::Base::from(6u64)),
                ];
                for coin_blind in [7u64, 8u64, 9u64, 10u64] {
                    let n = pn
                        .issue(issue_secret, token_id, owner_addr, bet_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(coin_blind))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w");
                    issued.push((n.commitment.inner(), u64::from(mark), path, token_id, pallas::Base::from(coin_blind)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Pre-create bet B (abandoned) with a 1:1 lock child, for HouseCloseV1.
                let cid = crate::tests::blockchain::derive_contract_id_from_name("baccarat");
                let r_b = h.commit_bet(player_pub, bet_value, BetType::Player, secret_nonce_b, blind_b, token_id, 200, 1, value_blind)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let n = notes.lock().unwrap();
                let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                let blind_seed_b = poseidon_hash([pallas::Base::from(bet_value), r_b.bet_id]);
                let child_b = pn_transfer_child(&n[1], bet_value, blind_seed_b, blind_seed_b, pallas::Base::zero())?;
                smol::block_on(chain.block()?.with_call_tree(
                    cid, &r_b.call_data, vec![r_b.proof.clone()],
                    vec![(child_b.contract_id, child_b.call_data, child_b.proofs)],
                )?.with_fee_collect()?.submit())?;
                *bet_b.lock().unwrap() = Some(r_b.bet_id);
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
                        let r = h.commit_bet(player_pub, bet_value, BetType::Player, secret_nonce_a, blind_a, token_id, 200, 1, value_blind)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *bet_a.lock().unwrap() = Some(r.bet_id);
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), r.bet_id]);
                        let child = pn_transfer_child(&n[2], bet_value, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
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
                        let r = h.draw_cards(id, secret_nonce_a, poseidon_hash([pallas::Base::from(7u64), secret_nonce_a]), pallas::Base::zero(), pallas::Base::zero())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
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
                        let r = h.settle_bet(id, secret_nonce_a, player_pub, bet_value, BetType::Player, token_id, blind_a)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let payout = payout_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("payout not stashed".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(payout), id]);
                        let child = pn_transfer_payout_child(&n[3], bet_value, payout, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
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
                        let r = h.house_close(id, house_secret, house_pub_x, house_pub_y, pallas::Base::zero(), pallas::Base::zero())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), id]);
                        // Distinct output coin_blind: the setup pre-create already spent a
                        // note with coin_blind == blind_seed for this bet, so reuse would
                        // collide (PN DuplicateCoin).
                        let child = pn_transfer_child(&n[4], bet_value, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(7u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
