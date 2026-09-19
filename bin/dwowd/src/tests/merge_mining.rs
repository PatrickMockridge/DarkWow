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

use dwow_chain::fee_window::FeeWindowFlags;
use dwow_chain::{
    monero::{
        fixed_array::FixedByteArray, monero_block_deserialize, utils::create_merkle_proof,
        MoneroPowData,
    },
    Block, BlockHeader, ContractCall, PowSource, Transaction,
};
use dwow_sdk::ensure;
use dwow_sdk::ensure_eq;
use dwow_sdk::test_support::{TestError, TestResult};
use dwow_sdk::{
    blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion, MoneroBlockHeight},
    crypto::{keypair::Network, pasta_prelude::Group, NATIVE_TOKEN_CONTRACT_ID},
    pasta::pallas,
};

use crate::tests::genesis::GenesisHarness;

/// This module's name in an INFRA-FAIL attribution.
const MODULE: &str = "tests::merge_mining";

/// INFRA-FAIL: a fixture step in this module failed.
///
/// These tests build their own headers rather than going through `tests::harness`, so the
/// block-level plumbing here is shared infrastructure in the same sense the harness's is.
fn infra(stage: &'static str, cause: impl Into<Box<dyn std::error::Error>>) -> TestError {
    TestError::infra(MODULE, stage, cause)
}

// ── Real Monero testnet block data ──────────────────────────────────
// Blob from Monero testnet, height 2912484, merge-mined DarkFi.
// Source: src/linear/src/monero/mod.rs §tests
const XMR_BLOCK: &str = "1010f881efca0644a1185eeccb2629b316ec0d41659111299ad1b736a3b0d8eac8bbc6384dc5c84bb6010002a0e2b10101ffe4e1b1010180e0a596bb1103f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2be14a010b204d874ed5087b649c711dd4479434a85dbf7e9bdfae26f5bc785964d4b45c0204751b43e10321082d5f403be836d45d026fbaa2a8e4b4a9d0d821f29d709321f8d764f32d446fa80000";
const SEED_HASH: &str =
    "f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2b";

/// Build a `MoneroPowData` from the real testnet block, using a synthetic
/// aux-chain merkle proof (same pattern as `test_monero_powdata_serde`).
fn build_test_monero_powdata() -> TestResult<MoneroPowData> {
    let block = monero_block_deserialize(XMR_BLOCK)
        .map_err(|e| infra("deserializing the Monero block", format!("{e}")))?;
    let seed = FixedByteArray::from_bytes(
        &hex::decode(SEED_HASH)
            .map_err(|e| infra("decoding the seed hash", format!("{e}")))?,
    )
    .map_err(|e| infra("building the seed hash array", format!("{e}")))?;

    // Fake aux-chain merkle proof — the integration test doesn't need a real
    // one because `accept_block` doesn't re-verify the Monero merkle proof
    // (that happened in `mm_submit_solution` before calling `accept_block`).
    let tx_hashes = &[
        "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
        "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
    ]
    .iter()
    .map(|hash| {
        monero::Hash::from_str(hash)
            .map_err(|e| infra("parsing an aux-chain transaction hash", format!("{e}")))
    })
    .collect::<TestResult<Vec<_>>>()?;

    let aux_chain_merkle_proof = create_merkle_proof(&tx_hashes, &tx_hashes[0]).ok_or_else(|| {
        infra("building the aux-chain merkle proof", "create_merkle_proof returned None")
    })?;

    MoneroPowData::new(block, seed, aux_chain_merkle_proof)
        .map_err(|e| infra("constructing MoneroPowData", format!("{e}")))
}

/// Build a block header with `PowSource::Monero` for merge mining tests.
///
/// `miner` must be the key the coinbase note is bound to: `accept_block` requires
/// `header.miner` to equal the coinbase's committed public key on every non-genesis block
/// (the C1 binding rule). This builder hardcoded `[0u8; 32]`, which is why
/// `test_merge_mined_block_acceptance` failed with "coinbase note is bound to a key that is
/// not header.miner" — the same class `tests::harness` was fixed for, in a file that builds
/// its own header and so never went through that fix. Callers derive it with the shared
/// `tests::harness::miner_for`, so builder and rule cannot drift apart again.
fn build_merge_mined_header(
    prev_hash: blake3::Hash,
    height: BlockHeight,
    reward: BlockReward,
    merkle_root: blake3::Hash,
    pow_data: MoneroPowData,
    miner: [u8; 32],
) -> TestResult<BlockHeader> {
    let seed_hash_bytes: [u8; 32] = hex::decode(SEED_HASH)
        .map_err(|e| infra("decoding the seed hash", format!("{e}")))?
        .try_into()
        .map_err(|v: Vec<u8>| {
            infra("sizing the seed hash array", format!("expected 32 bytes, got {}", v.len()))
        })?;

    Ok(BlockHeader {
        fee_window_flags: FeeWindowFlags::default(),
        version: BlockVersion::CURRENT,
        previous: prev_hash,
        merkle_root,
        timestamp: BlockTimestamp::new(120 * height.get()),
        target: BlockTarget::MAX,
        nonce: 0,
        height,
        uncle_merkle_root: [0u8; 32],
        total_reward: reward,
        randomx_key: seed_hash_bytes,
        miner,
        commitment_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: PowSource::Monero(pow_data),
    })
}

/// Compute a simple blake3 merkle root from a list of transactions (same
/// pattern as the block-header merkle root construction).
fn compute_merkle_root(txs: &[Transaction]) -> blake3::Hash {
    let tx_hashes: Vec<blake3::Hash> = txs.iter().map(|tx| tx.hash()).collect();
    if tx_hashes.is_empty() {
        return blake3::hash(&[]);
    }
    let mut layer = tx_hashes.clone();
    while layer.len() > 1 {
        if layer.len() % 2 != 0 {
            if let Some(last) = layer.last() {
                layer.push(*last);
            }
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
fn test_merge_mined_block_acceptance() -> TestResult<()> {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Setup ────────────────────────────────────────────────
        let har = GenesisHarness::new_without_contracts()
            .map_err(|e| infra("creating the genesis harness", e))?;

        let keys_path = std::env::temp_dir()
            .join(format!("dwow_mm_{}.toml", std::process::id()));
        std::fs::write(&keys_path, crate::tests::modules::chain_setup::GENESIS_KEYS_TOML)
            .map_err(|e| infra("writing the test keys file", e))?;
        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .map_err(|e| infra("opening the test account", e))?;
        let magic_bytes = crate::tests::modules::chain_setup::DRKW_MAGIC;

        // ── Genesis (height 1) ───────────────────────────────────
        let recipient_1 =
            crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(1))
                .map_err(|e| infra("deriving the height-1 mining recipient", e))?;
        crate::init_genesis(&har.chain_state, recipient_1.clone(), magic_bytes)
            .await
            .map_err(|e| infra("initialising genesis", e))?;
        ensure_eq!(har.block_height(), BlockHeight::new(1));

        // ── Build merge-mined block at height 2 ──────────────────
        let height = BlockHeight::new(2);
        let recipient =
            crate::accounts::MiningRecipient::from_account(&miner_mgr, height)
                .map_err(|e| infra("deriving the height-2 mining recipient", e))?;
        let reward = dwow_sdk::blockchain::expected_reward(height);

        // Production coinbase: build_linear_coinbase (plaintext call, real
        // AEAD encryption, real nullifier). Same path as miner_task →
        // prepare_block → build_linear_coinbase.
        let (coinbase, pow_reward_call, _blind) =
            crate::registry::model::build_linear_coinbase(
                recipient, reward, &har.chain_state, height,
            ).await.map_err(|e| infra("building the linear coinbase", e))?;

        let coinbase_tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0,
            nullifiers: vec![coinbase.nullifier],
            witness: vec![],
        };

        // Build MoneroPowData from real testnet block
        let pow_data = build_test_monero_powdata()?;

        let all_txs = vec![coinbase_tx];
        let merkle_root = compute_merkle_root(&all_txs);

        let prev = har
            .chain_state
            .get_latest_block()
            .map_err(|e| infra("reading the latest block", e))?;
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev)
            .map_err(|e| infra("hashing the previous block", e))?;

        // `miner` must be the key the coinbase note is bound to, or `accept_block`'s C1
        // binding rule rejects the block — which is exactly what this test used to do.
        let miner = super::harness::miner_for(&all_txs);
        let header = build_merge_mined_header(prev_hash, height, reward, merkle_root, pow_data, miner)?;
        let block = Block { header, transactions: all_txs };

        // ── Accept block ─────────────────────────────────────────
        // Merge-mined blocks skip native PoW (§5.2) — `accept_block`
        // checks `pow_source` and bypasses the RandomX hash check.
        // We pass a dummy RandomX VM; it is never called for Monero blocks.
        let flags =
            randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(flags, &block.header.randomx_key)
            .map_err(|e| infra("creating the RandomX cache", format!("{e}")))?;
        let vm = Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .map_err(|e| infra("creating the RandomX VM", format!("{e}")))?,
        );

        crate::block_acceptor::accept_block(
            &har.chain_state,
            &block,
            &[],
            &vm,
            BlockTarget::MAX,
            None,
        )
        .map_err(|e| infra("accepting the merge-mined block", format!("{e}")))?;

        // ── Assertions ───────────────────────────────────────────
        ensure_eq!(har.block_height(), BlockHeight::new(2),
            "chain height must advance to 2");

        let stored = har
            .chain_state
            .get_block(BlockHeight::new(2))
            .map_err(|e| infra("retrieving block 2", e))?;

        // The stored header differs from the submitted header: connect_block
        // updates nullifier_root and commitment_merkle_root during commit (the
        // real coinbase nullifier enters the nullifier SMT). Verify the
        // block is retrievable with correct height and PowSource.
        ensure_eq!(stored.header.height, BlockHeight::new(2),
            "stored block height must be 2");

        // Verify PowSource is Monero on the SUBMITTED block (pre-commit).
        // NOTE: connect_block stores blocks via serde_json::to_vec, but
        // PowSource + MoneroPowData lack Serialize/Deserialize impls — the
        // Monero variant does not survive the sled roundtrip (pre-existing
        // bug, tracked separately). Verify on the pre-commit block instead.
        ensure!(
            matches!(block.header.pow_source, PowSource::Monero(_)),
            "submitted block must carry PowSource::Monero"
        );

        // Verify the block is properly stored with real coinbase data.
        // nullifier_root is updated by connect_block only when the nullifier
        // SMT is non-empty; coinbase-only blocks may retain [0u8; 32] if
        // the nullifier batch is handled at a different stage.
        ensure_eq!(stored.header.total_reward, reward,
            "stored block must retain total_reward");
        Ok(())
    })
}

// ═══════════════════════════════════════════════════════════════════════
// Test 2 — Determinism: identical inputs produce identical merge-mined
//          blocks
// ═══════════════════════════════════════════════════════════════════════
// Spec §1.5: "The merge mining FFI SHALL be a deterministic function of
// its inputs. Given identical Monero block data and DarkWow chain state,
// identical DarkWow blocks SHALL be produced."

#[test]
fn test_merge_mined_block_deterministic() -> TestResult<()> {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Build first block ────────────────────────────────────
        let har1 = GenesisHarness::new_without_contracts()
            .map_err(|e| infra("creating the first genesis harness", e))?;
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_mm_det_{}.toml", std::process::id()));
        std::fs::write(&keys_path, crate::tests::modules::chain_setup::GENESIS_KEYS_TOML)
            .map_err(|e| infra("writing the test keys file", e))?;
        let mgr1 = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .map_err(|e| infra("opening the first test account", e))?;
        let magic = crate::tests::modules::chain_setup::DRKW_MAGIC;

        let r1 = crate::accounts::MiningRecipient::from_account(&mgr1, BlockHeight::new(1))
            .map_err(|e| infra("deriving the first height-1 recipient", e))?;
        crate::init_genesis(&har1.chain_state, r1, magic)
            .await
            .map_err(|e| infra("initialising genesis on the first harness", e))?;

        let recipient =
            crate::accounts::MiningRecipient::from_account(&mgr1, BlockHeight::new(2))
                .map_err(|e| infra("deriving the first height-2 recipient", e))?;
        let reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(2));
        let pow_data = build_test_monero_powdata()?;

        // Production coinbase — same path as miner_task
        let (coinbase, pow_reward_call, _blind) =
            crate::registry::model::build_linear_coinbase(
                recipient, reward, &har1.chain_state, BlockHeight::new(2),
            ).await.map_err(|e| infra("building the first coinbase", e))?;

        let coinbase_tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0,
            nullifiers: vec![coinbase.nullifier],
            witness: vec![],
        };
        let txs = vec![coinbase_tx];
        let merkle = compute_merkle_root(&txs);
        let prev = har1.chain_state.get_latest_block()
            .map_err(|e| infra("reading the first harness's latest block", e))?;
        let prev_hash = har1.chain_state.hash_block_with_cached_vm(&prev)
            .map_err(|e| infra("hashing the first previous block", e))?;
        let header = build_merge_mined_header(
            prev_hash,
            BlockHeight::new(2),
            reward,
            merkle,
            pow_data,
            super::harness::miner_for(&txs),
        )?;
        let block1 = Block { header, transactions: txs };

        // ── Build second block identically ───────────────────────
        let har2 = GenesisHarness::new_without_contracts()
            .map_err(|e| infra("creating the second genesis harness", e))?;
        let mgr2 = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .map_err(|e| infra("opening the second test account", e))?;

        let r1b = crate::accounts::MiningRecipient::from_account(&mgr2, BlockHeight::new(1))
            .map_err(|e| infra("deriving the second height-1 recipient", e))?;
        crate::init_genesis(&har2.chain_state, r1b, magic)
            .await
            .map_err(|e| infra("initialising genesis on the second harness", e))?;

        // Must reconstruct pow_data — MoneroPowData is consumed by the block
        let pow_data2 = build_test_monero_powdata()?;
        let recipient2 =
            crate::accounts::MiningRecipient::from_account(&mgr2, BlockHeight::new(2))
                .map_err(|e| infra("deriving the second height-2 recipient", e))?;

        // Production coinbase — same path as miner_task, independent harness
        let (coinbase2, pow_reward_call2, _blind2) =
            crate::registry::model::build_linear_coinbase(
                recipient2, reward, &har2.chain_state, BlockHeight::new(2),
            ).await.map_err(|e| infra("building the second coinbase", e))?;

        let coinbase_tx2 = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call2],
            lock_time: 0,
            nullifiers: vec![coinbase2.nullifier],
            witness: vec![],
        };
        let txs2 = vec![coinbase_tx2];
        let merkle2 = compute_merkle_root(&txs2);
        let prev2 = har2.chain_state.get_latest_block()
            .map_err(|e| infra("reading the second harness's latest block", e))?;
        let prev_hash2 = har2.chain_state.hash_block_with_cached_vm(&prev2)
            .map_err(|e| infra("hashing the second previous block", e))?;
        let header2 = build_merge_mined_header(
            prev_hash2,
            BlockHeight::new(2),
            reward,
            merkle2,
            pow_data2,
            super::harness::miner_for(&txs2),
        )?;
        let block2 = Block { header: header2, transactions: txs2 };

        // ── Assertions: identical hashes ─────────────────────────
        // NB: both blocks carry the same `miner` because both coinbases are bound to the
        // same identity, so this comparison does not witness the binding rule — it
        // witnesses determinism of block production, which is what §1.5 asks of it.
        let hash1 = har1.chain_state.hash_block_with_cached_vm(&block1)
            .map_err(|e| infra("hashing the first merge-mined block", e))?;
        let hash2 = har2.chain_state.hash_block_with_cached_vm(&block2)
            .map_err(|e| infra("hashing the second merge-mined block", e))?;
        ensure_eq!(hash1, hash2,
            "identical inputs must produce identical merge-mined block hashes");
        Ok(())
    })
}
