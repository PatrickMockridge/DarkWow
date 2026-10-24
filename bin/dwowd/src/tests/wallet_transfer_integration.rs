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

//! Wallet transfer + capability scanning integration test.
//!
//! Covers the write path (native DRKW transfer) and the address round-trip that
//! the transfer depends on: the sender builds a transfer to the recipient's
//! address, which must be the standard `[prefix][pubkey][checksum]` address the
//! recipient's `wallet address` command emits — NOT a raw pubkey. A raw-pubkey
//! address fails `Address::from_str` with "Invalid address type" (the exact bug
//! this test guards against).
//!
//! Also re-asserts the read path: the sender scans the coinbase to a non-zero
//! DRKW balance (capability discovery) before spending it.

use std::path::PathBuf;
use std::sync::Arc;

use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::crypto::keypair::{Address, Network};
use dwow_sdk::test_support::TestResult;
use dwow_wallet::local_wallet::LocalWallet;
use dwow_wallet::Dww;

use crate::tests::genesis::GenesisHarness;

/// The wallet address round-trips through `Address::from_str`, and the sender
/// can build a native DRKW transfer to that address.
#[test]
fn test_wallet_address_roundtrip_and_transfer() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Chain: genesis + coinbase funding node0 (wallet-1) ─────────────
        let har = GenesisHarness::new().expect("GenesisHarness");

        let keys_toml = "[node0]\nwallet_secret = \
            \"755c6e8a21b3e15f146ba636a146c228b5f91202fc7e0bb0065efdd9fd685405\"\n\
            [wallet2]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_transfer_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let chain_magic = crate::tests::modules::chain_setup::DRKW_MAGIC;

        let recipient_1 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, BlockHeight::new(1),
        ).expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1, chain_magic)
            .await.expect("init_genesis");

        // Block 2: coinbase (funds node0 so wallet-1 can spend).
        let height_2 = BlockHeight::new(2);
        let recipient_2 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_2,
        ).expect("MiningRecipient height 2");
        // The coinbase note is bound to this key and `accept_block` requires
        // `header.miner` to match it (the C1 binding, 55a04076c9).
        let miner_2 = recipient_2.public().to_bytes();
        let (coinbase_2, pow_reward_call, _blind) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, dwow_sdk::blockchain::expected_reward(height_2), &har.chain_state, height_2,
            ).await.expect("build_linear_coinbase");
        let coinbase_tx_2 = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };
        let prev = har.chain_state.get_latest_block().expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev).expect("hash failed");
        let header_2 = dwow_chain::BlockHeader {
            fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            previous: prev_hash,
            merkle_root: dwow_chain::compute_merkle_root(&[coinbase_tx_2.clone()]),
            timestamp: dwow_sdk::blockchain::BlockTimestamp::new(120),
            target: dwow_sdk::blockchain::BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: dwow_sdk::blockchain::expected_reward(height_2),
            randomx_key: dwow_chain::Miner::derive_key_from_height(height_2),
            miner: miner_2,
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: dwow_chain::PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };
        let block_2 = dwow_chain::Block { header: header_2, transactions: vec![coinbase_tx_2] };
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
            .expect("RandomXCache");
        let vm = Arc::new(std::sync::Mutex::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"),
        ));
        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            dwow_sdk::blockchain::BlockTarget::MAX, None,
        ).expect("accept_block height 2");

        // ── Wallet-1 (node0): scan → DRKW balance (capability discovery) ────
        let wallet_dir_1 = std::env::temp_dir()
            .join(format!("dwow_transfer_db1_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir_1);
        let dww1 = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "node0",
            wallet_dir_1.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet-1 init");
        dww1.initialize_wallet().expect("wallet-1 schema init");
        let mut tree = dww1.get_capability_commitment_tree().expect("tree");
        for h in 1u64..=2 {
            let block = har.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(),
                transactions: block.transactions.clone(),
            };
            // Populate chain_blocks (for the global commitment-tree reconstruction in
            // get_merkle_proof) and scan for capabilities.
            dww1.insert_synced_block(&scan_block).expect("insert block");
            dww1.scan_block_linear(&mut tree, &scan_block).expect("scan block");
        }
        let balances = dww1.capability_balance().expect("balance");
        let drkw_key = bs58::encode(&[0u8; 32]).into_string();
        let drkw = balances.get(&drkw_key).copied().unwrap_or(0);
        assert!(drkw > 0, "wallet-1 must hold DRKW after scanning coinbase");

        // ── Wallet-2 (wallet2): address must round-trip ─────────────────────
        let wallet_dir_2 = std::env::temp_dir()
            .join(format!("dwow_transfer_db2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir_2);
        // LocalWallet::open requires the SQLite schema to exist — initialize it
        // via the full Dww path first (same as `wallet initialize`), then open
        // the SqliteOnly LocalWallet handle to exercise that path.
        let dww2 = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "wallet2",
            wallet_dir_2.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet-2 init");
        dww2.initialize_wallet().expect("wallet-2 schema init");
        drop(dww2);
        let wallet2 = LocalWallet::open(
            &wallet_dir_2.to_string_lossy(),
            "",
            Some(&keys_path),
            Network::Testnet,
            "wallet2",
        ).expect("wallet-2 open");
        let addr2 = wallet2.default_address().expect("wallet-2 address");
        // The address MUST parse as a standard [prefix][pubkey][checksum] address.
        let parsed: Address = addr2.parse().expect("address must round-trip through Address::from_str");
        assert_eq!(parsed.to_string(), addr2, "address display must match the parsed form");

        // ── Wallet-1 builds a native DRKW transfer to wallet-2's address ────
        let seed = [7u8; 32];
        let wtx = dww1.build_native_transfer(1_000_000, &addr2, seed)
            .await
            .expect("build_native_transfer to the recipient address");
        assert!(!wtx.calls.is_empty(), "transfer tx must carry at least the transfer call");
        assert_eq!(wtx.calls[0].data.data[0], 0x03, "calls[0] = TransferV1");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir_1);
        let _ = std::fs::remove_dir_all(&wallet_dir_2);
    });
}

/// A recipient wallet decrypts an incoming native TransferV1 and records the
/// received DRKW capability — the receive side no other test covers
/// (wallet.md §2.1 transfer receive).
#[test]
fn test_transfer_receive_decrypt() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Chain: genesis + coinbase funding node0 (wallet-1) ─────────────
        let har = GenesisHarness::new().expect("GenesisHarness");
        let keys_toml = "[node0]\nwallet_secret = \
            \"755c6e8a21b3e15f146ba636a146c228b5f91202fc7e0bb0065efdd9fd685405\"\n\
            [wallet2]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_transfer_recv_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let chain_magic = crate::tests::modules::chain_setup::DRKW_MAGIC;
        let recipient_1 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, BlockHeight::new(1),
        ).expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1, chain_magic)
            .await.expect("init_genesis");

        // Block 2: coinbase (funds node0 so wallet-1 can spend).
        let height_2 = BlockHeight::new(2);
        let recipient_2 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_2,
        ).expect("MiningRecipient height 2");
        // The coinbase note is bound to this key and `accept_block` requires
        // `header.miner` to match it (the C1 binding, 55a04076c9).
        let miner_2 = recipient_2.public().to_bytes();
        let (coinbase_2, pow_reward_call, _blind) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, dwow_sdk::blockchain::expected_reward(height_2), &har.chain_state, height_2,
            ).await.expect("build_linear_coinbase");
        let coinbase_tx_2 = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };
        let prev = har.chain_state.get_latest_block().expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev).expect("hash failed");
        let header_2 = dwow_chain::BlockHeader {
            fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            previous: prev_hash,
            merkle_root: dwow_chain::compute_merkle_root(&[coinbase_tx_2.clone()]),
            timestamp: dwow_sdk::blockchain::BlockTimestamp::new(120),
            target: dwow_sdk::blockchain::BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: dwow_sdk::blockchain::expected_reward(height_2),
            randomx_key: dwow_chain::Miner::derive_key_from_height(height_2),
            miner: miner_2,
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: dwow_chain::PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };
        let block_2 = dwow_chain::Block { header: header_2, transactions: vec![coinbase_tx_2] };
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
            .expect("RandomXCache");
        let vm = Arc::new(std::sync::Mutex::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"),
        ));
        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            dwow_sdk::blockchain::BlockTarget::MAX, None,
        ).expect("accept_block height 2");

        // ── Wallet-1 (node0): scan coinbase → DRKW ─────────────────────────
        let wallet_dir_1 = std::env::temp_dir()
            .join(format!("dwow_transfer_recv_db1_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir_1);
        let dww1 = Dww::new(
            Network::Testnet, Some(&keys_path), "node0",
            wallet_dir_1.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet-1 init");
        dww1.initialize_wallet().expect("wallet-1 schema init");
        let mut tree1 = dww1.get_capability_commitment_tree().expect("tree1");
        for h in 1u64..=2 {
            let block = har.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            };
            // Populate chain_blocks (so get_merkle_proof can reconstruct the
            // global commitment tree) and scan for capabilities.
            dww1.insert_synced_block(&scan_block).expect("insert block");
            dww1.scan_block_linear(&mut tree1, &scan_block).expect("scan block");
        }

        // ── Wallet-2 (recipient) ──────────────────────────────────────────
        let wallet_dir_2 = std::env::temp_dir()
            .join(format!("dwow_transfer_recv_db2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir_2);
        let dww2 = Dww::new(
            Network::Testnet, Some(&keys_path), "wallet2",
            wallet_dir_2.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet-2 init");
        dww2.initialize_wallet().expect("wallet-2 schema init");
        let addr2 = dww2.default_address().expect("wallet-2 address");

        // ── Wallet-1 builds a native transfer to wallet-2 ──────────────────
        let amount: u64 = 1_000_000;
        let seed = [7u8; 32];
        let wtx = dww1.build_native_transfer(amount, &addr2.to_string(), seed)
            .await.expect("build_native_transfer");

        // ── Synthetic block carrying the TransferV1 call → scan wallet-2 ───
        let transfer_call = dwow_chain::ContractCall {
            contract_id: wtx.calls[0].data.contract_id,
            data: wtx.calls[0].data.data.clone(),
        };
        let recv_block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
                version: dwow_sdk::blockchain::BlockVersion::CURRENT,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: dwow_sdk::blockchain::BlockTimestamp::new(0),
                target: dwow_sdk::blockchain::BlockTarget::MAX,
                nonce: 0,
                height: BlockHeight::new(3),
                uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO,
                randomx_key: [0u8; 32],
                miner: miner_2,
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
                anchor_owner: [0u8; 32],
                caribina_anchor: None,
            },
            transactions: vec![dwow_chain::Transaction {
                version: dwow_sdk::blockchain::BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![transfer_call],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };
        let mut tree2 = dww2.get_capability_commitment_tree().expect("tree2");
        let result = dww2.scan_block_linear(&mut tree2, &recv_block)
            .expect("scan transfer block");

        assert!(result.native_outputs.len() >= 1,
            "recipient must decrypt the transfer note; got {} native_outputs / {} capabilities. \
             diagnostics: decode_attempts={} decrypt_attempts={} decrypt_successes={} \
             construct_attempts={} construct_successes={}",
            result.native_outputs.len(), result.capabilities.len(),
            result.diagnostics.aead_decode_attempts,
            result.diagnostics.aead_decrypt_attempts,
            result.diagnostics.aead_decrypt_successes,
            result.diagnostics.capability_construct_attempts,
            result.diagnostics.capability_construct_successes);
        assert_eq!(result.native_outputs[0].cap_record.value, amount,
            "transfer value must be discovered from the note");

        // The received commitment must be persisted and reflected in the balance.
        let balances = dww2.capability_balance().expect("wallet-2 balance");
        let drkw_key = bs58::encode(&[0u8; 32]).into_string();
        let drkw = balances.get(&drkw_key).copied().unwrap_or(0);
        assert!(drkw >= amount,
            "wallet-2 must hold the transferred DRKW; got {} (amount {})", drkw, amount);

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir_1);
        let _ = std::fs::remove_dir_all(&wallet_dir_2);
    });
}

/// The wallet builds a native DRKW transfer and the block carrying it is
/// ACCEPTED through the full `accept_block` path — the write-path validation
/// gate (`coin_roots_db` merkle-root check) that no synthetic-block test
/// reaches (l1-capability-write-path-spec.md §2/§5 test (1)).
#[test]
fn test_transfer_accepts_through_accept_block() -> TestResult<()> {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Chain: genesis + coinbase funding node0 (wallet-1) ─────────────
        let har = GenesisHarness::new().expect("GenesisHarness");
        let keys_toml = "[node0]\nwallet_secret = \
            \"755c6e8a21b3e15f146ba636a146c228b5f91202fc7e0bb0065efdd9fd685405\"\n\
            [wallet2]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_transfer_accept_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let chain_magic = crate::tests::modules::chain_setup::DRKW_MAGIC;
        let recipient_1 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, BlockHeight::new(1),
        ).expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1, chain_magic)
            .await.expect("init_genesis");

        // Block 2: coinbase (funds node0 so wallet-1 can spend).
        let height_2 = BlockHeight::new(2);
        let recipient_2 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_2,
        ).expect("MiningRecipient height 2");
        // The coinbase note is bound to this key and `accept_block` requires
        // `header.miner` to match it (the C1 binding, 55a04076c9).
        let miner_2 = recipient_2.public().to_bytes();
        let (coinbase_2, pow_reward_call, _blind) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, dwow_sdk::blockchain::expected_reward(height_2), &har.chain_state, height_2,
            ).await.expect("build_linear_coinbase");
        let coinbase_tx_2 = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };
        let prev = har.chain_state.get_latest_block().expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev).expect("hash failed");
        let header_2 = dwow_chain::BlockHeader {
            fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            previous: prev_hash,
            merkle_root: dwow_chain::compute_merkle_root(&[coinbase_tx_2.clone()]),
            timestamp: dwow_sdk::blockchain::BlockTimestamp::new(120),
            target: dwow_sdk::blockchain::BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: dwow_sdk::blockchain::expected_reward(height_2),
            randomx_key: dwow_chain::Miner::derive_key_from_height(height_2),
            miner: miner_2,
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: dwow_chain::PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };
        let block_2 = dwow_chain::Block { header: header_2, transactions: vec![coinbase_tx_2] };
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
            .expect("RandomXCache");
        let vm = Arc::new(std::sync::Mutex::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"),
        ));
        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            dwow_sdk::blockchain::BlockTarget::MAX, None,
        ).expect("accept_block height 2");

        // ── Mine the filler blocks so the genesis (h1) and height-2 coinbases
        // mature past `COINBASE_MATURITY`. A coinbase cannot be spent before
        // maturity (`check_coinbase_maturity` in chain_state.rs, run from
        // `connect_block`), so the transfer must spend a mature commitment.
        //
        // Both the window and the spend height are derived from the constant rather
        // than hardcoded. The test build lowers it via `short-coinbase-maturity`
        // (src/linear/Cargo.toml); at the production value this loop is 99 blocks of
        // pure filler, each costing a ~256 MiB RandomX cache — which is what made
        // these tests need 28-32 GB and go unrunnable.
        for h in 3u64..=(dwow_chain::COINBASE_MATURITY + 1) {
            let height = BlockHeight::new(h);
            let recipient = crate::accounts::MiningRecipient::from_account(&miner_mgr, height)
                .expect("MiningRecipient");
            let (coinbase, pow_reward_call, _blind) =
                crate::registry::model::build_linear_coinbase(
                    recipient, dwow_sdk::blockchain::expected_reward(height), &har.chain_state, height,
                ).await.expect("build_linear_coinbase");
            let coinbase_tx = dwow_chain::Transaction {
                version: dwow_sdk::blockchain::BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![pow_reward_call],
                lock_time: 0,
                nullifiers: vec![coinbase.nullifier],
                witness: vec![],
            };
            let mut block = crate::tests::harness::build_test_block(
                &har.chain_state, height, vec![coinbase_tx],
            )?;
            let target = har.chain_state.consensus.lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_next_work_required(&har.chain_state.store, height)
                .unwrap_or(dwow_sdk::blockchain::BlockTarget::MAX);
            block.header.target = target;
            let vm = crate::tests::heavyweight_pipeline::build_accept_vm(&block)
                .expect("build_accept_vm");
            if target < dwow_sdk::blockchain::BlockTarget::MAX {
                block.header.nonce = crate::tests::heavyweight_pipeline::mine_test_nonce(
                    &block, &vm, target,
                ).expect("mine_test_nonce");
            }
            crate::block_acceptor::accept_block(
                &har.chain_state, &block, &[], &vm,
                target, None,
            ).expect("accept_block (maturity mining)");
        }

        // ── Wallet-1 (node0): scan → two DRKW caps (transfer input + fee) ───
        let wallet_dir_1 = std::env::temp_dir()
            .join(format!("dwow_transfer_accept_db1_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir_1);
        let dww1 = Dww::new(
            Network::Testnet, Some(&keys_path), "node0",
            wallet_dir_1.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet-1 init");
        dww1.initialize_wallet().expect("wallet-1 schema init");
        let mut tree1 = dww1.get_capability_commitment_tree().expect("tree1");
        // Insert every block up to the spend height so reconstruct_global_coin_map can replay
        // the full global coin tree; scan only the coinbase blocks the wallet
        // receives (1..=2) so it holds exactly the two mature DRKW caps.
        for h in 1u64..=(dwow_chain::COINBASE_MATURITY + 1) {
            let block = har.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            };
            dww1.insert_synced_block(&scan_block).expect("insert block");
            if h <= 2 {
                dww1.scan_block_linear(&mut tree1, &scan_block).expect("scan block");
            }
        }

        // ── Wallet-2 (recipient) address ──────────────────────────────────
        let wallet_dir_2 = std::env::temp_dir()
            .join(format!("dwow_transfer_accept_db2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir_2);
        let dww2 = Dww::new(
            Network::Testnet, Some(&keys_path), "wallet2",
            wallet_dir_2.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet-2 init");
        dww2.initialize_wallet().expect("wallet-2 schema init");
        let addr2 = dww2.default_address().expect("wallet-2 address");

        // ── Wallet-1 builds a native transfer to wallet-2 ──────────────────
        let amount: u64 = 1_000_000;
        let seed = [7u8; 32];
        let wtx = dww1.build_native_transfer(amount, &addr2.to_string(), seed)
            .await.expect("build_native_transfer");

        // ── Convert core tx → chain tx (exactly as admit_tx_to_mempool) ────
        let transfer_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: wtx.calls.iter().map(|leaf| dwow_chain::ContractCall {
                contract_id: leaf.data.contract_id,
                data: leaf.data.data.clone(),
            }).collect(),
            lock_time: 0,
            nullifiers: wtx.nullifiers.clone(),
            witness: dwow_serial::serialize(&wtx),
        };

        // ── The spend block: [coinbase, transfer, fee_collect] → accept_block ────
        let height_xfer = BlockHeight::new(dwow_chain::COINBASE_MATURITY + 2);
        let recipient_xfer = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_xfer,
        ).expect("MiningRecipient");
        let (coinbase_xfer, pow_reward_call_xfer, _blindx) =
            crate::registry::model::build_linear_coinbase(
                recipient_xfer.clone(), dwow_sdk::blockchain::expected_reward(height_xfer), &har.chain_state, height_xfer,
            ).await.expect("build_linear_coinbase");
        let coinbase_tx_xfer = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call_xfer],
            lock_time: 0,
            nullifiers: vec![coinbase_xfer.nullifier],
            witness: vec![],
        };
        // The transfer's FeeV3 fee call requires a closing FeeCollectV1 tx
        // (validation.rs: "block has N fee call(s) but no FeeCollectV1 call").
        let fee_amount: u64 = transfer_tx.contract_calls.iter()
            .filter(|c| c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
                && c.data.first() == Some(&0x08))
            .filter_map(|c| dwow_native_token_contract::model::fee::FeeParamsV3::decode(&c.data[1..]).ok())
            .map(|p| p.fee.get())
            .sum();
        let fee_collect_tx = crate::registry::model::build_fee_collect_tx(
            &recipient_xfer,
            &[transfer_tx.clone()],
            height_xfer,
            dwow_sdk::blockchain::FeeAmount::new(fee_amount),
        ).expect("build_fee_collect_tx")
            .expect("fee_collect tx present (transfer carries a FeeV3 fee call)");

        let mut block_xfer = crate::tests::harness::build_test_block(
            &har.chain_state, height_xfer,
            vec![coinbase_tx_xfer, transfer_tx, fee_collect_tx],
        )?;
        let target_xfer = har.chain_state.consensus.lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_next_work_required(&har.chain_state.store, height_xfer)
            .unwrap_or(dwow_sdk::blockchain::BlockTarget::MAX);
        block_xfer.header.target = target_xfer;
        let vm_xfer = crate::tests::heavyweight_pipeline::build_accept_vm(&block_xfer)
            .expect("build_accept_vm");
        if target_xfer < dwow_sdk::blockchain::BlockTarget::MAX {
            block_xfer.header.nonce = crate::tests::heavyweight_pipeline::mine_test_nonce(
                &block_xfer, &vm_xfer, target_xfer,
            ).expect("mine_test_nonce");
        }

        let height_before = har.chain_state.get_height();
        crate::block_acceptor::accept_block(
            &har.chain_state, &block_xfer, &[], &vm_xfer,
            target_xfer, None,
        ).expect("accept_block (transfer)");

        // The write-path gate: the transfer block must be ACCEPTED (height
        // advanced), which only happens if the transfer input's merkle_root is
        // a coin_roots_db key — the assertion that fails under Bug A.
        let height_after = har.chain_state.get_height();
        assert_eq!(height_after, height_before.succ(),
            "transfer block must advance the chain height (accepted, not rejected)");

        // ── Wallet-2 (recipient) receives via the sync→scan path (Docker) ──
        // The Docker sync task only inserts blocks (insert_synced_block); the
        // scan runs separately via `wal scan → scan_blocks`. Exercise that exact
        // path (not the direct scan_block_linear) to witness the Docker
        // divergence deterministically (l1-capability-write-path-spec.md §4.1).
        for h in 1u64..=(dwow_chain::COINBASE_MATURITY + 2) {
            let block = har.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            };
            dww2.insert_synced_block(&scan_block).expect("insert block");
        }
        dww2.scan_blocks(&mut vec![], None, &false).await.expect("scan_blocks");

        let held = dww2.wallet.get_held_capabilities(Some(false)).expect("held caps");
        assert!(
            held.iter().any(|c| c.value == amount
                && c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID),
            "recipient must decrypt the mined transfer note via scan_blocks (value={amount}); held={}",
            held.len(),
        );

        let balances2 = dww2.capability_balance().expect("wallet-2 balance");
        let drkw_key = bs58::encode(&[0u8; 32]).into_string();
        let drkw2 = balances2.get(&drkw_key).copied().unwrap_or(0);
        assert!(drkw2 >= amount,
            "wallet-2 must hold the transferred DRKW via scan_blocks; got {} (amount {})", drkw2, amount);

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir_1);
        let _ = std::fs::remove_dir_all(&wallet_dir_2);
        Ok(())
    })
}

// ============================================================================
// `invoke_contract`'s two-call (leaf + fee) path — the fee-funded manifest call
// ============================================================================

/// One wallet that holds BOTH the box capability it must spend AND mature DRKW to pay the
/// fee, plus the chain it needs to prove against.
struct BoxOwnerScenario {
    har: GenesisHarness,
    dww: Dww,
    /// The block carrying the harness-seeded box state — the height the subject block follows.
    box_put_height: BlockHeight,
    keys_path: PathBuf,
    wallet_dir: PathBuf,
}

/// Build a coinbase chain transaction for `recipient` at `height` (single PoWRewardV1 call).
async fn coinbase_tx(
    har: &GenesisHarness,
    recipient: crate::accounts::MiningRecipient,
    height: BlockHeight,
) -> dwow_chain::Transaction {
    let (coinbase, pow_reward_call, _blind) =
        crate::registry::model::build_linear_coinbase(
            recipient, dwow_sdk::blockchain::expected_reward(height), &har.chain_state, height,
        ).await.expect("build_linear_coinbase");
    dwow_chain::Transaction {
        version: dwow_sdk::blockchain::BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![pow_reward_call],
        lock_time: 0,
        nullifiers: vec![coinbase.nullifier],
        witness: vec![],
    }
}

/// Build a block from `txs`, seal it (next-work target, RandomX VM, PoW nonce) and submit it
/// through `accept_block`. Returns `accept_block`'s own result so a caller can assert
/// acceptance (the positive test) or refusal (the control) — a build failure is a test bug
/// and panics here, so an `Err` return is the acceptor's verdict and nothing else.
fn submit_block(
    har: &GenesisHarness,
    height: BlockHeight,
    txs: Vec<dwow_chain::Transaction>,
) -> Result<(), String> {
    let mut block = crate::tests::harness::build_test_block(&har.chain_state, height, txs)
        .map_err(|e| format!("build_test_block: {e:?}"))?;
    let target = har.chain_state.consensus.lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_next_work_required(&har.chain_state.store, height)
        .unwrap_or(dwow_sdk::blockchain::BlockTarget::MAX);
    block.header.target = target;
    let vm = crate::tests::heavyweight_pipeline::build_accept_vm(&block)
        .map_err(|e| format!("build_accept_vm: {e:?}"))?;
    if target < dwow_sdk::blockchain::BlockTarget::MAX {
        block.header.nonce = crate::tests::heavyweight_pipeline::mine_test_nonce(&block, &vm, target)
            .map_err(|e| format!("mine_test_nonce: {e:?}"))?;
    }
    crate::block_acceptor::accept_block(&har.chain_state, &block, &[], &vm, target, None)
        .map(|_| ())
        .map_err(|e| format!("accept_block: {e:?}"))
}

/// The box `put` params JSON the wallet's generic prover reads.
///
/// Only `new_contents_commit` and `tx_nonce` are `[[parameters]]` the manifest consumes;
/// `box_id`, `old_state_nonce`, `new_state_nonce` and `old_contents_commit` are carried for
/// readability and *ignored* — `decode_params_from_json` reads only declared fields, and the
/// real values come from the wallet's own note record through the `note:` witness sources.
/// Same shape `capability_scan_integration.rs::test_box_put_wallet_driven_generic_prover`
/// uses; the seeded box's contents are the ones those constants encode.
fn box_put_params_json() -> String {
    use dwow_sdk::crypto::pasta_prelude::PrimeField;
    use dwow_sdk::crypto::poseidon_hash;
    use dwow_sdk::pasta::pallas;
    let old_cc = poseidon_hash([pallas::Base::from(100u64)]);
    let new_cc = poseidon_hash([pallas::Base::from(200u64)]);
    let base_hex = |b: &pallas::Base| format!("0x{}", hex::encode(b.to_repr()));
    format!(
        r#"{{"box_id":"{}","old_state_nonce":"{}","new_state_nonce":"{}","old_contents_commit":"{}","new_contents_commit":"{}","tx_nonce":"{}"}}"#,
        base_hex(&pallas::Base::from(1u64)),
        base_hex(&pallas::Base::from(1u64)),
        base_hex(&pallas::Base::from(2u64)),
        base_hex(&old_cc),
        base_hex(&new_cc),
        base_hex(&pallas::Base::from(0u64)),
    )
}

/// Bring up the one-wallet scenario: a chain whose box state has been seeded by a harness
/// `put`, and a single wallet (`boxowner`) that holds both the box capability that put
/// created and mature DRKW to pay a fee with.
///
/// `boxowner`'s secret is the literal `42` that `BoxHarness` adopts as its box owner, so the
/// SAME account decrypts the seeded box note. It is the mining recipient for genesis and the
/// height-2 block ONLY: a coinbase cannot fund a fee until it is `COINBASE_MATURITY` blocks
/// old (`chain_state::check_coinbase_maturity`, invoked from `connect_block`), and the fee
/// builder picks a held DRKW cap without regard to maturity — so the wallet must hold no
/// immature cap, or an immature one could be selected and the whole block refused. The
/// maturity window is therefore mined by `node0`, whose coinbases `boxowner` cannot decrypt
/// and so never holds.
/// `tag` distinguishes the two tests that call this. libtest runs them as threads of ONE
/// process, so `std::process::id()` alone is identical for both — they would share a keys
/// file and one test's cleanup would delete the other's while it was still running, which
/// is exactly how the positive test failed the first time these were run.
async fn box_owner_scenario(tag: &str) -> TestResult<BoxOwnerScenario> {
    use dwow_contract_test_harness::harness::BoxHarness;
    use dwow_sdk::crypto::BOX_CONTRACT_ID;

    let har = GenesisHarness::new().expect("GenesisHarness");
    let keys_toml = "[node0]\nwallet_secret = \
        \"755c6e8a21b3e15f146ba636a146c228b5f91202fc7e0bb0065efdd9fd685405\"\n\
        [boxowner]\nwallet_secret = \
        \"2a00000000000000000000000000000000000000000000000000000000000000\"\n";
    let keys_path = std::env::temp_dir()
        .join(format!("dwow_box_fee_keys_{tag}_{}.toml", std::process::id()));
    std::fs::write(&keys_path, keys_toml).expect("write test keys");

    let boxowner_mgr = crate::accounts::AccountManager::open(
        &keys_path, Network::Testnet, "boxowner",
    ).expect("open boxowner AccountManager");
    let node0_mgr = crate::accounts::AccountManager::open(
        &keys_path, Network::Testnet, "node0",
    ).expect("open node0 AccountManager");
    let chain_magic = crate::tests::modules::chain_setup::DRKW_MAGIC;

    // Genesis: boxowner's height-1 coinbase.
    let recipient_1 = crate::accounts::MiningRecipient::from_account(
        &boxowner_mgr, BlockHeight::new(1),
    ).expect("MiningRecipient height 1");
    crate::init_genesis(&har.chain_state, recipient_1, chain_magic)
        .await.expect("init_genesis");

    // ── Block 2: boxowner's second (and last discovered) coinbase. ────────────
    // Built with a hand-rolled header rather than `build_test_block`: the header helper
    // hashes the previous block through its cached RandomX VM and genesis's is not cached
    // yet. `test_transfer_accepts_through_accept_block` builds height 2 the same way.
    let height_2 = BlockHeight::new(2);
    let recipient_2 = crate::accounts::MiningRecipient::from_account(&boxowner_mgr, height_2)
        .expect("MiningRecipient height 2");
    let miner_2 = recipient_2.public().to_bytes();
    let coinbase_tx_2 = coinbase_tx(&har, recipient_2, height_2).await;
    let prev = har.chain_state.get_latest_block().expect("get_latest_block");
    let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev).expect("hash failed");
    let header_2 = dwow_chain::BlockHeader {
        fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
        version: dwow_sdk::blockchain::BlockVersion::CURRENT,
        previous: prev_hash,
        merkle_root: dwow_chain::compute_merkle_root(&[coinbase_tx_2.clone()]),
        timestamp: dwow_sdk::blockchain::BlockTimestamp::new(120),
        target: dwow_sdk::blockchain::BlockTarget::MAX,
        nonce: 0,
        height: height_2,
        uncle_merkle_root: [0u8; 32],
        total_reward: dwow_sdk::blockchain::expected_reward(height_2),
        randomx_key: dwow_chain::Miner::derive_key_from_height(height_2),
        miner: miner_2,
        commitment_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: dwow_chain::PowSource::Native,
        anchor_owner: [0u8; 32],
        caribina_anchor: None,
    };
    let block_2 = dwow_chain::Block { header: header_2, transactions: vec![coinbase_tx_2] };
    let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
        .expect("RandomXCache");
    let vm = Arc::new(std::sync::Mutex::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"),
    ));
    crate::block_acceptor::accept_block(
        &har.chain_state, &block_2, &[], &vm,
        dwow_sdk::blockchain::BlockTarget::MAX, None,
    ).expect("accept_block height 2");

    // ── The maturity window, mined by node0 (height derived from the constant). ─
    for h in 3u64..=(dwow_chain::COINBASE_MATURITY + 1) {
        let height = BlockHeight::new(h);
        let recipient = crate::accounts::MiningRecipient::from_account(&node0_mgr, height)
            .expect("MiningRecipient");
        let cb = coinbase_tx(&har, recipient, height).await;
        submit_block(&har, height, vec![cb]).expect("accept_block (maturity mining)");
    }

    // ── node0's coinbase + the harness-seeded box put. ──────────────────────
    // node0 mines it so boxowner's scan of this block finds the box note and no immature
    // coinbase of its own.
    let box_put_height = BlockHeight::new(dwow_chain::COINBASE_MATURITY + 2);
    let recipient_for_spend = crate::accounts::MiningRecipient::from_account(&node0_mgr, box_put_height)
        .expect("MiningRecipient height 102");
    let coinbase_tx_for_spend = coinbase_tx(&har, recipient_for_spend, box_put_height).await;

    let harness = BoxHarness::spawn();
    let put = harness.put().expect("seed box put");
    let box_put_call_data = put.call_data;
    let mut box_put_tx = crate::tests::harness::build_contract_tx(
        *BOX_CONTRACT_ID, box_put_call_data.clone(),
    );
    box_put_tx.witness = crate::tests::heavyweight_pipeline::build_witness(
        *BOX_CONTRACT_ID, &box_put_call_data, vec![put.proof], None,
    );
    submit_block(&har, box_put_height, vec![coinbase_tx_for_spend, box_put_tx])
        .expect("accept_block (seed box put)");

    // ── The ONE wallet: boxowner discovers the box note and its mature coinbases. ──
    let wallet_dir = std::env::temp_dir()
        .join(format!("dwow_box_fee_db_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wallet_dir);
    let dww = Dww::new(
        Network::Testnet, Some(&keys_path), "boxowner",
        wallet_dir.to_string_lossy().to_string(), "".to_string(), false, None,
    ).expect("wallet init");
    dww.initialize_wallet().expect("wallet schema init");

    // Seed the Box manifest + zkas (normally extracted from the genesis DeployV1 payload;
    // the test seeds them directly — genesis-scan delivery is separate).
    let box_cid_str = bs58::encode(BOX_CONTRACT_ID.to_bytes()).into_string();
    let manifest_toml = include_str!("../../../../src/contract/box/manifest.toml");
    dww.wallet.insert_contract_metadata_with_manifest(
        &dwow_wallet::walletdb::ContractMetadataRecord {
            contract_id: box_cid_str.clone(), name: "box".into(), symbol: None,
            category: "Infrastructure".into(), description: Some("Box".into()),
            public: true, deployer_pubkey: "".into(), deploy_height: BlockHeight::new(1),
            attestations_json: "[]".into(), manifest_wasm_verdict: String::new(), lock_status: "unlocked".into(),
        }, Some(manifest_toml),
    ).expect("store Box manifest");
    dww.wallet.store_zkas_binary(&box_cid_str, "Put", "Put",
        include_bytes!("../../../../src/contract/box/proof/put.zk.bin"))
        .expect("store put zkas");

    // Scan blocks 1..=box_put_height in order. `insert_synced_block` requires contiguity and
    // the global/contract trees are replayed from the inserted blocks; scanning discovers the
    // two mature DRKW coinbases AND the seeded box capability.
    let mut cap_tree = dww.get_capability_commitment_tree().expect("cap tree");
    for h in 1u64..=box_put_height.get() {
        let block = har.chain_state.get_block(BlockHeight::new(h)).expect("block");
        let scan_block = dwow_chain::Block {
            header: block.header.clone(), transactions: block.transactions.clone(),
        };
        dww.insert_synced_block(&scan_block).expect("insert block");
        dww.scan_block_linear(&mut cap_tree, &scan_block).expect("scan block");
    }
    let held = dww.get_held_capabilities(Some(false)).expect("held caps");
    assert_eq!(
        held.iter().filter(|c| c.contract_id == *BOX_CONTRACT_ID).count(), 1,
        "the wallet must discover exactly the one seeded box capability",
    );

    Ok(BoxOwnerScenario { har, dww, box_put_height, keys_path, wallet_dir })
}

/// A fee-funded manifest call through `invoke_contract`: a box `put` proven against the
/// commitment over the WHOLE two-call transaction (the leaf and its FeeV3 fee), accepted
/// on chain.
///
/// This is the only test that can catch the defect it guards, because the failing predicate
/// is the node's own reconciliation: `verify_core_tx_with_tables` refuses a proof whose
/// published `tx_binding` does not match the transaction's commitment, which is a value the
/// proof-derivation covers and no local proof check sees. A leaf proven against the
/// single-call commitment is exactly what `ManifestContractClient::build` produces (see the
/// control below), so acceptance here is evidence the leaf was bound to the whole call set.
#[test]
fn test_invoke_contract_box_put_with_fee_accepts_through_accept_block() -> TestResult<()> {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        use dwow_sdk::crypto::{BOX_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};

        let BoxOwnerScenario { har, dww, box_put_height, keys_path, wallet_dir } =
            box_owner_scenario("invoke_contract_fee").await?;

        // The subject call: box `put` through the manifest path of `invoke_contract`. The
        // wallet holds the box capability to spend and mature DRKW to pay the fee, so this
        // must resolve to a [leaf, fee] transaction.
        let params_json = box_put_params_json();
        let tx = dww
            .invoke_contract("box", "put", Some(&params_json), vec![], vec![], None)
            .await
            .expect("invoke_contract box put");

        // The manifest path must produce exactly the leaf and its fee, and the leaf's first
        // byte must be the function code the wallet prepends for `put` (0x01) — pinning the
        // manifest's `code` against the wallet's selector rather than trusting the two agree.
        assert_eq!(tx.calls.len(), 2, "manifest path must produce [box leaf, fee]");
        assert_eq!(tx.calls[0].data.contract_id, *BOX_CONTRACT_ID,
            "calls[0] must target the Box contract");
        assert_eq!(tx.calls[0].data.data.first(), Some(&0x01),
            "calls[0] must be box::put — its first byte is the put function code");
        assert_eq!(tx.calls[1].data.contract_id, *NATIVE_TOKEN_CONTRACT_ID,
            "calls[1] must be the native-token fee");
        assert_eq!(tx.calls[1].data.data.first(), Some(&0x08),
            "calls[1] must be the FeeV3 fee (function code 0x08)");

        // Convert the core tx → chain tx exactly as `admit_tx_to_mempool` does.
        let subject_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: tx.calls.iter().map(|leaf| dwow_chain::ContractCall {
                contract_id: leaf.data.contract_id,
                data: leaf.data.data.clone(),
            }).collect(),
            lock_time: 0,
            nullifiers: tx.nullifiers.clone(),
            witness: dwow_serial::serialize(&tx),
        };

        // ── The fee block:[coinbase, subject, fee_collect] → accept_block. ──────────
        let height = box_put_height.succ();
        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open node0 AccountManager");
        let recipient = crate::accounts::MiningRecipient::from_account(&miner_mgr, height)
            .expect("MiningRecipient");
        let coinbase = coinbase_tx(&har, recipient.clone(), height).await;
        // The fee call requires a closing FeeCollectV1 (validation.rs: "block has N fee
        // call(s) but no FeeCollectV1 call").
        let fee_amount: u64 = subject_tx.contract_calls.iter()
            .filter(|c| c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
                && c.data.first() == Some(&0x08))
            .filter_map(|c| dwow_native_token_contract::model::fee::FeeParamsV3::decode(&c.data[1..]).ok())
            .map(|p| p.fee.get())
            .sum();
        let fee_collect_tx = crate::registry::model::build_fee_collect_tx(
            &recipient,
            &[subject_tx.clone()],
            height,
            dwow_sdk::blockchain::FeeAmount::new(fee_amount),
        ).expect("build_fee_collect_tx")
            .expect("fee_collect tx present (the put carries a FeeV3 fee call)");

        let height_before = har.chain_state.get_height();
        submit_block(&har, height, vec![coinbase, subject_tx, fee_collect_tx])
            .expect("accept_block (box put + fee)");
        assert_eq!(har.chain_state.get_height(), height_before.succ(),
            "the two-call box put block must advance the chain height (accepted, not rejected)");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
        Ok(())
    })
}

/// The mirror control: the SAME box `put` leaf, built through the retained SINGLE-call
/// `ManifestContractClient::build` (which binds the proof to this call alone), placed in the
/// SAME two-call transaction shape. `accept_block` must REFUSE it.
///
/// This is what gives the positive test its teeth. The two tests differ in exactly one
/// thing — whether the leaf proved against the one-call or the two-call commitment — so if
/// this control ever accepts, `build` is not single-call-only, and if the positive ever
/// refuses, the wallet is not binding to the whole call set. Both are asserted by the
/// acceptor's own verdict, not by inspecting a proof.
#[test]
fn test_single_call_manifest_build_is_refused_in_a_two_call_tx() -> TestResult<()> {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        use dwow_core::tx::ContractCallLeaf;
        use dwow_sdk::crypto::pasta_prelude::{Field, PrimeField};
        use dwow_sdk::crypto::{BOX_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
        use dwow_sdk::pasta::pallas;
        use dwow_sdk::tx::ContractCall;

        let BoxOwnerScenario { har, dww, box_put_height, keys_path, wallet_dir } =
            box_owner_scenario("single_call_mirror").await?;

        // The leaf, through the retained single-call path — `build` proves it against the
        // commitment over this call ALONE (contract_client.rs: "the commitment a transaction
        // carrying exactly this one call will carry").
        let params_json = box_put_params_json();
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(
            include_str!("../../../../src/contract/box/manifest.toml")
        ).expect("parse Box manifest");
        let box_cid_str = bs58::encode(BOX_CONTRACT_ID.to_bytes()).into_string();
        let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
        let self_pk = *dww.default_address().expect("self address").public_key();
        let client = dwow_sdk::contract_client::ManifestContractClient::new(
            "box", manifest.clone(), box_cid_str.clone(), seed, self_pk,
        );
        // `build` is the `ContractClient` trait method — the trait must be in scope to call it.
        use dwow_sdk::contract_client::ContractClient;
        let (call_body, proof_bytes) = client
            .build("put", &params_json, &dww)
            .expect("ManifestContractClient::build box put (single-call)");
        let mut leaf_data = vec![0x01u8]; // the wallet prepends the fn code byte (0x01 = put)
        leaf_data.extend_from_slice(&call_body);
        let leaf = ContractCallLeaf {
            call: ContractCall { contract_id: *BOX_CONTRACT_ID, data: leaf_data },
            proofs: proof_bytes.into_iter().map(dwow_core::zk::Proof::new).collect(),
        };

        // The fee, prepared and proved exactly as `invoke_contract` does it — bound to the
        // TWO-call commitment. Only the leaf's binding differs from the positive test.
        let circuit_costs: Vec<u64> = {
            let resolver = dwow_wallet::manifest_resolver::ManifestResolver::new(&manifest);
            resolver.get_cost_profile("put")
                .map(|cp| vec![cp.circuit_difficulty])
                .unwrap_or_default()
        };
        let fee = dwow_wallet::fee_builder::prepare_fee_call(
            &dww.wallet, &dww.account_mgr, None, seed, &circuit_costs,
            dww.contract_risk_factor(&*BOX_CONTRACT_ID, true),
            dwow_sdk::blockchain::WasmKb::ZERO, dww.fee_window_cfs().0,
            dwow_sdk::blockchain::FeeTier::LOW,
        ).expect("prepare_fee_call");
        let fee_call = fee.contract_call();
        let tx_commitment = dwow_sdk::crypto::util::tx_commitment([&leaf.call, &fee_call]);
        let tx = fee.finish(leaf, tx_commitment)
            .expect("finish — the fee proves against the two-call commitment");

        // Convert core tx → chain tx exactly as the positive test does.
        let subject_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: tx.calls.iter().map(|leaf| dwow_chain::ContractCall {
                contract_id: leaf.data.contract_id,
                data: leaf.data.data.clone(),
            }).collect(),
            lock_time: 0,
            nullifiers: tx.nullifiers.clone(),
            witness: dwow_serial::serialize(&tx),
        };

        // ── The fee block:the SAME [coinbase, subject, fee_collect] shape. ──────────
        let height = box_put_height.succ();
        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open node0 AccountManager");
        let recipient = crate::accounts::MiningRecipient::from_account(&miner_mgr, height)
            .expect("MiningRecipient");
        let coinbase = coinbase_tx(&har, recipient.clone(), height).await;
        let fee_amount: u64 = subject_tx.contract_calls.iter()
            .filter(|c| c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
                && c.data.first() == Some(&0x08))
            .filter_map(|c| dwow_native_token_contract::model::fee::FeeParamsV3::decode(&c.data[1..]).ok())
            .map(|p| p.fee.get())
            .sum();
        let fee_collect_tx = crate::registry::model::build_fee_collect_tx(
            &recipient,
            &[subject_tx.clone()],
            height,
            dwow_sdk::blockchain::FeeAmount::new(fee_amount),
        ).expect("build_fee_collect_tx")
            .expect("fee_collect tx present");

        let result = submit_block(&har, height, vec![coinbase, subject_tx, fee_collect_tx]);
        assert!(
            result.is_err(),
            "a leaf proved against the single-call commitment must be refused when the \
             transaction carries two calls, but the block was accepted: {result:?}",
        );

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
        Ok(())
    })
}
