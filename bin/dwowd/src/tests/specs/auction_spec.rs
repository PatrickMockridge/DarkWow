//! ContractTestSpec for auction. ClaimWinningsV1, SettleAuctionV1, and RefundBidV1 each
//! require one promissory_note::transfer_v1 (0x04) child call. The child output value_commit
//! is the payout amount (highest_bid / bid.amount) with blind seed:
//!   ClaimWinnings/Settle: poseidon_hash([highest_bid, auction_id])
//!   RefundBid: poseidon_hash([bid.amount, bid_id])

use dwow_contract_test_harness::harness::{AuctionHarness, ContractHarness, PromissoryNoteHarness};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    poseidon_hash, util::fp_mod_fv, pasta_prelude::PrimeField, Blind, MerkleNode, MerkleTree,
    PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointResult, EndpointSpec, EndpointExpectation,
};

fn pn_transfer_child(
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
    blind_seed: pallas::Base,
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
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: blind_seed,
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

pub fn auction_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(AuctionHarness::spawn()));
    let h: &AuctionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/auction/dwow_auction_contract.wasm");

    let seller_sk = pallas::Base::from(10u64);
    let seller_pk = PublicKey::from_secret(SecretKey::from_base(seller_sk));
    let bidder_sk = pallas::Base::from(20u64);
    let bidder_pk = PublicKey::from_secret(SecretKey::from_base(bidder_sk));
    let winner_sk = pallas::Base::from(30u64);
    let winner_pk = PublicKey::from_secret(SecretKey::from_base(winner_sk));

    let asset_id = pallas::Base::from(1u64);
    let reserve_price = 1000u64;
    let deadline = 500u64;
    let issue_secret = pallas::Base::from(100u64);

    // Auction A (claim) uses bid 1500; auction B (settle) bid 1600; auction C (refund) bid 1700.
    let bid_a = 1500u64;
    let bid_b = 1600u64;
    let bid_c = 1700u64;

    let auction_a: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let bid_a_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let auction_b: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let bid_c_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    let notes: Arc<Mutex<Option<Vec<(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base)>>>> =
        Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "auction",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let auction_b = auction_b.clone();
            let bid_c_id = bid_c_id.clone();
            move |chain| {
                let cid = crate::tests::blockchain::derive_contract_id_from_name("auction");
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // Issue 3 notes (bid_a / bid_b / bid_c) for the three payout children.
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, bid_a, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let tid = token0.asset_id;

                let mut issued = Vec::new();
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                issued.push((token0.commitment.inner(), u64::from(mark0), tree.witness(mark0, 0).expect("w0"), tid, pallas::Base::from(6u64)));

                for (idx, val) in [bid_b, bid_c].iter().enumerate() {
                    let n = pn
                        .issue(issue_secret, tid, owner_addr, *val, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64 + idx as u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    issued.push((n.commitment.inner(), u64::from(mark), tree.witness(mark, 0).expect("wi"), tid, pallas::Base::from(8u64 + idx as u64)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Pre-create auction B (settle): Create + PlaceBid(bid_b) + Close.
                let r = h.create_auction(seller_sk, pallas::Base::from(100u64), reserve_price, asset_id, deadline, 0, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &r.call_data, vec![r.proof.clone()])?.submit())?;
                let ab = r.auction_id;
                let pb = h.place_bid(ab, bidder_sk, bid_b, pallas::Base::from(1u64), deadline, 0, 0, bidder_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &pb.call_data, vec![pb.proof.clone()])?.submit())?;
                let cl = h.close_auction(ab, pb.bid_id, seller_sk, deadline, 0, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &cl.call_data, vec![cl.proof.clone()])?.submit())?;
                *auction_b.lock().unwrap() = Some(ab);

                // Pre-create auction C (refund): Create + PlaceBid(bid_c) + PlaceBid(2000, outbids).
                let r = h.create_auction(seller_sk, pallas::Base::from(101u64), reserve_price, asset_id, deadline, 0, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &r.call_data, vec![r.proof.clone()])?.submit())?;
                let ac = r.auction_id;
                let pc1 = h.place_bid(ac, bidder_sk, bid_c, pallas::Base::from(1u64), deadline, 0, 0, bidder_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &pc1.call_data, vec![pc1.proof.clone()])?.submit())?;
                *bid_c_id.lock().unwrap() = Some(pc1.bid_id);
                let pc2 = h.place_bid(ac, winner_sk, 2000, pallas::Base::from(2u64), deadline, 0, bid_c, winner_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &pc2.call_data, vec![pc2.proof.clone()])?.submit())?;
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "CreateAuctionV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let auction_a = auction_a.clone();
                    move || {
                        let r = h.create_auction(seller_sk, pallas::Base::from(102u64), reserve_price, asset_id, deadline, 0, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *auction_a.lock().unwrap() = Some(r.auction_id);
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "PlaceBidV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let auction_a = auction_a.clone();
                    let bid_a_id = bid_a_id.clone();
                    move || {
                        let aid = auction_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("auction A not created".into()))?;
                        let r = h.place_bid(aid, bidder_sk, bid_a, pallas::Base::from(1u64), deadline, 0, 0, bidder_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *bid_a_id.lock().unwrap() = Some(r.bid_id);
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "CloseAuctionV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let auction_a = auction_a.clone();
                    let bid_a_id = bid_a_id.clone();
                    move || {
                        let aid = auction_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("auction A not created".into()))?;
                        let bid = bid_a_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bid A not placed".into()))?;
                        let r = h.close_auction(aid, bid, seller_sk, deadline, 0, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "ClaimWinningsV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let auction_a = auction_a.clone();
                    let bid_a_id = bid_a_id.clone();
                    move || {
                        let aid = auction_a.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("auction A not created".into()))?;
                        let bid = bid_a_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bid A not placed".into()))?;
                        let r = h.claim_winnings(aid, bid, winner_sk, winner_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bid_a), aid]);
                        let child = pn_transfer_child(&n[0], bid_a, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SettleAuctionV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let auction_b = auction_b.clone();
                    move || {
                        let aid = auction_b.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("auction B not pre-created".into()))?;
                        let r = h.settle_auction(aid, seller_sk, bid_b, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bid_b), aid]);
                        let child = pn_transfer_child(&n[1], bid_b, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "RefundBidV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let bid_c_id = bid_c_id.clone();
                    move || {
                        let bid = bid_c_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bid C not placed".into()))?;
                        let r = h.refund_bid(bid, bidder_sk, bidder_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bid_c), bid]);
                        let child = pn_transfer_child(&n[2], bid_c, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
