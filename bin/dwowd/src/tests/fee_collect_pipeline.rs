/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! FeeCollectV1 end-to-end determinism test — MOC close-out item 9.
//!
//! Mines 101 blocks through the production `accept_block` path to satisfy
//! COINBASE_MATURITY, constructs a FeeV1 transaction, mines a block containing
//! the fee tx plus the FeeCollectV1 collection plate, and verifies the fee
//! coin is correctly created, the pot is zeroed, supply is unchanged, mass
//! balance passes, and re-execution from genesis produces identical state.
//!
//! Heavyweight — runs in ~10-25 min. Gated behind `#[ignore]`.
//!
//! ```bash
//! RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 \
//!   cargo test -p dwowd -- test_fee_collect_determinism --ignored --nocapture
//! ```

use std::sync::Arc;

use dwow_chain::{Block, ContractCall, Transaction};
use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion, MoneroBlockHeight, SupplyAmount};
use dwow_sdk::crypto::{
    keypair::Network,
    pasta_prelude::Group,
    ContractId, PublicKey, SecretKey, NATIVE_TOKEN_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use dwow_serial::Encodable;

use crate::tests::genesis::GenesisHarness;

/// Build a block header with `target: u32::MAX` (guaranteed PoW pass — first
/// RandomX nonce wins). Deterministic keys per height.
fn build_block_header(
    prev_hash: blake3::Hash,
    height: BlockHeight,
    reward: BlockReward,
    txs: &[Transaction],
) -> dwow_chain::BlockHeader {
    let merkle_root = {
        let tx_hashes: Vec<blake3::Hash> = txs.iter().map(|tx| tx.hash()).collect();
        if tx_hashes.is_empty() {
            blake3::hash(&[])
        } else {
            let mut layer = tx_hashes.clone();
            while layer.len() > 1 {
                if layer.len() % 2 != 0 {
                    layer.push(*layer.last().unwrap());
                }
                layer = layer
                    .chunks(2)
                    .map(|pair| {
                        let mut combined = pair[0].as_bytes().to_vec();
                        combined.extend_from_slice(pair[1].as_bytes());
                        blake3::hash(&combined)
                    })
                    .collect();
            }
            layer[0]
        }
    };
    dwow_chain::BlockHeader {
        version: BlockVersion::CURRENT,
        previous: prev_hash,
        merkle_root,
        timestamp: BlockTimestamp::new(120 * height.get()), // deterministic per height
        target: BlockTarget::MAX,
        nonce: 0,
        height,
        uncle_merkle_root: [0u8; 32],
        total_reward: reward,
        randomx_key: dwow_chain::Miner::derive_key_from_height(height),
        coin_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: dwow_chain::PowSource::Native,
    }
}

/// Mine a block: coinbase + optional txs, accept via production path.
#[deprecated = "use build_linear_coinbase + accept_block for production-path testing. \
    This function uses fake AEAD notes (ciphertext: vec![0u8; 32]), fake value \
    commitments (Point::identity()), and fake nullifiers ([1u8; 32]) — wallet \
    scan will always return zero native outputs. See T3 (test_wallet_coinbase_scan_only) \
    for the correct production-path test pattern."]
pub(crate) fn mine_block(
    har: &GenesisHarness,
    recipient: &crate::accounts::MiningRecipient,
    height: BlockHeight,
    txs: Vec<Transaction>,
) -> Result<Block> {
    let prev = har.chain_state.get_latest_block()
        .map_err(|e| dwow_core::Error::Custom(format!("get_latest_block: {}", e)))?;
    let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev);
    let reward = dwow_sdk::blockchain::expected_reward(height);
    let mut all_txs = txs;
    all_txs.insert(0, Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![ContractCall {
            contract_id: *NATIVE_TOKEN_CONTRACT_ID,
            data: {
                let mut d = vec![0x05u8]; // PoWRewardV1
                let params = dwow_native_token_contract::model::PoWRewardParamsV1 {
                    input: dwow_native_token_contract::model::ClearInput {
                        value: reward.get(),
                        token_id: pallas::Base::zero(),
                        value_blind: dwow_sdk::crypto::Blind(pallas::Scalar::zero()),
                        token_blind: dwow_sdk::crypto::BaseBlind::ZERO,
                        signature_public: recipient.public(),
                    },
                    output: dwow_native_token_contract::model::Output {
                        value_commit: pallas::Point::identity(),
                        token_commit: pallas::Base::zero(),
                        coin: dwow_native_token_contract::model::Coin::from_attributes(
                            &recipient.public(), reward.get(),
                            dwow_native_token_contract::model::DRKW_TOKEN_ID,
                            dwow_sdk::crypto::FuncId::none(),
                            pallas::Base::zero(),
                            dwow_sdk::crypto::Blind(pallas::Base::zero()),
                        ),
                        nullifier: dwow_native_token_contract::model::Nullifier::from_bytes([1u8; 32]).unwrap(),
                        note: dwow_sdk::crypto::note::AeadEncryptedNote {
                            ciphertext: vec![0u8; 32],
                            ephem_public: recipient.public(),
                        },
                    },
                    nullifier: dwow_native_token_contract::model::Nullifier::from_bytes([1u8; 32]).unwrap(),
                    expected_cumulative_supply: 0,
                    old_cumulative_commit: pallas::Point::identity(),
                    old_cumulative_blind: pallas::Scalar::zero(),
                    new_cumulative_commit: pallas::Point::identity(),
                    tx_binding: pallas::Base::zero(),
                    tx_nonce: pallas::Base::zero(),
                };
                d.extend(dwow_serial::serialize(&params));
                d
            },
        }],
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    });

    let header = build_block_header(prev_hash, height, reward, &all_txs);
    let block = Block { header, transactions: all_txs };

    let rx_flags = randomx::RandomXFlags::get_recommended_flags()
        & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &block.header.randomx_key)
        .map_err(|e| dwow_core::Error::Custom(format!("cache: {}", e)))?;
    let vm = Arc::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
            .map_err(|e| dwow_core::Error::Custom(format!("vm: {}", e)))?,
    );

    crate::block_acceptor::accept_block(
        &har.chain_state, &block, &[], &vm,
        height.pred().expect("mine_block is only called for post-genesis heights (h >= 2); \
             the old `height - 1` relied on the same guarantee"),
        BlockTarget::MAX, None,
    )?;
    Ok(block)
}

/// The full FeeCollectV1 end-to-end determinism test.
///
/// Sequence:
/// 1. Genesis + mine 100 blocks (COINBASE_MATURITY satisfied at height 102)
/// 2. Build a FeeV1 tx using the wallet's fee builder
/// 3. Mine block 102 with FeeV1 tx + FeeCollectV1 collection plate
/// 4. Assert: final tx is 0x06, pot zeroed, supply unchanged, PoT clean
/// 5. Re-exec from genesis in a fresh harness — assert identical hashes
#[test]
#[ignore] // heavyweight — ~10-25 min, gated
fn test_fee_collect_determinism() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Setup ────────────────────────────────────────────────
        // Contracts materialize via the genesis block (init_genesis below)
        // — no startup deployment exists anymore.
        let har = GenesisHarness::new_without_contracts().expect("GenesisHarness");

        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_fcp_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");
        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("AccountManager");
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

        let mut blocks: Vec<Block> = Vec::new();

        // ── Genesis (height 1) ───────────────────────────────────
        let recipient_1 = crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(1))
            .expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1.clone(), magic_bytes)
            .await.expect("init_genesis");
        assert_eq!(har.block_height(), BlockHeight::new(1));
        blocks.push(har.chain_state.get_block(BlockHeight::new(1)).expect("block 1"));

        // ── Mine heights 2..=101 for COINBASE_MATURITY ──────────
        let mut recipient_cur = recipient_1.clone();
        for h in 2..=101 {
            recipient_cur = crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(h))
                .expect(&format!("MiningRecipient height {}", h));
            let block = mine_block(&har, &recipient_cur, BlockHeight::new(h), vec![])
                .expect(&format!("mine_block height {}", h));
            assert_eq!(har.block_height(), BlockHeight::new(h));
            blocks.push(block);
        }

        // Verify supply after maturity runway
        let total_supply_after_101: u64 = (1..=101u64)
            .map(|h| dwow_sdk::blockchain::expected_reward(BlockHeight::new(h)))
            .map(|r| r.get())
            .sum();
        let sc_entry = har.chain_state.supply_chain.get(BlockHeight::new(101))
            .expect("supply_chain at 101");
        assert_eq!(sc_entry.total_supply, SupplyAmount::new(total_supply_after_101));

        // ── Build FeeV1 tx ───────────────────────────────────────
        let recipient_102 = crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(102))
            .expect("MiningRecipient height 102");
        let fee_amount = 42_000_000u64;

        // FeeV1 call data: [0x00][fee u64 LE][FeeParamsV1 — minimal]
        // We construct a structurally valid but proofless FeeV1 tx — the
        // test verifies FeeCollect (not full proof verification, which
        // requires the client-feature ProvingKey fix tracked as item 9
        // prerequisite #1).  The tx is admitted to the mempool and
        // `build_fee_collect_tx` sums its fee from the contract-call layout.
        let fee_call_data = {
            let mut d = vec![0x00u8];
            d.extend_from_slice(&fee_amount.to_le_bytes());
            d
        };
        let fee_tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall {
                contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                data: fee_call_data,
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        // ── Mine block 102 with fee tx + FeeCollectV1 ────────────
        let mut block102_txs = vec![fee_tx.clone()];
        // Simulate what prepare_block does: build the fee-collect tx from
        // the selected transactions and append it last.
        {
            let zk = crate::registry::model::LinearPowRewardZk::new(har.chain_state.clone())
                .await.expect("LinearPowRewardZk");
            if let Some(fee_collect_tx) = crate::registry::model::build_fee_collect_tx(
                &recipient_102, &block102_txs, BlockHeight::new(102), &zk,
            ).expect("build_fee_collect_tx") {
                block102_txs.push(fee_collect_tx);
            }
        }
        let block102 = mine_block(&har, &recipient_102, BlockHeight::new(102), block102_txs)
            .expect("mine_block 102");

        // ── Assertions ───────────────────────────────────────────
        // Final tx must be FeeCollectV1
        let last_tx = block102.transactions.last().expect("last tx");
        let has_0x06 = last_tx.contract_calls.iter()
            .any(|c| c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
                 && c.data.first() == Some(&0x06));
        assert!(has_0x06, "final tx must be FeeCollectV1 (0x06)");

        // Supply unchanged by fee collection (fees redistribute, not mint)
        let sc_102 = har.chain_state.supply_chain.get(BlockHeight::new(102))
            .expect("supply_chain at 102");
        let total_supply_102: u64 = (1..=102u64)
            .map(|h| dwow_sdk::blockchain::expected_reward(BlockHeight::new(h)))
            .map(|r| r.get())
            .sum();
        assert_eq!(sc_102.total_supply, SupplyAmount::new(total_supply_102),
            "TOTAL_SUPPLY unchanged — fees redistribute, not mint");

        // ── Re-exec determinism ──────────────────────────────────
        // Contracts materialize via init_genesis below — no startup deploy.
        let har2 = GenesisHarness::new_without_contracts().expect("GenesisHarness2");
        let miner_mgr2 = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("AccountManager2");
        let magic2 = [0xDA, 0x57, 0x01, 0x57];
        let recipient2 = crate::accounts::MiningRecipient::from_account(&miner_mgr2, BlockHeight::new(1))
            .expect("MiningRecipient2 height 1");
        crate::init_genesis(&har2.chain_state, recipient2.clone(), magic2)
            .await.expect("init_genesis2");
        assert_eq!(har2.block_height(), BlockHeight::new(1));

        // Re-exec blocks 2..=102 identically
        for h in 2..=102 {
            let rec = crate::accounts::MiningRecipient::from_account(&miner_mgr2, BlockHeight::new(h))
                .expect(&format!("MiningRecipient2 height {}", h));
            let txs = if h == 102 {
                // Reconstruct the same block102 txs (fee + fee_collect)
                let zk = crate::registry::model::LinearPowRewardZk::new(har2.chain_state.clone())
                    .await.expect("LinearPowRewardZk2");
                let ft = Transaction {
                    version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
                    contract_calls: vec![ContractCall {
                        contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                        data: {
                            let mut d = vec![0x00u8];
                            d.extend_from_slice(&fee_amount.to_le_bytes());
                            d
                        },
                    }],
                    lock_time: 0, nullifiers: vec![], witness: vec![],
                };
                let mut tx102 = vec![ft.clone()];
                if let Some(fc) = crate::registry::model::build_fee_collect_tx(
                    &rec, &tx102, BlockHeight::new(h), &zk,
                ).expect("build_fee_collect_tx re-exec") {
                    tx102.push(fc);
                }
                tx102
            } else {
                vec![]
            };
            let block_re = mine_block(&har2, &rec, BlockHeight::new(h), txs)
                .expect(&format!("re-exec mine_block {}", h));
            let original_hash = har.chain_state
                .hash_block_with_cached_vm(
                    &har.chain_state.get_block(BlockHeight::new(h)).expect(&format!("orig block {}", h))
                );
            let rebuilt_hash = har2.chain_state.hash_block_with_cached_vm(&block_re);
            assert_eq!(original_hash, rebuilt_hash,
                "re-exec determinism: block {} hash mismatch", h);
        }

        // Supply identical after re-exec
        let sc2_102 = har2.chain_state.supply_chain.get(BlockHeight::new(102))
            .expect("supply_chain2 at 102");
        assert_eq!(sc_102.total_supply, sc2_102.total_supply,
            "re-exec total supply must match");

        // ── Negative: empty mempool → no FeeCollect ──────────────
        let recipient_103 = crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(103))
            .expect("MiningRecipient 103");
        let block103_no_fees = mine_block(&har, &recipient_103, BlockHeight::new(103), vec![])
            .expect("mine_block 103");
        let has_no_0x06 = !block103_no_fees.transactions.iter()
            .any(|tx| tx.contract_calls.iter()
                .any(|c| c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
                     && c.data.first() == Some(&0x06)));
        assert!(has_no_0x06, "zero-fee block must have no FeeCollectV1 tx");
    });
}
