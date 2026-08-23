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

use std::sync::Arc;

use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::crypto::keypair::{Address, Network};
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
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n\
            [wallet2]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_transfer_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let chain_magic = [0xDA, 0x57, 0x01, 0x57];

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
        let linear_zk = crate::registry::model::LinearPowRewardZk::new(
            har.chain_state.clone(),
        ).await.expect("LinearPowRewardZk");
        let (coinbase_2, _pi, pow_reward_call, _blind) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, dwow_sdk::blockchain::expected_reward(height_2), &linear_zk, height_2,
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
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: dwow_chain::PowSource::Native,
        };
        let block_2 = dwow_chain::Block { header: header_2, transactions: vec![coinbase_tx_2] };
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
            .expect("RandomXCache");
        let vm = Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"),
        );
        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            BlockHeight::new(1), dwow_sdk::blockchain::BlockTarget::MAX, None,
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
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n\
            [wallet2]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_transfer_recv_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let chain_magic = [0xDA, 0x57, 0x01, 0x57];
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
        let linear_zk = crate::registry::model::LinearPowRewardZk::new(
            har.chain_state.clone(),
        ).await.expect("LinearPowRewardZk");
        let (coinbase_2, _pi, pow_reward_call, _blind) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, dwow_sdk::blockchain::expected_reward(height_2), &linear_zk, height_2,
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
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: dwow_chain::PowSource::Native,
        };
        let block_2 = dwow_chain::Block { header: header_2, transactions: vec![coinbase_tx_2] };
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
            .expect("RandomXCache");
        let vm = Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"),
        );
        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            BlockHeight::new(1), dwow_sdk::blockchain::BlockTarget::MAX, None,
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
            dww1.scan_block_linear(&mut tree1, &dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            }).expect("scan block");
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
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
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

        // The received coin must be persisted and reflected in the balance.
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
