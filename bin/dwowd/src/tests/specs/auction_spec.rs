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

/// Assemble the PN TransferV1 (0x04) child's call data and **stop, before the proof exists**
/// (`OBL-C212`). `note` is (commitment, leaf pos, merkle path, asset_id, commitment_blind).
///
/// Returns the ordered-set call, the plan to prove once the commitment is known, and the nonce the
/// plan binds (zero — this contract's calls carry no nonce field). The shape is `dex_spec.rs`'s
/// `pn_transfer_prepare`, which is the working copy this one follows: a child that binds its own
/// call commitment is refused the moment its parent's arm derives the frame's, so the child has to
/// wait for the caller to take one commitment over the finished `[child, parent]` set.
fn pn_transfer_prepare(
    note: &(pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base),
    value: u64,
    blind_seed: pallas::Base,
) -> dwow_core::Result<(
    dwow_sdk::tx::ContractCall,
    dwow_promissory_note_contract::client::transfer::TransferCallPlan,
    pallas::Base,
)> {
    let (note_commitment, pos, path, asset_id, commitment_blind) = note;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());
    let nonce = pallas::Base::zero();
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
        tx_nonce: nonce,
    };
    let output = TransferCallOutput {
        recipient: poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(200u64)]),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(200u64))),
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: poseidon_hash([blind_seed, *note_commitment]),
    };
    let pn = PromissoryNoteHarness::spawn();
    let plan = pn
        .transfer_prepare(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    let mut call_data = vec![0x04u8];
    call_data.extend_from_slice(
        &plan.params().encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?,
    );
    let call = dwow_sdk::tx::ContractCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, data: call_data };
    Ok((call, plan, nonce))
}

pub fn auction_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(AuctionHarness::spawn()));
    let h: &AuctionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/auction/dwow_auction_contract.wasm");

    // `OBL-C212`: the harness no longer proves with a zero commitment — every row must frame its own
    // transaction. The commitment is derived over the finished call set *including* the contract id,
    // so the spec needs the id the pipeline will assign, which `deploy` derives purely from the name.
    let cid = crate::tests::blockchain::derive_contract_id_from_name("auction");

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
                        .issue(issue_secret, tid, *val, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64 + idx as u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    issued.push((n.commitment.inner(), u64::from(mark), tree.witness(mark, 0).expect("wi"), tid, pallas::Base::from(8u64 + idx as u64)));
                }
                *notes.lock().unwrap() = Some(issued);

                // Pre-create auction B (settle): Create + PlaceBid(bid_b) + Close.
                // `OBL-C212`: each is its own single-call transaction, so the commitment is taken
                // over that one assembled call and the proof is made against it.
                //
                // The 6th argument is the `block_height` witness. It feeds `auction_id` and
                // nothing else: `create_auction.zk:11` uses it only in the id hash, and the
                // entrypoint records `created_at` from `get_verifying_block_height()`
                // (`entrypoint.rs:396,412`) rather than from this witness. It must therefore be
                // DISTINCT per create — with the seller and `asset_id` shared and the harness's
                // `nonce` fixed at zero, three creates at one height hash to a single
                // `auction_id`, and the second is refused ("Auction already exists",
                // `entrypoint.rs:390-393`). Until 2026-10-13 all three passed 0, which the
                // `tx_binding` failure was masking.
                let (call_data, input) = h.create_auction_prepare(seller_sk, pallas::Base::from(100u64), reserve_price, asset_id, deadline, 1, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                let (proof, pi) = h.create_auction_prove(&input, tx_commitment)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &call_data, vec![proof])?.submit())?;
                let ab = pi.auction_id;
                let (call_data, input) = h.place_bid_prepare(ab, bidder_sk, bid_b, pallas::Base::from(1u64), deadline, 0, 0, bidder_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                let (proof, pi) = h.place_bid_prove(&input, tx_commitment)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &call_data, vec![proof])?.submit())?;
                let (call_data, input) = h.close_auction_prepare(ab, pi.bid_id, seller_sk, deadline, 0, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                let (proof, _pi) = h.close_auction_prove(&input, tx_commitment)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &call_data, vec![proof])?.submit())?;
                *auction_b.lock().unwrap() = Some(ab);

                // Pre-create auction C (refund): Create + PlaceBid(bid_c) + PlaceBid(2000, outbids).
                let (call_data, input) = h.create_auction_prepare(seller_sk, pallas::Base::from(101u64), reserve_price, asset_id, deadline, 2, seller_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                let (proof, pi) = h.create_auction_prove(&input, tx_commitment)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &call_data, vec![proof])?.submit())?;
                let ac = pi.auction_id;
                let (call_data, input) = h.place_bid_prepare(ac, bidder_sk, bid_c, pallas::Base::from(1u64), deadline, 0, 0, bidder_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                let (proof, pi) = h.place_bid_prove(&input, tx_commitment)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &call_data, vec![proof])?.submit())?;
                *bid_c_id.lock().unwrap() = Some(pi.bid_id);
                let (call_data, input) = h.place_bid_prepare(ac, winner_sk, 2000, pallas::Base::from(2u64), deadline, 0, bid_c, winner_pk)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                let (proof, _pi) = h.place_bid_prove(&input, tx_commitment)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &call_data, vec![proof])?.submit())?;
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
                        // Single-call row: the commitment is over this one assembled call (`OBL-C212`).
                        let (call_data, input) = h.create_auction_prepare(seller_sk, pallas::Base::from(102u64), reserve_price, asset_id, deadline, 3, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                        let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                        let (proof, pi) = h.create_auction_prove(&input, tx_commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *auction_a.lock().unwrap() = Some(pi.auction_id);
                        Ok(EndpointResult { children: vec![], call_data, proofs: vec![proof] })
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
                        // Single-call row: the commitment is over this one assembled call (`OBL-C212`).
                        let (call_data, input) = h.place_bid_prepare(aid, bidder_sk, bid_a, pallas::Base::from(1u64), deadline, 0, 0, bidder_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                        let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                        let (proof, pi) = h.place_bid_prove(&input, tx_commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *bid_a_id.lock().unwrap() = Some(pi.bid_id);
                        Ok(EndpointResult { children: vec![], call_data, proofs: vec![proof] })
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
                        // Single-call row: the commitment is over this one assembled call (`OBL-C212`).
                        let (call_data, input) = h.close_auction_prepare(aid, bid, seller_sk, deadline, 0, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let call = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                        let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&call]);
                        let (proof, _pi) = h.close_auction_prove(&input, tx_commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data, proofs: vec![proof] })
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
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bid_a), aid]);
                        // `OBL-C212`: build BOTH calls first, take ONE commitment over `[child, parent]`
                        // (DFS post-order), then prove both against it — the dex_spec framing.
                        let (child, child_plan, child_nonce) = pn_transfer_prepare(&n[0], bid_a, blind_seed)?;
                        let (call_data, input) = h.claim_winnings_prepare(aid, bid, winner_sk, winner_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let parent = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                        let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&child, &parent]);
                        let debris = child_plan.prove(tx_commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let (proof, _pi) = h.claim_winnings_prove(&input, tx_commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child.data, proofs: debris.proofs, children: vec![] }],
                            call_data,
                            proofs: vec![proof],
                        })
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
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bid_b), aid]);
                        // `OBL-C212`: build BOTH calls first, take ONE commitment over `[child, parent]`
                        // (DFS post-order), then prove both against it — the dex_spec framing.
                        let (child, child_plan, child_nonce) = pn_transfer_prepare(&n[1], bid_b, blind_seed)?;
                        let (call_data, input) = h.settle_auction_prepare(aid, seller_sk, bid_b, seller_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let parent = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                        let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&child, &parent]);
                        let debris = child_plan.prove(tx_commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let (proof, _pi) = h.settle_auction_prove(&input, tx_commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child.data, proofs: debris.proofs, children: vec![] }],
                            call_data,
                            proofs: vec![proof],
                        })
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
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bid_c), bid]);
                        // `OBL-C212`: build BOTH calls first, take ONE commitment over `[child, parent]`
                        // (DFS post-order), then prove both against it — the dex_spec framing.
                        let (child, child_plan, child_nonce) = pn_transfer_prepare(&n[2], bid_c, blind_seed)?;
                        let (call_data, input) = h.refund_bid_prepare(bid, bidder_sk, bidder_pk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let parent = dwow_sdk::tx::ContractCall { contract_id: cid, data: call_data.clone() };
                        let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&child, &parent]);
                        let debris = child_plan.prove(tx_commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let (proof, _pi) = h.refund_bid_prove(&input, tx_commitment)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *PROMISSORY_NOTE_CONTRACT_ID, call_data: child.data, proofs: debris.proofs, children: vec![] }],
                            call_data,
                            proofs: vec![proof],
                        })
                    }
                }),
            },
        ],
    }
}
