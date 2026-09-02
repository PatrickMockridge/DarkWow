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

//! Daemon (mining node + observer) sync integration test.
//!
//! Two real daemon nodes over in-process loopback P2P: an authority node
//! (genesis + blocks) serves, and a fresh non-authority node runs
//! `consensus_linear_init_task` to PULL to the authority's tip. Asserts the
//! Python model's convergence invariant (equal block hash at every height).
//!
//! This is the same pull path the observer exercises (an observer is a
//! non-authority node without `miner_task`; `consensus_linear_init_task` is
//! identical). Run with:
//!   RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 \
//!     cargo test -p dwowd --lib -- daemon_sync_integration

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use url::Url;

use dwow_core::net::settings::{MagicBytes, NetworkProfile, Settings};
use dwow_core::net::P2p;
use dwow_sdk::blockchain::BlockHeight;

use crate::tests::genesis::GenesisHarness;
use crate::Network;

/// Unique suffix for the temp keys.toml — both daemon_sync tests run in the same
/// process (same `std::process::id()`), so a bare PID would collide and one test
/// would delete the other's keys file mid-run.
static KEYS_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Pick an ephemeral loopback TCP port (same trick as src/net/tests.rs).
fn get_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

/// Minimal plain-tcp P2P settings for an in-process loopback node.
/// The magic bytes MUST equal the chain magic: the daemon's genesis check
/// compares the synced genesis anchor_tx_id[0..4] against the node's P2P
/// magic_bytes (consensus_linear.rs genesis check).
fn loopback_settings(inbound: Option<u16>, peers: Vec<Url>, magic: [u8; 4]) -> Settings {
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

/// Build a real authority chain: genesis (height 1) + one coinbase (height 2).
/// Returns the chain state and the temp keys.toml path.
async fn build_authority_chain() -> (Arc<dwow_chain::CChainState>, std::path::PathBuf) {
    use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction, compute_merkle_root};
    use dwow_sdk::blockchain::expected_reward;

    let har = GenesisHarness::new().expect("GenesisHarness");

    let keys_toml = "[node0]\nwallet_secret = \
        \"0100000000000000000000000000000000000000000000000000000000000000\"\n\
        [node1]\nwallet_secret = \
        \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
    let keys_path = std::env::temp_dir().join(format!(
        "dwow_daemon_sync_{}_{}.toml",
        std::process::id(),
        KEYS_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::write(&keys_path, keys_toml).expect("write test keys");

    let miner_mgr = crate::accounts::AccountManager::open(
        &keys_path, Network::Testnet, "node0",
    ).expect("open miner AccountManager");
    let chain_magic = [0xDA, 0x57, 0x01, 0x57];

    // Block 1: genesis.
    let recipient_1 = crate::accounts::MiningRecipient::from_account(
        &miner_mgr, BlockHeight::new(1),
    ).expect("MiningRecipient height 1");
    crate::init_genesis(&har.chain_state, recipient_1, chain_magic)
        .await.expect("init_genesis");

    // Block 2: post-genesis coinbase.
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
        miner: [0u8; 32],
        commitment_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: PowSource::Native,
    };
    let block_2 = Block { header: header_2, transactions: vec![coinbase_tx_2] };
    let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &block_2.header.randomx_key)
        .expect("RandomXCache height 2");
    let vm = Arc::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM height 2"),
    );
    crate::block_acceptor::accept_block(
        &har.chain_state, &block_2, &[], &vm,
        BlockHeight::new(1), dwow_sdk::blockchain::BlockTarget::MAX, None,
    ).expect("accept_block height 2");

    (har.chain_state, keys_path)
}

/// A non-authority node pulls blocks from an authority node and converges to
/// the same chain (equal block hash at every height).
#[test]
fn test_daemon_pull_sync_converges() {
    dwow_native_token_contract::enable_deterministic_zk();

    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .try_init();

    smol::block_on(async {
        let chain_magic = [0xDA, 0x57, 0x01, 0x57];

        // ── Authority chain ────────────────────────────────────────────────
        let (authority_chain, keys_path) = build_authority_chain().await;
        let authority_height = authority_chain.get_height();
        assert_eq!(authority_height, BlockHeight::new(2));

        // ── Executor (runs P2P + sync task) ─────────────────────────────────
        let ex: Arc<smol::Executor<'static>> = Arc::new(smol::Executor::new());
        let (signal, shutdown) = smol::channel::unbounded::<()>();
        let ex_thread = {
            let ex = ex.clone();
            std::thread::spawn(move || {
                let _ = smol::future::block_on(ex.run(shutdown.recv()));
            })
        };

        // ── Serving node A: P2P + unified SyncServer (port+2) ──────────────
        let port_a = get_free_port();
        let serving_p2p = P2p::new(
            loopback_settings(Some(port_a), vec![], chain_magic), ex.clone(),
        ).await.expect("serving P2p::new");
        serving_p2p.clone().start().await.expect("serving P2p::start");
        // Serve the unified sync rail on port+2, matching what node B dials.
        let mut sync_addr_a = Url::parse(&format!("tcp+tls://127.0.0.1:{port_a}")).unwrap();
        if let Some(p) = sync_addr_a.port() {
            let _ = sync_addr_a.set_port(Some(p + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
        }
        let sync_server_a = dwow_chain::sync_connection::SyncServer::listen(
            sync_addr_a, chain_magic, authority_chain.clone(), None,
        ).await.expect("SyncServer::listen");
        smol::spawn(async move { let _ = sync_server_a.run().await; }).detach();

        // ── Syncing node B: fresh empty chain + consensus init task ─────────
        let syncing_chain = GenesisHarness::new_without_contracts()
            .expect("GenesisHarness empty").chain_state;
        let url_a = Url::parse(&format!("tcp+tls://127.0.0.1:{port_a}")).unwrap();
        let settings_b = loopback_settings(None, vec![url_a], chain_magic);

        let sync_state = Arc::new(AtomicU8::new(crate::SyncState::Initial as u8));
        let p2p_handler = crate::proto::DwowP2pHandler::init(
            &settings_b, &ex, Some(syncing_chain.clone()), None, None, None, sync_state.clone(),
        ).await.expect("DwowP2pHandler::init");

        let registry = crate::registry::DwowMinersRegistry::init_linear(
            Network::Testnet, syncing_chain.clone(),
        ).await.expect("DwowMinersRegistry::init_linear");

        let account_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open AccountManager");

        let node_b = crate::DwowNode::new(
            Some(syncing_chain.clone()),
            None,
            p2p_handler,
            registry,
            HashMap::new(),
            true,
            0,
            Arc::new(smol::lock::RwLock::new(account_mgr)),
            sync_state,
        ).await.expect("DwowNode::new");

        node_b.p2p_handler.start(&ex, &node_b).await.expect("DwowP2pHandler::start");

        let config = crate::task::consensus_linear::ConsensusInitTaskConfig {
            skip_sync: false,
            checkpoint_height: None,
            checkpoint: None,
            genesis_authority: None,
        };
        let task_node = node_b.clone();
        let task_ex = ex.clone();
        ex.spawn(async move {
            let _ = crate::task::consensus_linear::consensus_linear_init_task(
                task_node, config, task_ex,
            ).await;
        }).detach();

        // ── Poll until B converges to A's height ────────────────────────────
        // The genesis block is a full 9-contract bootstrap: applying it on the
        // syncing node re-executes all deployments (~10 min in debug). Allow a
        // generous window, not 60s.
        let deadline = Instant::now() + Duration::from_secs(1800);
        loop {
            let h = syncing_chain.get_height();
            if h >= authority_height {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "daemon sync timed out: syncing node at height {} (target {}) after 900s",
                h.get(), authority_height.get(),
            );
            smol::Timer::after(Duration::from_millis(500)).await;
        }

        // ── Assert convergence: equal block hash at every height ────────────
        for h in 1u64..=authority_height.get() {
            let bh = BlockHeight::new(h);
            let a_block = authority_chain.get_block(bh).expect("authority block");
            let b_block = syncing_chain.get_block(bh).expect("synced block");
            let a_hash = authority_chain.hash_block_with_cached_vm(&a_block).expect("a hash");
            let b_hash = syncing_chain.hash_block_with_cached_vm(&b_block).expect("b hash");
            assert_eq!(a_hash, b_hash, "block hash mismatch at height {}", h);
        }

        // ── Cleanup ─────────────────────────────────────────────────────────
        drop(signal);
        ex_thread.join().expect("executor thread");
        let _ = std::fs::remove_file(&keys_path);
    });
}

/// Build (but do NOT accept) a coinbase block at `height` on `chain_state`,
/// with an explicit timestamp. Same production coinbase path as the miner.
/// Used to construct competing blocks for the reorg test.
async fn build_coinbase_block(
    chain_state: &Arc<dwow_chain::CChainState>,
    keys_path: &std::path::Path,
    height: BlockHeight,
    timestamp: u64,
    account: &str,
) -> dwow_chain::Block {
    use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction, compute_merkle_root};
    use dwow_sdk::blockchain::expected_reward;

    let miner_mgr = crate::accounts::AccountManager::open(
        keys_path, Network::Testnet, account,
    ).expect("open miner AccountManager");
    let reward = expected_reward(height);
    let recipient = crate::accounts::MiningRecipient::from_account(&miner_mgr, height)
        .expect("MiningRecipient");
    let linear_zk = crate::registry::model::LinearPowRewardZk::new(chain_state.clone())
        .await.expect("LinearPowRewardZk");
    let (coinbase, _pi, pow_reward_call, _blind) =
        crate::registry::model::build_linear_coinbase(recipient, reward, &linear_zk, height)
            .await.expect("build_linear_coinbase");
    let coinbase_tx = Transaction {
        version: dwow_sdk::blockchain::BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![pow_reward_call],
        lock_time: 0,
        nullifiers: vec![coinbase.nullifier],
        witness: vec![],
    };
    let prev = chain_state.get_latest_block().expect("get_latest_block");
    let prev_hash = chain_state.hash_block_with_cached_vm(&prev).expect("hash failed");
    let header = BlockHeader {
        fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
        version: dwow_sdk::blockchain::BlockVersion::CURRENT,
        previous: prev_hash,
        merkle_root: compute_merkle_root(&[coinbase_tx.clone()]),
        timestamp: dwow_sdk::blockchain::BlockTimestamp::new(timestamp),
        target: dwow_sdk::blockchain::BlockTarget::MAX,
        nonce: 0,
        height,
        uncle_merkle_root: [0u8; 32],
        total_reward: reward,
        randomx_key: Miner::derive_key_from_height(height),
        miner: [0u8; 32],
        commitment_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: dwow_sdk::blockchain::MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: PowSource::Native,
    };
    Block { header, transactions: vec![coinbase_tx] }
}

/// Build and accept a coinbase block at `height` on `chain_state`, returning
/// the block (for later broadcast). Same production path as the miner.
async fn mine_coinbase_block(
    chain_state: &Arc<dwow_chain::CChainState>,
    keys_path: &std::path::Path,
    height: BlockHeight,
) -> dwow_chain::Block {
    let block = build_coinbase_block(chain_state, keys_path, height, height.get() * 120, "node0").await;
    let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &block.header.randomx_key)
        .expect("RandomXCache");
    let vm = Arc::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"),
    );
    crate::block_acceptor::accept_block(
        chain_state, &block, &[], &vm,
        height.pred().expect("pred"), dwow_sdk::blockchain::BlockTarget::MAX, None,
    ).expect("accept_block");
    block
}

/// A miner broadcasts a freshly-mined block; a synced peer receives it via the
/// broadcast (push) path and applies it without a pull round-trip.
#[test]
fn test_daemon_broadcast_propagates() {
    dwow_native_token_contract::enable_deterministic_zk();

    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .try_init();

    smol::block_on(async {
        let chain_magic = [0xDA, 0x57, 0x01, 0x57];

        // ── Authority chain: genesis + block 2 ─────────────────────────────
        let (authority_chain, keys_path) = build_authority_chain().await;

        // ── Executor ───────────────────────────────────────────────────────
        let ex: Arc<smol::Executor<'static>> = Arc::new(smol::Executor::new());
        let (signal, shutdown) = smol::channel::unbounded::<()>();
        let ex_thread = {
            let ex = ex.clone();
            std::thread::spawn(move || {
                let _ = smol::future::block_on(ex.run(shutdown.recv()));
            })
        };

        // ── Serving/mining node A ──────────────────────────────────────────
        let port_a = get_free_port();
        let p2p_a = P2p::new(
            loopback_settings(Some(port_a), vec![], chain_magic), ex.clone(),
        ).await.expect("A P2p::new");
        p2p_a.clone().start().await.expect("A P2p::start");
        // Serve the unified sync rail on port+2, matching what node B dials.
        let mut sync_addr_a = Url::parse(&format!("tcp+tls://127.0.0.1:{port_a}")).unwrap();
        if let Some(p) = sync_addr_a.port() {
            let _ = sync_addr_a.set_port(Some(p + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
        }
        let sync_server_a = dwow_chain::sync_connection::SyncServer::listen(
            sync_addr_a, chain_magic, authority_chain.clone(), None,
        ).await.expect("SyncServer::listen");
        smol::spawn(async move { let _ = sync_server_a.run().await; }).detach();

        // ── Syncing node B (pulls to height 2) ─────────────────────────────
        let syncing_chain = GenesisHarness::new_without_contracts()
            .expect("GenesisHarness empty").chain_state;
        let url_a = Url::parse(&format!("tcp+tls://127.0.0.1:{port_a}")).unwrap();
        let settings_b = loopback_settings(None, vec![url_a], chain_magic);
        let sync_state = Arc::new(AtomicU8::new(crate::SyncState::Initial as u8));
        let p2p_handler = crate::proto::DwowP2pHandler::init(
            &settings_b, &ex, Some(syncing_chain.clone()), None, None, None, sync_state.clone(),
        ).await.expect("DwowP2pHandler::init");
        let registry = crate::registry::DwowMinersRegistry::init_linear(
            Network::Testnet, syncing_chain.clone(),
        ).await.expect("registry");
        let account_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("AccountManager");
        let node_b = crate::DwowNode::new(
            Some(syncing_chain.clone()), None, p2p_handler, registry,
            HashMap::new(), true, 0, Arc::new(smol::lock::RwLock::new(account_mgr)),
            sync_state.clone(),
        ).await.expect("DwowNode::new");
        node_b.p2p_handler.start(&ex, &node_b).await.expect("DwowP2pHandler::start");

        // Register B's broadcast receive handler so it can apply pushed blocks.
        let broadcast_b = crate::proto::LinearBroadcastHandler::init(
            &node_b.p2p_handler.p2p, syncing_chain.clone(), None, sync_state.clone(),
        ).await;
        broadcast_b.start(&ex).await.expect("B LinearBroadcastHandler::start");

        let config = crate::task::consensus_linear::ConsensusInitTaskConfig {
            skip_sync: false,
            checkpoint_height: None,
            checkpoint: None,
            genesis_authority: None,
        };
        let task_node = node_b.clone();
        let task_ex = ex.clone();
        ex.spawn(async move {
            let _ = crate::task::consensus_linear::consensus_linear_init_task(
                task_node, config, task_ex,
            ).await;
        }).detach();

        // Wait for B to pull to height 2 (genesis re-execution is ~10 min).
        let deadline = Instant::now() + Duration::from_secs(1800);
        loop {
            if syncing_chain.get_height() >= BlockHeight::new(2) { break; }
            assert!(Instant::now() < deadline, "B never pulled to height 2");
            smol::Timer::after(Duration::from_millis(500)).await;
        }

        // ── A mines block 3 and broadcasts it ──────────────────────────────
        let block_3 = mine_coinbase_block(&authority_chain, &keys_path, BlockHeight::new(3)).await;
        crate::proto::linear_broadcast::broadcast_block(&p2p_a, block_3, vec![]).await;

        // ── Assert B applies the broadcast block (push, not pull) ──────────
        let deadline = Instant::now() + Duration::from_secs(1800);
        loop {
            if syncing_chain.get_height() >= BlockHeight::new(3) { break; }
            assert!(Instant::now() < deadline, "B never applied broadcast block 3");
            smol::Timer::after(Duration::from_millis(500)).await;
        }
        let a3 = authority_chain.get_block(BlockHeight::new(3)).expect("a3");
        let b3 = syncing_chain.get_block(BlockHeight::new(3)).expect("b3");
        assert_eq!(
            authority_chain.hash_block_with_cached_vm(&a3).unwrap(),
            syncing_chain.hash_block_with_cached_vm(&b3).unwrap(),
            "broadcast block hash mismatch at height 3",
        );

        drop(signal);
        ex_thread.join().expect("executor thread");
        let _ = std::fs::remove_file(&keys_path);
    });
}

/// A syncing node adopts the authority's chain (hash-equal at every shared
/// height) and its `sync_state` reaches `CaughtUp` only after sync completes.
/// This is the deterministic-startup invariant (node-startup-spec.md §2): a
/// `miner` node is an observer until `CaughtUp`, so its mining gate stays
/// closed while behind. We assert the gate directly (via `sync_state`) rather
/// than running the unbounded `miner_task`.
#[test]
fn test_sync_state_gates_mining_until_caught_up() {
    dwow_native_token_contract::enable_deterministic_zk();

    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .try_init();

    smol::block_on(async {
        let chain_magic = [0xDA, 0x57, 0x01, 0x57];

        let (authority_chain, keys_path) = build_authority_chain().await;
        let authority_height = authority_chain.get_height();
        assert_eq!(authority_height, BlockHeight::new(2));

        let ex: Arc<smol::Executor<'static>> = Arc::new(smol::Executor::new());
        let (signal, shutdown) = smol::channel::unbounded::<()>();
        let ex_thread = {
            let ex = ex.clone();
            std::thread::spawn(move || {
                let _ = smol::future::block_on(ex.run(shutdown.recv()));
            })
        };

        // ── Serving node A (static authority chain) ─────────────────────────
        let port_a = get_free_port();
        let serving_p2p = P2p::new(
            loopback_settings(Some(port_a), vec![], chain_magic), ex.clone(),
        ).await.expect("serving P2p::new");
        serving_p2p.clone().start().await.expect("serving P2p::start");
        let mut sync_addr_a = Url::parse(&format!("tcp+tls://127.0.0.1:{port_a}")).unwrap();
        if let Some(p) = sync_addr_a.port() {
            let _ = sync_addr_a.set_port(Some(p + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
        }
        let sync_server_a = dwow_chain::sync_connection::SyncServer::listen(
            sync_addr_a, chain_magic, authority_chain.clone(), None,
        ).await.expect("SyncServer::listen");
        smol::spawn(async move { let _ = sync_server_a.run().await; }).detach();

        // ── Mining node B (fresh chain + miner task ENABLED) ─────────────────
        let syncing_chain = GenesisHarness::new_without_contracts()
            .expect("GenesisHarness empty").chain_state;
        let url_a = Url::parse(&format!("tcp+tls://127.0.0.1:{port_a}")).unwrap();
        let settings_b = loopback_settings(None, vec![url_a], chain_magic);
        let sync_state = Arc::new(AtomicU8::new(crate::SyncState::Initial as u8));
        let p2p_handler = crate::proto::DwowP2pHandler::init(
            &settings_b, &ex, Some(syncing_chain.clone()), None, None, None, sync_state.clone(),
        ).await.expect("DwowP2pHandler::init");
        let registry = crate::registry::DwowMinersRegistry::init_linear(
            Network::Testnet, syncing_chain.clone(),
        ).await.expect("registry");
        let account_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("AccountManager");
        let node_b = crate::DwowNode::new(
            Some(syncing_chain.clone()), None, p2p_handler, registry,
            HashMap::new(), true, 0, Arc::new(smol::lock::RwLock::new(account_mgr)),
            sync_state,
        ).await.expect("DwowNode::new");
        node_b.p2p_handler.start(&ex, &node_b).await.expect("DwowP2pHandler::start");

        let config = crate::task::consensus_linear::ConsensusInitTaskConfig {
            skip_sync: false,
            checkpoint_height: None,
            checkpoint: None,
            genesis_authority: None,
        };
        let task_node = node_b.clone();
        let task_ex = ex.clone();
        ex.spawn(async move {
            let _ = crate::task::consensus_linear::consensus_linear_init_task(
                task_node, config, task_ex,
            ).await;
        }).detach();

        // Mining gate is closed before sync: a `miner` node would be paused
        // here (node-startup-spec.md §2: observer until CaughtUp).
        assert_ne!(
            crate::SyncState::load(&node_b.mining_state.sync_state),
            crate::SyncState::CaughtUp,
            "sync_state must not be CaughtUp before the chain is synced",
        );

        // ── Wait for B to sync to the authority tip ─────────────────────────
        let deadline = Instant::now() + Duration::from_secs(1800);
        loop {
            if syncing_chain.get_height() >= authority_height {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "miner never synced to authority tip ({} vs {})",
                syncing_chain.get_height().get(), authority_height.get(),
            );
            smol::Timer::after(Duration::from_millis(500)).await;
        }

        // ── Assert adoption: hash-equal at every shared height (no fork) ────
        for h in 1u64..=authority_height.get() {
            let bh = BlockHeight::new(h);
            let a_block = authority_chain.get_block(bh).expect("authority block");
            let b_block = syncing_chain.get_block(bh).expect("synced block");
            let a_hash = authority_chain.hash_block_with_cached_vm(&a_block).unwrap();
            let b_hash = syncing_chain.hash_block_with_cached_vm(&b_block).unwrap();
            assert_eq!(a_hash, b_hash, "forked: hash mismatch at height {}", h);
        }

        // Mining gate is open after sync: sync_state reaches CaughtUp.
        assert_eq!(
            crate::SyncState::load(&node_b.mining_state.sync_state),
            crate::SyncState::CaughtUp,
            "sync_state must be CaughtUp after syncing to the authority tip",
        );

        drop(signal);
        ex_thread.join().expect("executor thread");
        let _ = std::fs::remove_file(&keys_path);
    });
}

/// F2/F3: a divergent node reorgs onto the competing chain via
/// `activate_best_chain` (Bitcoin `DisconnectBlock`/`ConnectBlock`). Two valid
/// block-3 candidates share parent block 2; the chain adopts the competing one
/// after disconnect + cumulative-commit rollback + reconnect.
#[test]
fn test_activate_best_chain_adopts_competing_block() {
    dwow_native_token_contract::enable_deterministic_zk();

    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(true)
        .try_init();

    smol::block_on(async {
        let (chain, keys_path) = build_authority_chain().await; // genesis + block 2

        // Two valid block-3 candidates built against the SAME parent (block 2);
        // they use DIFFERENT miners (node0 vs node1), so their coinbase
        // commitments (and hashes) genuinely differ — a real competing fork.
        let block_3_a = build_coinbase_block(&chain, &keys_path, BlockHeight::new(3), 300, "node0").await;
        let block_3_b = build_coinbase_block(&chain, &keys_path, BlockHeight::new(3), 400, "node1").await;
        assert_ne!(
            chain.hash_block_with_cached_vm(&block_3_a).unwrap(),
            chain.hash_block_with_cached_vm(&block_3_b).unwrap(),
            "competing blocks must differ (different timestamp)",
        );

        // Accept candidate A first — it becomes the canonical tip at height 3.
        let rx_flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(rx_flags, &block_3_a.header.randomx_key)
            .expect("RandomXCache");
        let vm = Arc::new(randomx::RandomXVM::new(rx_flags, Some(rx_cache), None).expect("RandomXVM"));
        crate::block_acceptor::accept_block(
            &chain, &block_3_a, &[], &vm, BlockHeight::new(2), block_3_a.header.target, None,
        ).expect("accept block 3a");
        assert_eq!(chain.get_height(), BlockHeight::new(3));

        // Reorg: adopt the competing block 3b (fork point = block 2).
        crate::block_acceptor::activate_best_chain(
            &chain, &[block_3_b.clone()], BlockHeight::new(2), None,
        ).expect("activate_best_chain");

        // The canonical tip must now be block 3b (height unchanged).
        assert_eq!(chain.get_height(), BlockHeight::new(3));
        let tip = chain.get_latest_block().expect("tip");
        let tip_hash = chain.hash_block_with_cached_vm(&tip).unwrap();
        let b_hash = chain.hash_block_with_cached_vm(&block_3_b).unwrap();
        assert_eq!(tip_hash, b_hash, "chain must adopt the competing block after reorg");

        let _ = std::fs::remove_file(&keys_path);
    });
}
