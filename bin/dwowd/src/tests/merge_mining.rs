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

//! Merge mining integration tests.
//!
//! Verifies spec-mandated behaviors from
//! `doc/src/arch/consensus/merge-mining-ffi.md`:
//!
//! - §5.3: merge-mined blocks enter the standard `accept_block` path
//! - §2.3/§5.2: `PowSource::Monero` skips native RandomX PoW verification
//! - §1.5: merge-mined block production is deterministic
//!
//! These tests use real Monero testnet block data (height 2912484) to
//! construct a valid `MoneroPowData` without requiring monerod or p2pool.

use std::sync::Arc;

use std::str::FromStr;

use dwow_chain::{
    monero::{
        create_ordered_tx_hashes_from_block,
        fixed_array::FixedByteArray, monero_block_deserialize, utils::create_merkle_proof,
        MoneroPowData,
    },
    Block, BlockHeader, ContractCall, PowSource, Transaction,
};
use dwow_core::Result;
use dwow_sdk::{
    blockchain::{BlockHeight, BlockReward, BlockTarget},
    blockchain::BlockHeight,
    crypto::{keypair::Network, pasta_prelude::Group, NATIVE_TOKEN_CONTRACT_ID},
    pasta::pallas,
};

use crate::tests::genesis::GenesisHarness;

// ── Real Monero testnet block data ──────────────────────────────────
// Blob from Monero testnet, height 2912484, merge-mined DarkFi.
// Source: src/linear/src/monero/mod.rs §tests
const XMR_BLOCK: &str = "1010f881efca0644a1185eeccb2629b316ec0d41659111299ad1b736a3b0d8eac8bbc6384dc5c84bb6010002a0e2b10101ffe4e1b1010180e0a596bb1103f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2be14a010b204d874ed5087b649c711dd4479434a85dbf7e9bdfae26f5bc785964d4b45c0204751b43e10321082d5f403be836d45d026fbaa2a8e4b4a9d0d821f29d709321f8d764f32d446fa80000";
const SEED_HASH: &str =
    "f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2b";

/// Build a `MoneroPowData` from the real testnet block, using a synthetic
/// aux-chain merkle proof (same pattern as `test_monero_powdata_serde`).
fn build_test_monero_powdata() -> Result<MoneroPowData> {
    let block = monero_block_deserialize(XMR_BLOCK)
        .map_err(|e| dwow_core::Error::Custom(format!("monero_block_deserialize: {e}")))?;
    let seed = FixedByteArray::from_bytes(
        &hex::decode(SEED_HASH)
            .map_err(|e| dwow_core::Error::Custom(format!("hex decode seed_hash: {e}")))?,
    )
    .map_err(|e| dwow_core::Error::Custom(format!("FixedByteArray::from_bytes: {e}")))?;

    // Fake aux-chain merkle proof — the integration test doesn't need a real
    // one because `accept_block` doesn't re-verify the Monero merkle proof
    // (that happened in `mm_submit_solution` before calling `accept_block`).
    let tx_hashes = &[
        "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
        "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
    ]
    .iter()
    .map(|hash| {
        monero::Hash::from_str(hash).map_err(|e| {
            dwow_core::Error::Custom(format!("Hash::from_str: {e}"))
        })
    })
    .collect::<Result<Vec<_>>>()?;

    let aux_chain_merkle_proof = create_merkle_proof(&tx_hashes, &tx_hashes[0])
        .ok_or_else(|| dwow_core::Error::Custom("create_merkle_proof failed".into()))?;

    MoneroPowData::new(block, seed, aux_chain_merkle_proof)
        .map_err(|e| dwow_core::Error::Custom(format!("MoneroPowData::new: {e}")))
}

/// Build a block header with `PowSource::Monero` for merge mining tests.
fn build_merge_mined_header(
    prev_hash: blake3::Hash,
    height: BlockHeight,
    reward: BlockReward,
    merkle_root: blake3::Hash,
    pow_data: MoneroPowData,
) -> BlockHeader {
    let seed_hash_bytes: [u8; 32] = hex::decode(SEED_HASH)
        .expect("SEED_HASH decodes")
        .try_into()
        .expect("SEED_HASH is 32 bytes");

    BlockHeader {
        version: 1,
        previous: prev_hash,
        merkle_root,
        timestamp: 120 * height.get(),
        target: BlockTarget::MAX,
        nonce: 0,
        height,
        uncle_merkle_root: [0u8; 32],
        total_reward: reward,
        randomx_key: seed_hash_bytes,
        coin_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: 0,
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: PowSource::Monero(pow_data),
    }
}

/// Build a synthetic coinbase transaction (PoWRewardV1 call data without ZK
/// proofs — matches the pattern in `fee_collect_pipeline::mine_block`).
fn build_coinbase_tx(
    recipient: &crate::accounts::MiningRecipient,
    reward: u64,
) -> Transaction {
    Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![ContractCall {
            contract_id: *NATIVE_TOKEN_CONTRACT_ID,
            data: {
                let mut d = vec![0x05u8]; // PoWRewardV1 selector
                let params = dwow_native_token_contract::model::PoWRewardParamsV1 {
                    input: dwow_native_token_contract::model::ClearInput {
                        value: reward,
                        token_id: pallas::Base::zero(),
                        value_blind: dwow_sdk::crypto::Blind(pallas::Scalar::zero()),
                        token_blind: dwow_sdk::crypto::BaseBlind::ZERO,
                        signature_public: recipient.public(),
                    },
                    output: dwow_native_token_contract::model::Output {
                        value_commit: pallas::Point::identity(),
                        token_commit: pallas::Base::zero(),
                        coin: dwow_native_token_contract::model::Coin::from_attributes(
                            &recipient.public(),
                            reward,
                            dwow_native_token_contract::model::DRKW_TOKEN_ID,
                            dwow_sdk::crypto::FuncId::none(),
                            pallas::Base::zero(),
                            dwow_sdk::crypto::Blind(pallas::Base::zero()),
                        ),
                        nullifier: dwow_native_token_contract::model::Nullifier::from_bytes(
                            [1u8; 32],
                        )
                        .unwrap(),
                        note: dwow_sdk::crypto::note::AeadEncryptedNote {
                            ciphertext: vec![0u8; 32],
                            ephem_public: recipient.public(),
                        },
                    },
                    nullifier: dwow_native_token_contract::model::Nullifier::from_bytes(
                        [1u8; 32],
                    )
                    .unwrap(),
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
    }
}

/// Compute a simple blake3 merkle root from a list of transactions (same
/// pattern as `fee_collect_pipeline::build_block_header`).
fn compute_merkle_root(txs: &[Transaction]) -> blake3::Hash {
    let tx_hashes: Vec<blake3::Hash> = txs.iter().map(|tx| tx.hash()).collect();
    if tx_hashes.is_empty() {
        return blake3::hash(&[]);
    }
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

// ═══════════════════════════════════════════════════════════════════════
// Test 1 — Positive: merge-mined block accepted via standard path
// ═══════════════════════════════════════════════════════════════════════
// Spec §5.3: "After reconstruction, the block SHALL enter the standard
// acceptance path (`accept_block()`)."
// Spec §2.3/§5.2: "`PowSource::Monero` blocks SHALL skip native RandomX
// PoW verification."

#[test]
fn test_merge_mined_block_acceptance() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Setup ────────────────────────────────────────────────
        let har = GenesisHarness::new_without_contracts().expect("GenesisHarness");

        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_mm_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");
        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .expect("AccountManager");
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

        // ── Genesis (height 1) ───────────────────────────────────
        let recipient_1 =
            crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(1))
                .expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1.clone(), magic_bytes)
            .await
            .expect("init_genesis");
        assert_eq!(har.block_height(), BlockHeight::new(1));

        // ── Build merge-mined block at height 2 ──────────────────
        let height = BlockHeight::new(2);
        let recipient =
            crate::accounts::MiningRecipient::from_account(&miner_mgr, height)
                .expect("MiningRecipient height 2");
        let reward = dwow_sdk::blockchain::expected_reward(height);

        // Build MoneroPowData from real testnet block
        let pow_data = build_test_monero_powdata().expect("MoneroPowData");

        // Build coinbase + block
        let coinbase = build_coinbase_tx(&recipient, reward.get());
        let all_txs = vec![coinbase];
        let merkle_root = compute_merkle_root(&all_txs);

        let prev = har
            .chain_state
            .get_latest_block()
            .expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev);

        let header =
            build_merge_mined_header(prev_hash, height, reward, merkle_root, pow_data);
        let block = Block { header, transactions: all_txs };

        // ── Accept block ─────────────────────────────────────────
        // Merge-mined blocks skip native PoW (§5.2) — `accept_block`
        // checks `pow_source` and bypasses the RandomX hash check.
        // We pass a dummy RandomX VM; it is never called for Monero blocks.
        let flags =
            randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(flags, &block.header.randomx_key)
            .expect("RandomXCache");
        let vm = Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .expect("RandomXVM"),
        );

        crate::block_acceptor::accept_block(
            &har.chain_state,
            &block,
            &[],
            &vm,
            height.pred().expect("height 2 has pred"),
            BlockTarget::MAX,
            None,
        )
        .expect("accept_block for merge-mined block");

        // ── Assertions ───────────────────────────────────────────
        assert_eq!(har.block_height(), BlockHeight::new(2),
            "chain height must advance to 2");

        let stored = har
            .chain_state
            .get_block(BlockHeight::new(2))
            .expect("block 2 retrievable");

        let stored_hash = har.chain_state.hash_block_with_cached_vm(&stored);
        let original_hash = har.chain_state.hash_block_with_cached_vm(&block);
        assert_eq!(stored_hash, original_hash,
            "stored block hash must match submitted");

        // Verify PowSource is Monero (not Native)
        assert!(
            matches!(stored.header.pow_source, PowSource::Monero(_)),
            "block must retain PowSource::Monero"
        );
    });
}

// ═══════════════════════════════════════════════════════════════════════
// Test 2 — Determinism: identical inputs produce identical merge-mined
//          blocks
// ═══════════════════════════════════════════════════════════════════════
// Spec §1.5: "The merge mining FFI SHALL be a deterministic function of
// its inputs. Given identical Monero block data and DarkWow chain state,
// identical DarkWow blocks SHALL be produced."

#[test]
fn test_merge_mined_block_deterministic() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Build first block ────────────────────────────────────
        let har1 = GenesisHarness::new_without_contracts().expect("GenesisHarness1");
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_mm_det_{}.toml", std::process::id()));
        std::fs::write(
            &keys_path,
            "[node0]\nwallet_secret = \
             \"0100000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .expect("write keys");
        let mgr1 = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .expect("AccountManager1");
        let magic = [0xDA, 0x57, 0x01, 0x57];

        let r1 = crate::accounts::MiningRecipient::from_account(&mgr1, BlockHeight::new(1))
            .expect("recipient1");
        crate::init_genesis(&har1.chain_state, r1, magic)
            .await
            .expect("init_genesis1");

        let recipient =
            crate::accounts::MiningRecipient::from_account(&mgr1, BlockHeight::new(2))
                .expect("recipient");
        let reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(2));
        let pow_data = build_test_monero_powdata().expect("MoneroPowData");
        let coinbase = build_coinbase_tx(&recipient, reward.get());
        let txs = vec![coinbase];
        let merkle = compute_merkle_root(&txs);
        let prev = har1.chain_state.get_latest_block().expect("prev");
        let prev_hash = har1.chain_state.hash_block_with_cached_vm(&prev);
        let header = build_merge_mined_header(
            prev_hash,
            BlockHeight::new(2),
            reward,
            merkle,
            pow_data,
        );
        let block1 = Block { header, transactions: txs };

        // ── Build second block identically ───────────────────────
        let har2 = GenesisHarness::new_without_contracts().expect("GenesisHarness2");
        let mgr2 = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .expect("AccountManager2");

        let r1b = crate::accounts::MiningRecipient::from_account(&mgr2, BlockHeight::new(1))
            .expect("recipient1b");
        crate::init_genesis(&har2.chain_state, r1b, magic)
            .await
            .expect("init_genesis2");

        // Must reconstruct pow_data — MoneroPowData is consumed by the block
        let pow_data2 = build_test_monero_powdata().expect("MoneroPowData2");
        let recipient2 =
            crate::accounts::MiningRecipient::from_account(&mgr2, BlockHeight::new(2))
                .expect("recipient2");
        let coinbase2 = build_coinbase_tx(&recipient2, reward.get());
        let txs2 = vec![coinbase2];
        let merkle2 = compute_merkle_root(&txs2);
        let prev2 = har2.chain_state.get_latest_block().expect("prev2");
        let prev_hash2 = har2.chain_state.hash_block_with_cached_vm(&prev2);
        let header2 = build_merge_mined_header(
            prev_hash2,
            BlockHeight::new(2),
            reward,
            merkle2,
            pow_data2,
        );
        let block2 = Block { header: header2, transactions: txs2 };

        // ── Assertions: identical hashes ─────────────────────────
        let hash1 = har1.chain_state.hash_block_with_cached_vm(&block1);
        let hash2 = har2.chain_state.hash_block_with_cached_vm(&block2);
        assert_eq!(hash1, hash2,
            "identical inputs must produce identical merge-mined block hashes");
    });
}
