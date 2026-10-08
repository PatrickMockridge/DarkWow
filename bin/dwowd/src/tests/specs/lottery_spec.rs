//! ContractTestSpec for lottery. Spec: heavyweight-spec.md §5.10.
//!
//! Money flow: InitializeV1 (non-ZK, in setup) creates the lottery; BuyTicketV1 locks the ticket
//! price (1:1 PN child); DrawWinnersV1 draws winning numbers from block-hash entropy (house-auth,
//! no child); RevealTicketV1 reveals the committed numbers (tx_binding-only, no child);
//! ClaimPrizeV1 pays the deterministic prize (payout child); ExpireLotteryV1 sweeps the house claim
//! (house-auth, payout+change child). Uses the shared `modules::child_calls` helpers.

use dwow_contract_test_harness::harness::{PromissoryNoteHarness, LotteryHarness};
use dwow_lottery_contract::model::derive_lottery_id;
use dwow_sdk::crypto::{
    poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey,
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
/// set the transaction will carry — children first, the parent last, post-order.
fn parent_call(cid: dwow_sdk::crypto::ContractId, data: &[u8]) -> dwow_sdk::tx::ContractCall {
    dwow_sdk::tx::ContractCall { contract_id: cid, data: data.to_vec() }
}

pub fn lottery_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(LotteryHarness::spawn()));
    let h: &LotteryHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/lottery/dwow_lottery_contract.wasm");

    // Deterministic, low-entropy secrets. The claim proof binds ticket_pub = ticket_secret * G,
    // so the player secret must be a Base (not arbitrary bytes) for the claim's ticket_secret to
    // derive the same public key as BuyTicket's player_pub.
    let player_secret = pallas::Base::from(1u64);
    let player_pub = PublicKey::from_secret(SecretKey::from_base(player_secret));
    let house_secret = pallas::Base::from(10u64);
    let house_pub = PublicKey::from_secret(SecretKey::from_base(house_secret));
    let issue_secret = pallas::Base::from(100u64);

    let ticket_price: u64 = 100;
    let numbers: &'static [u8] = &[1u8, 2, 3]; // sorted (matches the harness commitment hash)
    let nonce = pallas::Base::from(42u64); // secret nonce for commitment + ticket_id
    let draw_nonce = pallas::Base::from(99u64); // entropy seed nonce

    // Deterministic payouts (single ticket, zero house edge, 100% payout):
    // prize = prize_pool = ticket_price; house_claim = prize_pool - prize_pool/2 = ticket_price/2.
    let prize: u64 = ticket_price;
    let house_claim: u64 = ticket_price / 2;

    // Shared state across endpoints (lottery_id stashed in setup, ticket_id in BuyTicket).
    let lottery_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let ticket_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    // Issued PN capabilities: [0] BuyTicket lock (ticket_price), [1] ClaimPrize payout (ticket_price),
    // [2] ExpireLottery house claim (ticket_price).
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "lottery",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let lottery_id = lottery_id.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // note 0: BuyTicket lock (value ticket_price)
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, ticket_price, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
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

                // note 1: ClaimPrize payout, note 2: ExpireLottery house claim
                for (commitment_blind, value) in [(7u64, ticket_price), (8u64, ticket_price)] {
                    let n = pn
                        .issue(issue_secret, asset_id, owner_addr, value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(commitment_blind))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w");
                    issued.push((n.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(commitment_blind)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Initialize the lottery (non-ZK; lottery_id is derived from the block height).
                let cid = crate::tests::blockchain::derive_contract_id_from_name("lottery");
                let init = h.initialize(house_pub, ticket_price, 3, 10, 2, 2)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let hh = smol::block_on(chain.block()?.with_call(cid, h, &init.call_data, vec![])?.submit())?;
                *lottery_id.lock().unwrap() = Some(derive_lottery_id(&house_pub, hh.get()));
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "BuyTicketV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let lottery_id = lottery_id.clone();
                    let ticket_id = ticket_id.clone();
                    move || {
                        let id = lottery_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("lottery not initialized".into()))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let asset_id = n[0].3;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("lottery");
                        let plan = h.commit_ticket_prepare(player_pub, id, numbers.to_vec(), nonce, ticket_price, asset_id)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        let blind_seed = poseidon_hash([pallas::Base::from(ticket_price), id]);
                        // `OBL-C198`: one commitment over `[child, parent]`, both proofs bound to it.
                        let (child_call, child_plan, child_nonce) = pn_transfer_prepare(&n[0], ticket_price, blind_seed, pallas::Base::zero())?;
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &parent_call(cid, &call_data)]);
                        let (proof, public_inputs) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *ticket_id.lock().unwrap() = Some(public_inputs.ticket_id);
                        let debris = child_plan.prove(commitment, child_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data.clone(), proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data, proofs: vec![proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "DrawWinnersV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let lottery_id = lottery_id.clone();
                    move || {
                        let id = lottery_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("lottery not initialized".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("lottery");
                        let plan = h.draw_winners_prepare(id, house_secret, draw_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        // `OBL-C198`: no child, so the frame is this call alone.
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&parent_call(cid, &call_data)]);
                        let (proof, _) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data, proofs: vec![proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RevealTicketV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let ticket_id = ticket_id.clone();
                    move || {
                        let tid = ticket_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("ticket not committed".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("lottery");
                        let plan = h.reveal_ticket_prepare(tid, numbers.to_vec(), nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&parent_call(cid, &call_data)]);
                        let (proof, _) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data, proofs: vec![proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "ClaimPrizeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let lottery_id = lottery_id.clone();
                    let ticket_id = ticket_id.clone();
                    move || {
                        let id = lottery_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("lottery not initialized".into()))?;
                        let tid = ticket_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("ticket not committed".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("lottery");
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(prize), id]);
                        let plan = h.claim_prize_prepare(tid, player_secret, 0, 1)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        let (child_call, child_plan, child_nonce) = pn_transfer_payout_prepare(&n[1], ticket_price, prize, blind_seed)?;
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &parent_call(cid, &call_data)]);
                        let (proof, _) = plan.prove(commitment, pallas::Base::zero()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let debris = child_plan.prove(commitment, child_nonce).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let child = ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child_call.data.clone(), proofs: debris.proofs, children: vec![] };
                        Ok(EndpointResult { children: vec![child], call_data, proofs: vec![proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "ExpireLotteryV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let lottery_id = lottery_id.clone();
                    move || {
                        let id = lottery_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("lottery not initialized".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("lottery");
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(house_claim), id]);
                        let plan = h.expire_lottery_prepare(id, house_secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call_data = plan.call_data.clone();
                        let (child_call, child_plan, child_nonce) = pn_transfer_payout_prepare(&n[2], ticket_price, house_claim, blind_seed)?;
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
