/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
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

//! Uncle-minting regression test.
//!
//! Spec: uncle_merkle.md §"Uncle Minting & Maturity". Verifies that an accepted
//! uncle's reward is minted as a spendable note (UncleMintV1, 0x07): its commitment
//! is persisted to `commitment_set` at connect time, the canonical coinbase note is
//! minted at the reduced effective value, and `disconnect_block` reverses both.

use std::sync::Arc;

use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction, compute_merkle_root};
use dwow_sdk::blockchain::{
    BlockHeight, BlockReward, BlockTarget, BlockVersion, MoneroBlockHeight, expected_reward,
};
use dwow_sdk::pasta::pallas;
use dwow_sdk::test_support::{Context, TestError, TestResult};
use dwow_sdk::{ensure, ensure_eq};

use crate::Network;

/// This module's name in an INFRA-FAIL attribution.
const MODULE: &str = "tests::uncle_minting";

/// Build genesis (height 1) + a real coinbase block at height 2.
/// Returns the chain state and the miner account manager.
async fn build_chain() -> TestResult<(Arc<dwow_chain::CChainState>, crate::accounts::AccountManager)> {
    use dwow_chain::fee_window::FeeWindowFlags;

    let har = super::genesis::GenesisHarness::new()
        .map_err(|e| TestError::infra(MODULE, "creating the genesis harness", e))?;

    // The canonical genesis identity, not a copy of it: this literal used to be
    // duplicated here, so editing the key silently re-keyed the test.
    let keys_path = std::env::temp_dir().join(format!(
        "dwow_uncle_mint_{}_{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| TestError::infra(MODULE, "reading the system clock", e))?
            .as_nanos(),
    ));
    std::fs::write(&keys_path, super::modules::chain_setup::GENESIS_KEYS_TOML)
        .map_err(|e| TestError::infra(MODULE, "writing the test keys file", e))?;

    let miner_mgr = crate::accounts::AccountManager::open(&keys_path, Network::Testnet, "node0")
        .map_err(|e| TestError::infra(MODULE, "opening the miner account", e))?;
    let chain_magic = crate::tests::modules::chain_setup::DRKW_MAGIC;

    // Block 1: genesis.
    let recipient_1 = crate::accounts::MiningRecipient::from_account(
        &miner_mgr,
        BlockHeight::new(1),
    )
    .map_err(|e| TestError::infra(MODULE, "deriving the height-1 mining recipient", e))?;
    crate::init_genesis(&har.chain_state, recipient_1, chain_magic)
        .await
        .map_err(|e| TestError::infra(MODULE, "initialising genesis", e))?;

    // Block 2: post-genesis coinbase.
    let height_2 = BlockHeight::new(2);
    let reward_2 = expected_reward(height_2);
    let recipient_2 = crate::accounts::MiningRecipient::from_account(&miner_mgr, height_2)
        .map_err(|e| TestError::infra(MODULE, "deriving the height-2 mining recipient", e))?;
    // The coinbase note is bound to this key, and `accept_block` requires `header.miner`
    // to match it (the C1 binding, 55a04076c9). Capture it before `recipient_2` is moved.
    let miner_2 = recipient_2.public().to_bytes();
    let (coinbase_2, _pi_2, pow_call_2, _blind_2) = crate::registry::model::build_linear_coinbase(
        recipient_2,
        reward_2,
        &har.chain_state,
        height_2,
    )
    .await
    .map_err(|e| TestError::infra(MODULE, "building the height-2 coinbase", e))?;
    let coinbase_tx_2 = Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![pow_call_2],
        lock_time: 0,
        nullifiers: vec![coinbase_2.nullifier],
        witness: vec![],
    };
    let prev = har
        .chain_state
        .get_latest_block()
        .map_err(|e| TestError::infra(MODULE, "reading the latest block", e))?;
    let prev_hash = har
        .chain_state
        .hash_block_with_cached_vm(&prev)
        .map_err(|e| TestError::infra(MODULE, "hashing the latest block", e))?;
    let header_2 = BlockHeader {
        fee_window_flags: FeeWindowFlags::default(),
        version: BlockVersion::CURRENT,
        previous: prev_hash,
        merkle_root: compute_merkle_root(&[coinbase_tx_2.clone()]),
        timestamp: dwow_sdk::blockchain::BlockTimestamp::new(120),
        target: BlockTarget::MAX,
        nonce: 0,
        height: height_2,
        uncle_merkle_root: [0u8; 32],
        total_reward: reward_2,
        randomx_key: Miner::derive_key_from_height(height_2),
        miner: miner_2,
        commitment_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: PowSource::Native,
    };
    let block_2 = Block { header: header_2, transactions: vec![coinbase_tx_2] };
    let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
        .map_err(|e| TestError::infra(MODULE, "creating the height-2 RandomX cache", e))?;
    let vm = Arc::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
            .map_err(|e| TestError::infra(MODULE, "creating the height-2 RandomX VM", e))?,
    );
    crate::block_acceptor::accept_block(
        &har.chain_state,
        &block_2,
        &[],
        &vm,
        BlockTarget::MAX,
        None,
    )
    .map_err(|e| TestError::infra(MODULE, "accepting the height-2 block", e))?;

    Ok((har.chain_state, miner_mgr))
}

#[test]
fn test_uncle_note_persisted_and_reversed() -> TestResult<()> {
    dwow_native_token_contract::enable_deterministic_zk();
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();

    smol::block_on(async {
        let (chain_state, miner_mgr) = build_chain().await?;

        let height_3 = BlockHeight::new(3);
        let reward_3 = expected_reward(height_3);
        let recipient_3 = crate::accounts::MiningRecipient::from_account(&miner_mgr, height_3)
            .map_err(|e| TestError::infra(MODULE, "deriving the height-3 mining recipient", e))?;

        // Build an uncle: a competing block at height 2 (depth 1). Its header.miner
        // is set to a valid cycled pk_H so the uncle note can be AEAD-encrypted to it.
        let mut uncle_block =
            super::harness::build_test_block(&chain_state, BlockHeight::new(2), vec![])?;
        uncle_block.header.miner = recipient_3.public().to_bytes();
        let mut uncle = dwow_chain::create_uncle(uncle_block, 1, reward_3);
        uncle.accept_pin();

        // Mint the uncle note (UncleMintV1, 0x07).
        let uncle_tx = crate::registry::model::build_uncle_mint_tx(
            &uncle,
            height_3,
            pallas::Base::from(3u64),
        )
        .map_err(|e| TestError::infra(MODULE, "building the uncle mint transaction", e))?;

        // Extract the uncle note commitment from the transaction.
        let uncle_params = dwow_native_token_contract::model::UncleMintParamsV1::decode(
            &uncle_tx.contract_calls[0].data[1..],
        )
        .map_err(|e| TestError::infra(MODULE, "decoding UncleMintParamsV1", e))?;
        let uncle_commitment =
            dwow_chain::Commitment::from_base(uncle_params.output.commitment.inner());

        // Build block 3: reduced canonical coinbase + the uncle note.
        let effective = reward_3.get() - uncle.pin_confirmed.get();
        let (coinbase_3, _pi_3, pow_call_3, _blind_3) =
            crate::registry::model::build_linear_coinbase_effective(
                recipient_3,
                reward_3,
                BlockReward::new(effective),
                &chain_state,
                height_3,
            )
            .await
            .map_err(|e| TestError::infra(MODULE, "building the effective height-3 coinbase", e))?;
        let coinbase_tx_3 = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_call_3],
            lock_time: 0,
            nullifiers: vec![coinbase_3.nullifier],
            witness: vec![],
        };

        // consensus-coinbase.md §2.5: plaintext rewards carry no ZK proof —
        // the uncle note's serialized core-tx `proofs` must be empty.
        let uncle_core: dwow_core::tx::Transaction = dwow_serial::deserialize(&uncle_tx.witness)
            .map_err(|e| TestError::infra(MODULE, "decoding the uncle core transaction", e))?;
        ensure!(
            uncle_core.proofs.iter().all(|group| group.is_empty()),
            "uncle note must carry no ZK proof"
        );

        let txs = vec![coinbase_tx_3, uncle_tx.clone()];
        let block_3 = super::harness::build_test_block_with_uncles(
            &chain_state,
            height_3,
            txs,
            &[uncle.clone()],
        )?;

        let size_before = chain_state.commitment_set_size();

        // Connect block 3 with the accepted uncle.
        let rx_flags =
            randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_3.header.randomx_key)
            .map_err(|e| TestError::infra(MODULE, "creating the height-3 RandomX cache", e))?;
        let vm = Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                .map_err(|e| TestError::infra(MODULE, "creating the height-3 RandomX VM", e))?,
        );
        crate::block_acceptor::accept_block(
            &chain_state,
            &block_3,
            &[uncle.clone()],
            &vm,
            BlockTarget::MAX,
            None,
        )
        .map_err(|e| TestError::infra(MODULE, "accepting the height-3 block", e))?;

        // The uncle note commitment is persisted (coinbase + uncle note = +2).
        ensure!(
            chain_state.has_commitment(&uncle_commitment),
            "uncle note commitment should be in commitment_set"
        );
        ensure_eq!(
            chain_state.commitment_set_size(),
            size_before + 2,
            "coinbase + uncle note both persisted"
        );

        // Disconnect and assert full reversal.
        chain_state
            .disconnect_block(height_3)
            .map_err(|e| TestError::infra(MODULE, "disconnecting block 3", e))?;
        ensure!(
            !chain_state.has_commitment(&uncle_commitment),
            "uncle note commitment should be removed on disconnect"
        );
        ensure_eq!(
            chain_state.commitment_set_size(),
            size_before,
            "commitment_set restored after disconnect"
        );
        Ok(())
    })
}
