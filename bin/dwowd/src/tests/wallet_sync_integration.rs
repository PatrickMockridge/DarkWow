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

//! Wallet sync integration test — the production pull path end-to-end, in-process.
//!
//! Closes the gap that Docker was covering: a real serving node (P2P +
//! `LinearSyncHandler`) feeds a real wallet (`Dww`) over in-process loopback P2P,
//! the wallet's `run_wallet_sync` task pulls GetTip/GetBlocks, and the wallet
//! scans + decrypts the coinbase to reach a non-zero DRKW balance.
//!
//! This exercises peer discovery (ManualSession dial) → GetTip/GetBlocks →
//! block insert → scan → balance — the pipeline success criterion — without
//! Docker. Run with:
//!   RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 \
//!     cargo test --release -p dwowd --lib -- wallet_sync_integration

use std::sync::Arc;
use std::time::{Duration, Instant};

use url::Url;

use dwow_core::net::settings::{MagicBytes, NetworkProfile, Settings};
use dwow_core::net::P2p;
use dwow_sdk::blockchain::BlockHeight;
use dwow_wallet::sync_task::{run_wallet_sync, HighestPeerTip};
use dwow_wallet::Dww;

use crate::tests::genesis::GenesisHarness;
use crate::Network;

/// Real DarkWow testnet P2P magic bytes ("DRKW") — the value the mining node
/// and wallet both derive from their network config (dwowd_config.toml /
/// dww_config.toml `[net] magic_bytes`). Using the real value (not a bespoke
/// test constant) is what makes the cross-rail test exercise the production
/// handshake path.
const DRKW_MAGIC: [u8; 4] = [68, 82, 75, 87];

/// Pick an ephemeral loopback TCP port (same trick as src/net/tests.rs).
fn get_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

/// Minimal plain-tcp P2P settings for an in-process loopback node.
fn loopback_settings(
    inbound: Option<u16>,
    peers: Vec<Url>,
    magic: [u8; 4],
) -> Settings {
    let mut profiles = std::collections::HashMap::new();
    profiles.insert(
        "tcp+tls".to_string(),
        NetworkProfile { outbound_connect_timeout: 2, ..Default::default() },
    );
    let inbound_addrs = inbound
        .map(|p| vec![Url::parse(&format!("tcp+tls://127.0.0.1:{p}")).unwrap()])
        .unwrap_or_default();
    Settings {
        localnet: true,
        // Loopback test overlay: hostlist gates key on p2p_local (not localnet)
        // so 127.0.0.1 addrs are not dropped by filter_addresses.
        p2p_local: true,
        inbound_addrs: inbound_addrs.clone(),
        external_addrs: inbound_addrs,
        peers,
        seeds: vec![],
        inbound_connections: if inbound.is_some() { usize::MAX } else { 0 },
        active_profiles: vec!["tcp+tls".to_string()],
        magic_bytes: MagicBytes(magic),
        profiles,
        ..Default::default()
    }
}

/// Wallet sync pulls real coinbase blocks over in-process P2P and reaches balance.
#[test]
fn test_wallet_sync_pulls_blocks_to_balance() {
    use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction, compute_merkle_root};
    use dwow_sdk::blockchain::expected_reward;

    dwow_native_token_contract::enable_deterministic_zk();

    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .try_init();

    smol::block_on(async {
        // ── Setup: harness + miner keys ─────────────────────────────────────
        let har = GenesisHarness::new().expect("GenesisHarness");

        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_wallet_sync_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let chain_magic = [0xDA, 0x57, 0x01, 0x57];

        // ── Block 1: genesis (production path) ──────────────────────────────
        let recipient_1 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, BlockHeight::new(1),
        ).expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1, chain_magic)
            .await.expect("init_genesis");

        // ── Block 2: post-genesis coinbase ──────────────────────────────────
        let height_2 = BlockHeight::new(2);
        let reward_2 = expected_reward(height_2);
        let recipient_2 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_2,
        ).expect("MiningRecipient height 2");

        let linear_zk = crate::registry::model::LinearPowRewardZk::new(
            har.chain_state.clone(),
        ).await.expect("LinearPowRewardZk");

        let (coinbase_2, _pi_2, pow_reward_call_2, _blind_2) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, reward_2, &linear_zk, height_2,
            ).await.expect("build_linear_coinbase height 2");

        let coinbase_tx_2 = Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call_2],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };

        let prev = har.chain_state.get_latest_block().expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev).expect("hash failed");

        let header_2 = BlockHeader {
            fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            previous: prev_hash,
            merkle_root: compute_merkle_root(&[coinbase_tx_2.clone()]),
            timestamp: dwow_sdk::blockchain::BlockTimestamp::new(120),
            target: dwow_sdk::blockchain::BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward_2,
            randomx_key: Miner::derive_key_from_height(height_2),
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Native,
        };
        let block_2 = Block { header: header_2, transactions: vec![coinbase_tx_2] };

        let rx_flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
            .expect("RandomXCache height 2");
        let vm = Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM height 2"),
        );
        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            BlockHeight::new(1), dwow_sdk::blockchain::BlockTarget::MAX, None,
        ).expect("accept_block height 2");

        // ── Executor (runs P2P + sync task) ─────────────────────────────────
        let ex: Arc<smol::Executor<'static>> = Arc::new(smol::Executor::new());
        let (signal, shutdown) = smol::channel::unbounded::<()>();
        let ex_thread = {
            let ex = ex.clone();
            std::thread::spawn(move || {
                let _ = smol::future::block_on(ex.run(shutdown.recv()));
            })
        };

        let p2p_magic = DRKW_MAGIC;

        // ── Serving node: P2P + LinearSyncHandler ───────────────────────────
        let serving_port = get_free_port();
        let serving_settings = loopback_settings(Some(serving_port), vec![], p2p_magic);
        let serving_p2p = P2p::new(serving_settings, ex.clone()).await
            .expect("serving P2p::new");
        let linear_sync = crate::proto::LinearSyncHandler::init(
            &serving_p2p, har.chain_state.clone(),
        ).await;
        serving_p2p.clone().start().await.expect("serving P2p::start");
        linear_sync.start(&ex).await.expect("LinearSyncHandler::start");

        // ── Wallet node: Dww + P2P (peers = serving node) ───────────────────
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_wallet_sync_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);
        let mut dww = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "node0",
            wallet_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet initialize");
        dww.initialize_wallet().expect("wallet schema init");

        let serving_url = Url::parse(&format!("tcp+tls://127.0.0.1:{serving_port}")).unwrap();
        let wallet_settings = loopback_settings(None, vec![serving_url], p2p_magic);
        let wallet_p2p = P2p::new(wallet_settings, ex.clone()).await
            .expect("wallet P2p::new");
        wallet_p2p.clone().start().await.expect("wallet P2p::start");
        dww.p2p = Some(wallet_p2p.clone());

        let dww_ptr = dww.into_ptr();

        // ── Drive the real sync task ────────────────────────────────────────
        let highest_peer_tip = Arc::new(HighestPeerTip::new());
        let sync_dww = dww_ptr.clone();
        let sync_tip = highest_peer_tip.clone();
        let sync_p2p = wallet_p2p.clone();
        ex.spawn(async move {
            if let Err(e) = run_wallet_sync(sync_p2p, sync_dww, sync_tip).await {
                eprintln!("[wallet_sync_integration] run_wallet_sync ended: {e}");
            }
        }).detach();

        // ── Poll until the wallet has pulled both blocks ────────────────────
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let local = {
                let dww_r = dww_ptr.read().await;
                dww_r.wallet.chain_height().unwrap_or(BlockHeight::new(0))
            };
            if local >= BlockHeight::new(2) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "wallet sync timed out: still at height {} after 60s (peer_tip={})",
                local.get(), highest_peer_tip.get().get(),
            );
            smol::Timer::after(Duration::from_millis(500)).await;
        }

        // ── Scan + assert the pipeline success criterion (DRKW balance) ─────
        let mut output = vec![];
        {
            let dww_r = dww_ptr.read().await;
            dww_r.scan_blocks(&mut output, None, &false).await
                .expect("wallet scan after sync");
        }
        let balances = {
            let dww_r = dww_ptr.read().await;
            dww_r.capability_balance().expect("capability balance")
        };
        let drkw_key = bs58::encode(&[0u8; 32]).into_string();
        let drkw = balances.get(&drkw_key).copied().unwrap_or(0);
        assert!(
            drkw > 0,
            "wallet must have non-zero DRKW balance after sync+scan, got {} (key={})",
            drkw, drkw_key,
        );

        // ── Cleanup ─────────────────────────────────────────────────────────
        drop(miner_mgr);
        drop(signal);
        ex_thread.join().expect("executor thread");
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

/// Magic-bytes mismatch keeps the wallet at peers=0 — documents the exact
/// failure mode the pipeline hit (`[sync] Tick: local=0 peers=0`). A wallet
/// on the wrong network SHALL NOT connect to a DRKW node.
#[test]
fn test_wallet_sync_magic_mismatch_stays_at_zero() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .try_init();

    smol::block_on(async {
        let ex: Arc<smol::Executor<'static>> = Arc::new(smol::Executor::new());
        let (signal, shutdown) = smol::channel::unbounded::<()>();
        let ex_thread = {
            let ex = ex.clone();
            std::thread::spawn(move || {
                let _ = smol::future::block_on(ex.run(shutdown.recv()));
            })
        };

        // Serving node: inbound P2P with the real DRKW magic.
        let serving_port = get_free_port();
        let serving_settings = loopback_settings(Some(serving_port), vec![], DRKW_MAGIC);
        let serving_p2p = P2p::new(serving_settings, ex.clone()).await
            .expect("serving P2p::new");
        serving_p2p.clone().start().await.expect("serving P2p::start");

        // Wallet: outbound P2P dialing the serving node with the WRONG magic.
        let wrong_magic = [0x00, 0x00, 0x00, 0x00];
        let serving_url = Url::parse(&format!("tcp+tls://127.0.0.1:{serving_port}")).unwrap();
        let wallet_settings = loopback_settings(None, vec![serving_url], wrong_magic);
        let wallet_p2p = P2p::new(wallet_settings, ex.clone()).await
            .expect("wallet P2p::new");
        wallet_p2p.clone().start().await.expect("wallet P2p::start");

        // Give the dial a chance to be attempted and rejected at the magic check.
        smol::Timer::after(Duration::from_secs(3)).await;

        assert!(
            wallet_p2p.hosts().peers().is_empty(),
            "wallet with wrong magic must not connect to a DRKW node (got {} peers)",
            wallet_p2p.hosts().peers().len(),
        );

        drop(signal);
        ex_thread.join().expect("executor thread");
    });
}

/// A client-only wallet (app_name "dwow_wallet") connects inbound but MUST NOT
/// be counted as a full-node sync source by the node's peer filter
/// (linear_sync_client.rs WALLET_APP_NAME). Documents the fix so a node with
/// only wallet peers waits for a real peer instead of spinning on empty tips.
#[test]
fn test_wallet_peer_not_counted_as_full_node() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .try_init();

    smol::block_on(async {
        let ex: Arc<smol::Executor<'static>> = Arc::new(smol::Executor::new());
        let (signal, shutdown) = smol::channel::unbounded::<()>();
        let ex_thread = {
            let ex = ex.clone();
            std::thread::spawn(move || {
                let _ = smol::future::block_on(ex.run(shutdown.recv()));
            })
        };

        // Serving node: inbound P2P (default app_name "dwow_core").
        let serving_port = get_free_port();
        let serving_settings = loopback_settings(Some(serving_port), vec![], DRKW_MAGIC);
        let serving_p2p = P2p::new(serving_settings, ex.clone()).await
            .expect("serving P2p::new");
        serving_p2p.clone().start().await.expect("serving P2p::start");

        // Wallet: outbound P2P declaring the wallet's package name.
        let serving_url = Url::parse(&format!("tcp+tls://127.0.0.1:{serving_port}")).unwrap();
        let mut wallet_settings = loopback_settings(None, vec![serving_url], DRKW_MAGIC);
        wallet_settings.app_name = "dwow_wallet".to_string();
        let wallet_p2p = P2p::new(wallet_settings, ex.clone()).await
            .expect("wallet P2p::new");
        wallet_p2p.clone().start().await.expect("wallet P2p::start");

        // Wait for the wallet to connect AND complete the version handshake
        // (channel.version populated) so the peer filter can read its app_name.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let peers = serving_p2p.hosts().peers();
            if peers.iter().any(|c| c.version.get().is_some()) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "wallet never completed handshake with serving node"
            );
            smol::Timer::after(Duration::from_millis(200)).await;
        }

        // The node's sync client must NOT treat the wallet as a sync source.
        let client = crate::proto::linear_sync_client::LinearSyncClient::new(&serving_p2p);
        assert!(
            !client.has_full_node_peers(),
            "wallet peer (app_name dwow_wallet) must not be a full-node sync source"
        );
        assert!(
            client.filtered_peers().is_empty(),
            "wallet peer must be filtered out of full-node peers"
        );

        drop(signal);
        ex_thread.join().expect("executor thread");
    });
}
