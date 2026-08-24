//! ContractTestSpec for roulette. Spec: heavyweight-spec.md §5.9.
//!
//! Money flow: InitializeV1 creates the table; PlaceBetV1 locks a bet (1:1 PN child);
//! SpinWheelV1 draws the winning number from block-hash entropy (no child — its
//! `verify_state` reads `RouletteTable.winning_number`); SettleBetsV1 pays the
//! outcome-dependent payout (payout+change child); HouseCloseV1 sweeps the remaining
//! capital (payout+change child). Uses the shared `modules::child_calls` helpers.

use dwow_contract_test_harness::harness::{PromissoryNoteHarness, RouletteHarness};
use dwow_roulette_contract::model::{derive_table_id, RouletteTable};
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

pub fn roulette_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(RouletteHarness::spawn()));
    let h: &RouletteHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/roulette/dwow_roulette_contract.wasm");

    let player_pub = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());
    let house_secret = pallas::Base::from(10u64);
    let house_pub = PublicKey::from_secret(SecretKey::from_base(house_secret));
    let issue_secret = pallas::Base::from(100u64);
    let bet_amount: u64 = 1000;
    let bet_numbers: &'static [u8] = &[0u8];
    let house_capital: u64 = 2_000;

    // Shared state across endpoints (stashed via verify_state).
    let table_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    let winning_number: Arc<Mutex<Option<u8>>> = Arc::new(Mutex::new(None));
    let payout: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
    let remaining_capital: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
    let bet_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

    // Issued PN capabilities: [0] PlaceBet lock (1000), [1] SettleBets payout (1000),
    // [2] HouseClose sweep (100000).
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "roulette",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            let table_id = table_id.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // note 0: PlaceBet lock (value 1000)
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, bet_amount, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
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

                // note 1: SettleBets payout (1000), note 2: HouseClose sweep (1000)
                for (commitment_blind, value) in [(7u64, bet_amount), (8u64, bet_amount)] {
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

                // Initialize the table (table_id is derived from the block height).
                let cid = crate::tests::blockchain::derive_contract_id_from_name("roulette");
                let init = h.initialize(house_pub, false, house_capital, 10000, 2)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                let hh = smol::block_on(chain.block()?.with_call(cid, h, &init.call_data, vec![])?.submit())?;
                *table_id.lock().unwrap() = Some(derive_table_id(&house_pub, hh.get()));
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "PlaceBetV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let table_id = table_id.clone();
                    let bet_id = bet_id.clone();
                    move || {
                        let id = table_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("table not initialized".into()))?;
                        let r = h.place_bet(id, player_pub, 7, bet_numbers.to_vec(), bet_amount, pallas::Base::from(99u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *bet_id.lock().unwrap() = Some(r.bet_id);
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(bet_amount), id]);
                        let child = pn_transfer_child(&n[0], bet_amount, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SpinWheelV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let table_id = table_id.clone();
                    let winning_number = winning_number.clone();
                    move |chain| {
                        let id = table_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("table not initialized".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("roulette");
                        let bytes = chain.query_contract_state(cid, "roulette_tables", &id.to_repr())?
                            .ok_or_else(|| dwow_core::Error::Custom("table not found".into()))?;
                        let table = RouletteTable::decode(&bytes)
                            .map_err(|e| dwow_core::Error::Custom(format!("RouletteTable::decode: {e}")))?;
                        let wn = table.winning_number.ok_or_else(|| dwow_core::Error::Custom("no winning number".into()))?;
                        *winning_number.lock().unwrap() = Some(wn);
                        Ok(())
                    }
                })),
                generate: Box::new({
                    let table_id = table_id.clone();
                    move || {
                        let id = table_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("table not initialized".into()))?;
                        let r = h.spin_wheel(id, house_secret, pallas::Base::from(42u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "SettleBetsV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let table_id = table_id.clone();
                    let remaining_capital = remaining_capital.clone();
                    move |chain| {
                        let id = table_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("table not initialized".into()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("roulette");
                        let bytes = chain.query_contract_state(cid, "roulette_tables", &id.to_repr())?
                            .ok_or_else(|| dwow_core::Error::Custom("table not found".into()))?;
                        let table = RouletteTable::decode(&bytes)
                            .map_err(|e| dwow_core::Error::Custom(format!("RouletteTable::decode: {e}")))?;
                        *remaining_capital.lock().unwrap() = Some(table.house_capital);
                        Ok(())
                    }
                })),
                generate: Box::new({
                    let notes = notes.clone();
                    let table_id = table_id.clone();
                    let bet_id = bet_id.clone();
                    let winning_number = winning_number.clone();
                    let payout = payout.clone();
                    move || {
                        let id = table_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("table not initialized".into()))?;
                        let bid = bet_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("bet not placed".into()))?;
                        let wn = winning_number.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("no winning number".into()))?;
                        let won = bet_numbers.contains(&wn);
                        let p = if won { bet_amount } else { 0 };
                        *payout.lock().unwrap() = Some(p);
                        let r = h.settle_bets(id, vec![bid], p)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(p), id]);
                        let child = pn_transfer_payout_child(&n[1], bet_amount, p, blind_seed)?;
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
                    let table_id = table_id.clone();
                    let remaining_capital = remaining_capital.clone();
                    move || {
                        let id = table_id.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("table not initialized".into()))?;
                        let rc = remaining_capital.lock().unwrap().ok_or_else(|| dwow_core::Error::Custom("no remaining capital".into()))?;
                        let r = h.house_close(id, house_secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(rc), id]);
                        let child = pn_transfer_payout_child(&n[2], bet_amount, rc, blind_seed)?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
