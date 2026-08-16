//! ContractTestSpec for darktoshi_dice. Spec: heavyweight-spec.md §5.9.
//!
//! Money flow: CommitBetV1 locks the bet (1:1 PN child); RevealRollV1 reveals the secret nonce and
//! derives the roll from block-hash entropy (no child); SettleBetV1 pays out (payout child — 0 on a
//! house win); HouseCloseV1 collects an abandoned bet (child). Uses the shared `modules::child_calls`
//! helpers.
//!
//! NOTE: the dice state machine makes SettleBetV1 and HouseCloseV1 mutually exclusive on the same
//! bet (both consume the `Revealed` state). A single linear run can only green one of them. We green
//! CommitBet → RevealRoll → SettleBet (the main path), and assert HouseCloseV1 is REJECTED (the bet
//! is already `SettledHouse`). `target = 1` maximizes the house-win probability (the settle path
//! requires `roll >= target`, i.e. a player loss).

use dwow_contract_test_harness::harness::{DarkToshiDiceHarness, PromissoryNoteHarness};
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

pub fn darktoshi_dice_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DarkToshiDiceHarness::spawn()));
    let h: &DarkToshiDiceHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/darktoshi_dice/dwow_darktoshi_dice_contract.wasm");

    let player_secret = pallas::Base::from(1u64);
    let player_pub = PublicKey::from_secret(SecretKey::from_base(player_secret));
    let house_secret = pallas::Base::from(10u64);
    let issue_secret = pallas::Base::from(100u64);

    let bet_value: u64 = 1000;
    let target: u8 = 1; // minimize player-win probability (settle requires a loss)
    let secret_nonce = pallas::Base::from(99u64);
    let blind = pallas::Base::from(3u64);

    // bet_id stashed from the CommitBet result.
    let bet_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "darktoshi_dice",
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
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // note 0: CommitBetV1 lock (1000)
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

                // note 1: SettleBetV1 payout (1000 locked), note 2: HouseCloseV1 (1000 locked)
                for (coin_blind, value) in [(7u64, bet_value), (8u64, bet_value)] {
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
                    let bet_id = bet_id.clone();
                    move || {
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let token_id = n[0].3;
                        let r = h.commit_bet(player_pub, bet_value, target, secret_nonce, blind, token_id, 200u32)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *bet_id.lock().unwrap() = Some(r.public_inputs.bet_id);
                        let id = r.public_inputs.bet_id;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), id]);
                        let child = pn_transfer_child(&n[0], bet_value, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RevealRollV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let bet_id = bet_id.clone();
                    move || {
                        let id = bet_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet not committed".into()))?;
                        let r = h.reveal_roll(id, secret_nonce)
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
                    let bet_id = bet_id.clone();
                    move || {
                        let id = bet_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet not committed".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let token_id = n[0].3;
                        let (px, py) = player_pub.xy().expect("pk not identity");
                        let block_hash = pallas::Base::from(42u64); // free witness (roll_hash is not cross-checked)
                        let r = h.settle_bet(id, px, py, pallas::Base::from(bet_value), pallas::Base::from(target as u64), secret_nonce, blind, token_id, block_hash)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        // payout = 0 on a house win (target=1 ⇒ roll >= 1)
                        let blind_seed = poseidon_hash([pallas::Base::from(0u64), id]);
                        let child = pn_transfer_payout_child(&n[1], bet_value, 0, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "HouseCloseV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bet_id = bet_id.clone();
                    move || {
                        let id = bet_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet not committed".into()))?;
                        let r = h.house_close(id, house_secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_value), id]);
                        let child = pn_transfer_child(&n[2], bet_value, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
