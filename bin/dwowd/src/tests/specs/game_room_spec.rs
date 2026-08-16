//! ContractTestSpec for game_room. Money flow: deposit locks stake (1:1 PN child);
//! create_pot opens a betting pot; place_bet locks a bet (1:1 PN child); settle_pot pays
//! the winner (owner-authorized); claim pays the winner (payout+change child); withdraw
//! returns stake (1:1 PN child). Uses the shared `modules::child_calls` helpers.

use dwow_contract_test_harness::harness::{GameRoomHarness, PromissoryNoteHarness};
use dwow_sdk::crypto::{
    poseidon_hash, pasta_prelude::PrimeField, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::{pn_transfer_child, pn_transfer_payout_child, PnNote};
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};

pub fn game_room_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(GameRoomHarness::spawn()));
    let h: &GameRoomHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/game_room/dwow_game_room_contract.wasm");

    let owner_secret = pallas::Base::from(10u64);
    let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
    let player_secret = pallas::Base::from(1u64);
    let player_pub = PublicKey::from_secret(SecretKey::from_base(player_secret));
    let issue_secret = pallas::Base::from(100u64);
    let token_id = pallas::Base::from(2u64);
    let amount: u64 = 100;

    let room_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let pot_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "game_room",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let room_id = room_id.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // Issue two notes: [0] deposit+place_bet stake (100), [1] claim payout (100).
                let token = pn
                    .register_type(issue_secret, token_id, pallas::Base::from(3u64), owner_addr, amount, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token.call_data, token.token_proofs.clone())?.submit())?;
                let derived_token_id = token.token_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token.commitment.inner()));
                let mark = tree.mark().unwrap();
                let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w0");
                let mut issued = vec![(token.commitment.inner(), u64::from(mark), path, derived_token_id, pallas::Base::from(6u64))];

                for coin_blind in [7u64, 8u64, 9u64] {
                    let n = pn
                        .issue(issue_secret, derived_token_id, owner_addr, amount, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(coin_blind))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let m = tree.mark().unwrap();
                    let p: Vec<MerkleNode> = tree.witness(m, 0).expect("w");
                    issued.push((n.commitment.inner(), u64::from(m), p, derived_token_id, pallas::Base::from(coin_blind)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Create the room (room_id is derived from a fixed block_height + nonce).
                let cid = crate::tests::blockchain::derive_contract_id_from_name("game_room");
                let nonce = pallas::Base::from(1u64);
                let block_height: u64 = 1;
                let init = h.create_room(owner_secret, derived_token_id, block_height, nonce)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let call_data = init.call_data;
                let proof = init.proof;
                smol::block_on(chain.block()?.with_call(cid, h, &call_data, vec![proof])?.submit())?;
                let (ox, oy) = owner_pub.xy().expect("pk not identity");
                let rid = poseidon_hash([pallas::Base::from(4u64), ox, oy, derived_token_id, pallas::Base::from(block_height), nonce]);
                *room_id.lock().unwrap() = Some(rid);
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
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    let notes = notes.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let r = h.deposit(rid, player_secret, amount, pallas::Base::from(2u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), rid]);
                        let child = pn_transfer_child(&n[0], amount, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "CreatePotV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    let pot_id = pot_id.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let r = h.create_pot(rid, player_secret, pallas::Base::from(3u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *pot_id.lock().unwrap() = Some(r.public_inputs.pot_id);
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "PlaceBetV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    let pot_id = pot_id.clone();
                    let notes = notes.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let pid = pot_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("pot not created".into()))?;
                        let r = h.place_bet(rid, pid, player_secret, amount, dwow_game_room_contract::model::BetType::Bet, 2, pallas::Base::from(4u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), rid]);
                        let child = pn_transfer_child(&n[1], amount, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(7u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "FoldV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let r = h.fold(rid, player_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "ClosePotV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    let pot_id = pot_id.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let pid = pot_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("pot not created".into()))?;
                        let r = h.close_pot(rid, pid, player_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SettlePotV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    let pot_id = pot_id.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let pid = pot_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("pot not created".into()))?;
                        let r = h.settle_pot(owner_secret, rid, pid, vec![(player_pub, amount)], amount, pallas::Base::from(5u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
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
                    let room_id = room_id.clone();
                    let pot_id = pot_id.clone();
                    let notes = notes.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let pid = pot_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("pot not created".into()))?;
                        let r = h.claim(rid, pid, player_secret, amount, pallas::Base::from(6u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), rid]);
                        let child = pn_transfer_payout_child(&n[2], amount, amount, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "WithdrawV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    let notes = notes.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let r = h.withdraw(rid, player_secret, amount).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), rid]);
                        let child = pn_transfer_child(&n[3], amount, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(8u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RaiseV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let r = h.raise(rid, player_secret, amount, pallas::Base::from(7u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "CallV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let r = h.call(rid, player_secret, pallas::Base::from(8u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "ContributeEntropyV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let room_id = room_id.clone();
                    move || {
                        let rid = room_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("room not created".into()))?;
                        let r = h.contribute_entropy(rid, player_secret, pallas::Base::from(9u64), None)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
