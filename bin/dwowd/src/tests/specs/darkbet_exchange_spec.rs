//! ContractTestSpec for darkbet_exchange.
//!
//! Money flow: CreateMarketV1 opens a market (order-book or AMM). Order-book mode:
//! PlaceBackV1 + PlaceLayV1 lock a stake (PN child), MatchOrdersV1 pairs them,
//! ResolveMarketV1 sets the winner, SettleMarketV1 pays it out (PN child). AMM mode:
//! BuyPositionV1 wagers (PN child), AddLiquidityV1 + RemoveLiquidityV1 move liquidity
//! (PN child), ClaimWinningsV1 pays a resolved winning position (PN child).
//! CancelOrderV1 is exercised as a rejection (matched order is no longer cancellable).

use dwow_contract_test_harness::harness::{DarkbetExchangeHarness, PromissoryNoteHarness};
use dwow_sdk::crypto::{
    poseidon_hash, pasta_prelude::PrimeField, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::{pn_transfer_child, PnNote};
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};

pub fn darkbet_exchange_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DarkbetExchangeHarness::spawn()));
    let h: &DarkbetExchangeHarness = harness;
    let wasm =
        include_bytes!("../../../../../src/contract/darkbet_exchange/dwow_darkbet_exchange_contract.wasm");

    // Secrets (deterministic Base values).
    let issue_secret = pallas::Base::from(100u64);
    let creator_secret = pallas::Base::from(11u64);
    let oracle_secret = pallas::Base::from(12u64);
    let backer_secret = pallas::Base::from(13u64);
    let layer_secret = pallas::Base::from(14u64);
    let matcher_secret = pallas::Base::from(15u64);
    let owner_secret = pallas::Base::from(16u64);
    let provider_secret = pallas::Base::from(17u64);

    let creator_pub = PublicKey::from_secret(SecretKey::from_base(creator_secret));
    let oracle_pub = PublicKey::from_secret(SecretKey::from_base(oracle_secret));
    let oracle_id = oracle_pub.x().expect("pk not identity");
    let (ccx, ccy) = creator_pub.xy().expect("pk not identity");
    let (bx, by) = PublicKey::from_secret(SecretKey::from_base(backer_secret))
        .xy()
        .expect("pk");
    let (lx, ly) = PublicKey::from_secret(SecretKey::from_base(layer_secret))
        .xy()
        .expect("pk");
    let (ox, oy) = PublicKey::from_secret(SecretKey::from_base(owner_secret))
        .xy()
        .expect("pk");
    let (px, py) = PublicKey::from_secret(SecretKey::from_base(provider_secret))
        .xy()
        .expect("pk");

    let duration = 1000u64;

    // Three markets: one order-book, two AMM (keeps the AMM payout math independent).
    let ob_nonce = 1u64;
    let amm1_nonce = 2u64;
    let amm2_nonce = 3u64;
    let market_ob = poseidon_hash([
        pallas::Base::from(4u64),
        ccx, ccy,
        pallas::Base::from(ob_nonce + duration),
        pallas::Base::from(ob_nonce),
    ]);
    let market_amm1 = poseidon_hash([
        pallas::Base::from(4u64),
        ccx, ccy,
        pallas::Base::from(amm1_nonce + duration),
        pallas::Base::from(amm1_nonce),
    ]);
    let market_amm2 = poseidon_hash([
        pallas::Base::from(4u64),
        ccx, ccy,
        pallas::Base::from(amm2_nonce + duration),
        pallas::Base::from(amm2_nonce),
    ]);

    // Order-book ids (deterministic; order_id not circuit-constrained but used by
    // the value_blind and later match/cancel references).
    let stake = 1000u64;
    let odds = 20000u32;
    let back_order_id = poseidon_hash([
        market_ob,
        pallas::Base::from(0u64),
        pallas::Base::from(odds as u64),
        pallas::Base::from(stake),
        bx, by,
    ]);
    let lay_order_id = poseidon_hash([
        market_ob,
        pallas::Base::from(0u64),
        pallas::Base::from(odds as u64),
        pallas::Base::from(stake),
        lx, ly,
        pallas::Base::one(),
    ]);
    let match_id = poseidon_hash([market_ob, back_order_id, lay_order_id, pallas::Base::from(odds as u64)]);

    // AMM ids.
    let amount = 100u64;
    let lp_amount = 500u64;
    let bp_nonce = 4u64;
    let al_nonce = 5u64;
    let position_id = poseidon_hash([
        pallas::Base::from(4u64),
        market_amm1,
        ox, oy,
        pallas::Base::from(0u64),
        pallas::Base::from(amount),
        pallas::Base::from(bp_nonce),
    ]);
    let lp_share_id = poseidon_hash([
        pallas::Base::from(4u64),
        market_amm2,
        px, py,
        pallas::Base::from(lp_amount),
        pallas::Base::from(al_nonce),
    ]);

    // Deterministic payouts.
    let claim_payout: u64 = 100; // fresh AMM position potential_payout == amount
    let remove_payout: u64 = 500; // 500 shares / 500 pool * 500 total_pool
    let settle_payout: u64 = 2000; // back_stake * odds / 10000 = 1000*20000/10000

    // Issued PN capabilities (note 0 = register_type, notes 1..8 = issue notes).
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "darkbet_exchange",
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

                let token0 = pn
                    .register_type(
                        issue_secret,
                        pallas::Base::from(2u64),
                        pallas::Base::from(3u64),
                        owner_addr,
                        100_000,
                        pallas::Base::zero(),
                        pallas::Base::zero(),
                        pallas::Base::from(6u64),
                    )
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit(),
                )?;
                let token_id = token0.token_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("w0");
                let mut issued = vec![(
                    token0.commitment.inner(),
                    u64::from(mark0),
                    path0,
                    token_id,
                    pallas::Base::from(6u64),
                )];

                let note_vals: [(u64, u64); 8] =
                    [(7, 1000), (8, 1000), (9, 100), (10, 500), (11, 500), (12, 100), (13, 2000), (14, 1000)];
                for (coin_blind, value) in note_vals {
                    let n = pn
                        .issue(
                            issue_secret,
                            token_id,
                            owner_addr,
                            value,
                            pallas::Base::zero(),
                            pallas::Base::zero(),
                            pallas::Base::from(coin_blind),
                        )
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(
                        chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit(),
                    )?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("w");
                    issued.push((
                        n.commitment.inner(),
                        u64::from(mark),
                        path,
                        token_id,
                        pallas::Base::from(coin_blind),
                    ));
                }
                *notes.lock().unwrap() = Some(issued);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            // 0x00 CreateMarketV1 (order-book)
            EndpointSpec {
                name: "CreateMarketV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h
                        .create_market(creator_secret, oracle_id, ob_nonce, duration, 0)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // 0x00 CreateMarketV1 (AMM — buy/claim)
            EndpointSpec {
                name: "CreateMarketV1Amm1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h
                        .create_market(creator_secret, oracle_id, amm1_nonce, duration, 1)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // 0x00 CreateMarketV1 (AMM — liquidity)
            EndpointSpec {
                name: "CreateMarketV1Amm2",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h
                        .create_market(creator_secret, oracle_id, amm2_nonce, duration, 1)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // 0x01 PlaceBackV1
            EndpointSpec {
                name: "PlaceBackV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .place_back(market_ob, backer_secret, 0, odds, stake)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(stake), back_order_id]);
                        let child = pn_transfer_child(&n[1], stake, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(1u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // 0x02 PlaceLayV1
            EndpointSpec {
                name: "PlaceLayV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .place_lay(market_ob, layer_secret, 0, odds, stake)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(stake), lay_order_id]);
                        let child = pn_transfer_child(&n[2], stake, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(2u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // 0x07 BuyPositionV1
            EndpointSpec {
                name: "BuyPositionV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .buy_position(market_amm1, owner_secret, 0, amount, bp_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), position_id]);
                        let child = pn_transfer_child(&n[3], amount, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(3u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // 0x08 AddLiquidityV1
            EndpointSpec {
                name: "AddLiquidityV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .add_liquidity(market_amm2, provider_secret, lp_amount, al_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(lp_amount), lp_share_id]);
                        let child = pn_transfer_child(&n[4], lp_amount, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(4u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // 0x03 MatchOrdersV1
            EndpointSpec {
                name: "MatchOrdersV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h
                        .match_orders(market_ob, matcher_secret, back_order_id, lay_order_id, odds)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // 0x04 ResolveMarketV1 (order-book)
            EndpointSpec {
                name: "ResolveMarketV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h
                        .resolve_market(market_ob, oracle_secret, 0)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // 0x04 ResolveMarketV1 (AMM-1)
            EndpointSpec {
                name: "ResolveMarketV1Amm1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h
                        .resolve_market(market_amm1, oracle_secret, 0)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // 0x05 SettleMarketV1 (non-ZK)
            EndpointSpec {
                name: "SettleMarketV1",
                is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .settle_market(market_ob, vec![match_id])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(settle_payout), market_ob]);
                        let child = pn_transfer_child(&n[7], settle_payout, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(7u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            // 0x0A ClaimWinningsV1
            EndpointSpec {
                name: "ClaimWinningsV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .claim_winnings(market_amm1, position_id, owner_secret, 0, amount)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(claim_payout), position_id]);
                        let child = pn_transfer_child(&n[6], claim_payout, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(6u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // 0x09 RemoveLiquidityV1
            EndpointSpec {
                name: "RemoveLiquidityV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .remove_liquidity(market_amm2, lp_share_id, provider_secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(remove_payout), lp_share_id]);
                        let child = pn_transfer_child(&n[5], remove_payout, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(5u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // 0x06 CancelOrderV1 — matched back order is no longer cancellable (Rejection).
            EndpointSpec {
                name: "CancelOrderV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let r = h
                            .cancel_order(back_order_id, backer_secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(stake), back_order_id]);
                        let child = pn_transfer_child(&n[8], stake, blind_seed, poseidon_hash([blind_seed, pallas::Base::from(8u64)]), pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
