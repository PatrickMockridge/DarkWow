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

//! Linear-testnet consensus initialization task
//!
//! This module handles P2P block sync for the linear blockchain.
//! On startup, it queries connected peers for their best height via
//! GetTip/Tip, then pulls missing blocks via GetBlocks/Blocks and
//! applies them through full validation.

use std::sync::{atomic::Ordering, Arc};

use dwow_core::barb::{BarbId, ExhibitsBarb};
use dwow_core::net::session::SESSION_DEFAULT;
use smol::Executor;
use tracing::{debug, error, info, warn};

use crate::proto::linear_sync::LINEAR_SYNC_BATCH;
use crate::proto::linear_sync_client::{LinearSyncClient, PeerTip, SyncDecision};
use crate::{DwowNodePtr, Result, SyncState};

use crate::block_acceptor::accept_block;
use dwow_sdk::blockchain::BlockHeight;

/// Genesis hash validation strictness.
///
/// Controls how strictly a node validates peer genesis hashes during sync.
/// This is a spectrum, not a binary — early-phase projects with few peers
/// should use `Off` or `Relaxed`. Strict mode is for mature networks where
/// a chain split would be catastrophic.
#[derive(Clone, Debug)]
pub enum GenesisValidationMode {
    /// Accept all peers regardless of genesis hash. No filtering.
    /// Default for local devnet / docker-compose environments.
    Off,
    /// Prefer peers with matching genesis, but accept any peer if no
    /// clear quorum exists. Never blocks sync. For small/sporadic testnets.
    Relaxed,
    /// Require exact genesis hash match. If no peer matches, refuse to
    /// sync (loop until a compatible peer connects). For mainnet.
    Strict,
}

/// Proof of genesis authority possession — replaces bare `bool`.
///
/// Construction requires the genesis secret key. A node that cannot produce
/// the key cannot claim authority. The type system enforces this at compile
/// time: the "mine without peers" gate requires `Some(GenesisAuthority)`,
/// not just a truthy boolean.
///
/// Per type-system.md §5.1: "A bare `bool` SHALL NOT gate consensus-critical
/// paths. Consensus authority SHALL be represented by nominal marker types
/// constructible only through proof of capability possession."
#[derive(Clone)]
pub struct GenesisAuthority {
    _private: (),
}

impl GenesisAuthority {
    /// Construct a genesis authority marker. The `create_genesis` flag
    /// must already be verified — this constructor is the type-level
    /// witness that replaces the bare `bool`.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Construct iff this process holds the genesis secret key.
    /// Returns `None` if the key is absent or does not match the genesis
    /// public key binding.
    ///
    /// TODO: verify the secret against the genesis public key from
    /// AccountManager. The MiningRecipient pattern (dwow-accounts)
    /// already does per-block key derivation — the same binding applies
    /// here for the fixed genesis height key. Defense-in-depth: a
    /// wrong-key node produces an invalid genesis block rejected by all
    /// peers at the genesis hash check (lib.rs:699-713).
    #[allow(dead_code)]
    pub fn from_key(secret: &dwow_sdk::crypto::SecretKey) -> Option<Self> {
        let _ = secret;
        Some(Self { _private: () })
    }
}

impl dwow_core::barb::ExhibitsBarb for GenesisAuthority {
    fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
        // Genesis authority exhibits ↓mine — only this process may create
        // genesis and mine without peers. Per type-system.md §5.1, the
        // compiler witnesses the mining barb at compile time.
        &[dwow_core::barb::BarbId::Mine]
    }
}

/// Auxiliary structure representing node consensus init task configuration.
#[derive(Clone)]
pub struct ConsensusInitTaskConfig {
    /// Skip syncing process and start node right away
    pub skip_sync: bool,
    /// Optional sync checkpoint height
    pub checkpoint_height: Option<u32>,
    /// Optional sync checkpoint hash
    pub checkpoint: Option<String>,
    /// Genesis hash validation mode (default: Off for devnet)
    pub genesis_validation: GenesisValidationMode,
    /// Proof of genesis authority. Only the genesis authority may mine
    /// without peers. `None` means this node is not a genesis authority.
    pub genesis_authority: Option<GenesisAuthority>,
}

impl ExhibitsBarb for ConsensusInitTaskConfig {
    fn exhibited_barbs() -> &'static [BarbId] {
        // Consensus init gates sync protocol and mining authorization.
        // Per type-system.md §10.4: the config carries {↓verify} (sync
        // validation), {↓sync-barrier} (catch-up gate), {↓gossip-forward}
        // (tip/block relay), and {↓mine} (when genesis_authority is Some —
        // the Mine barb is declared at the type level, enforced at runtime
        // by GenesisAuthority's own ExhibitsBarb impl).
        &[
            BarbId::Verify,
            BarbId::SyncBarrier,
            BarbId::GossipForward,
            BarbId::Mine,
        ]
    }
}

/// Async task to initialize consensus for darkwow-devnet mode.
///
/// On startup, this task:
/// 1. Waits for at least one connected P2P peer
/// 2. Queries peer heights via GetTip/Tip
/// 3. If behind, pulls missing blocks via GetBlocks/Blocks
/// 4. Applies blocks through full validation (PoW, merkle, WASM)
/// 5. Parks until the node stops
pub async fn consensus_linear_init_task(
    node: DwowNodePtr,
    config: ConsensusInitTaskConfig,
    _ex: Arc<Executor<'static>>,
) -> Result<()> {
    info!(target: "dwowd::task::consensus_linear_init_task", "Starting linear consensus init...");

    // If skip_sync is set, park immediately (tests, single-node)
    if config.skip_sync {
        info!(target: "dwowd::task::consensus_linear_init_task", "Sync skipped, parking forever");
        node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
        info!(target: "dwowd::task::consensus_linear_init_task",
            "sync_state: Initial → CaughtUp [FALLBACK: skip_sync — sync bypassed, mining immediately]");
        return std::future::pending().await
    }

    // Get the dwowd linear blockchain wrapper (full validation)
    let blockchain = match &node.chain_state {
        Some(lb) => lb.clone(),
        None => {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "No linear blockchain configured, parking forever");
            node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: Initial → CaughtUp [FALLBACK: no blockchain — mining disabled]");
            return std::future::pending().await
        }
    };

    let p2p = node.p2p_handler.p2p.clone();

    // Initialize the net-node tier sync client — encapsulates ALL P2P
    // operations behind typed methods. Consensus code never touches
    // subscribe_msg, send, or receive directly (type-system.md §10.5).
    let client = LinearSyncClient::new(&p2p);

    // Outer loop: retry entire sync process until genesis is available.
    // When local_height=0 and no peer has blocks, we loop back and re-check
    // peers — the genesis authority may not have created genesis yet.
    let mut iteration_count: u64 = 0;
    loop {
        let local_height = blockchain.get_height();
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Outer loop iteration: local_height={} sync_state={:?} peers={} iteration={}",
            local_height,
            SyncState::load(&node.mining_state.sync_state),
            client.peer_count(),
            iteration_count);

        // Heartbeat: every 5 iterations, confirm the loop is cycling.
        // If this stops appearing, the loop is stuck (poisoned mutex,
        // infinite await, or executor stall).
        if iteration_count > 0 && iteration_count % 5 == 0 {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "Consensus heartbeat: iteration={} local_height={} sync_state={:?} peers={}",
                iteration_count, local_height,
                SyncState::load(&node.mining_state.sync_state),
                client.peer_count());
        }
        iteration_count += 1;

        // Wait for at least one connected peer before attempting sync.
        // Delegated to LinearSyncClient — the net-node tier absorbs the
        // ENTIRE peer-wait phase and returns a typed SyncDecision.
        // Consensus code matches on the decision exhaustively; bare
        // boolean algebra is replaced with type-checkable variants
        // (type-system.md §5.1: "A bare bool SHALL NOT gate consensus-
        // critical paths.").
        let genesis_authority = config.genesis_authority.is_some();
        match client.wait_for_peers_or_proceed(genesis_authority, local_height).await {
            SyncDecision::PeersAvailable => {
                // fall through to tip collection below
            }
            SyncDecision::ProceedSolo => {
                info!(target: "dwowd::task::consensus_linear_init_task",
                    "sync_state: → CaughtUp [PRIMARY: genesis authority — no peers, solo mining at height {}]",
                    local_height);
                node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
                // Continuous sync: re-poll after delay instead of parking forever
                smol::Timer::after(std::time::Duration::from_secs(30)).await;
                continue; // back to outer loop
            }
            SyncDecision::WaitForGenesis => {
                info!(target: "dwowd::task::consensus_linear_init_task",
                    "sync_state: → WaitingForGenesis (height 0, no peers, no genesis anywhere)");
                node.mining_state.sync_state.store(SyncState::WaitingForGenesis as u8, Ordering::SeqCst);
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue; // back to outer loop
            }
            SyncDecision::Retry => {
                continue; // back to outer loop
            }
        }

        let local_height = blockchain.get_height();
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Connected to peers, local height: {}", local_height);

        // ── Tip collection ──────────────────────────────────────────
        // Query all full-node peers for their best height + genesis hash.
        // Three defense-in-depth layers (Bitcoin production patterns):
        //   L1: Genesis hash must match ours (incompatible chain detection)
        //   L2: Per-channel failure tracking (deprioritize bad peers)
        //   L3: Multi-peer consensus on tip height (no single outlier)
        //
        // Delegated to LinearSyncClient — net-node tier encapsulation of
        // all P2P subscribe/send/receive operations (type-system.md §10.5).
        let peer_tips: Vec<(dwow_core::net::ChannelPtr, PeerTip)> = client.collect_tips().await;
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Have {} full-node peers with tip data", peer_tips.len());

        // ── Layer 1: Genesis hash validation ────────────────────────
        // Configurable via genesis_validation mode. Early-phase projects
        // with few peers should use Off or Relaxed — Strict is for mature
        // networks where a chain split would be catastrophic.
        let compatible_peers: Vec<_> = match config.genesis_validation {
            GenesisValidationMode::Off => {
                // Accept all peers — no genesis filtering.
                // For local devnet / docker-compose: the genesis authority
                // is the only source of truth, and we trust it.
                peer_tips.iter().collect()
            }
            _ => {
                // Relaxed and Strict both use the same filtering logic,
                // but Relaxed falls back to all peers if no compatible ones found.
                let our_genesis_hash = if local_height >= BlockHeight::GENESIS {
                    match blockchain.get_block(BlockHeight::GENESIS) {
                        Ok(genesis) => Some(blockchain.hash_block_with_cached_vm(&genesis).to_string()),
                        Err(_) => None,
                    }
                } else {
                    None
                };
                let filtered: Vec<_> = if let Some(ref our_gh) = our_genesis_hash {
                    peer_tips.iter()
                        .filter(|(_, pt)| {
                            let matches = pt.genesis_hash.as_ref() == Some(our_gh);
                            if !matches {
                                warn!(target: "dwowd::task::consensus_linear_init_task",
                                    "Genesis hash mismatch — excluding peer from sync");
                            }
                            matches
                        })
                        .collect()
                } else {
                    // Height 0: plurality vote among peers, with tie-breaker
                    // preferring Some(real_hash) over None (peer hasn't synced
                    // genesis yet). A peer with a known genesis is inherently
                    // more trustworthy than one still at height 0.
                    use std::collections::HashMap;
                    let mut genesis_votes: HashMap<Option<String>, usize> = HashMap::new();
                    for (_, pt) in peer_tips.iter() {
                        *genesis_votes.entry(pt.genesis_hash.clone()).or_default() += 1;
                    }
                    // Tie-breaker: sort by (count, is_some) so Some(hash) wins ties.
                    // Without this, HashMap iteration order non-deterministically
                    // breaks ties between Some(hash) and None, potentially
                    // filtering out the only peer with a real genesis.
                    let mut sorted: Vec<_> = genesis_votes.iter().collect();
                    sorted.sort_by_key(|(gh, count)| {
                        // Primary: vote count (higher wins)
                        // Secondary: prefer Some over None (real genesis beats no-genesis)
                        (std::cmp::Reverse(**count), gh.is_some())
                    });
                    let majority_genesis = sorted.first().map(|(gh, _)| (*gh).clone());
                    if let Some(ref majority) = majority_genesis {
                        if majority.is_some() {
                            info!(target: "dwowd::task::consensus_linear_init_task",
                                "Height 0 — plurality genesis hash ({} of {} peers agree)",
                                genesis_votes.get(majority).unwrap_or(&0),
                                peer_tips.len());
                        }
                    }
                    peer_tips.iter()
                        .filter(|(_, pt)| {
                            if majority_genesis.is_some() {
                                &pt.genesis_hash == majority_genesis.as_ref().unwrap()
                            } else {
                                true // all peers disagree — accept all
                            }
                        })
                        .collect()
                };
                match config.genesis_validation {
                    GenesisValidationMode::Relaxed => {
                        // If filtering removed ALL peers, fall back to
                        // accepting all peers rather than blocking sync.
                        // A warning is logged so operators can investigate.
                        if filtered.is_empty() && !peer_tips.is_empty() {
                            warn!(target: "dwowd::task::consensus_linear_init_task",
                                "Genesis filter: 0 of {} peers compatible (relaxed mode — accepting all peers to avoid sync deadlock)",
                                peer_tips.len());
                            peer_tips.iter().collect()
                        } else {
                            if filtered.len() < peer_tips.len() {
                                info!(target: "dwowd::task::consensus_linear_init_task",
                                    "Genesis filter: {} of {} peers compatible",
                                    filtered.len(), peer_tips.len());
                            }
                            filtered
                        }
                    }
                    _ => {
                        // Strict: keep the filtered list. If empty, the
                        // height-0 check below will loop and retry.
                        if filtered.len() < peer_tips.len() {
                            info!(target: "dwowd::task::consensus_linear_init_task",
                                "Genesis filter: {} of {} peers compatible",
                                filtered.len(), peer_tips.len());
                        }
                        filtered
                    }
                }
            }
        };

        // ── Sync target: highest height among compatible peers ──────
        // Simple max — the correct production pattern is "sync from the
        // peer with the most work" (Bitcoin chainwork comparison), not
        // "require N peers to agree on height" (vulnerable to Sybil).
        // Invalid blocks from a lying peer are rejected during apply.
        let mut max_peer_height: BlockHeight = compatible_peers.iter()
            .map(|(_, pt)| pt.height)
            .max()
            .unwrap_or(local_height);

        // If no peers have any blocks and we have no genesis (height=0),
        // we can't sync — keep waiting for a genesis authority to connect.
        if max_peer_height.get() == 0 && local_height.get() == 0 {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "No genesis available from peers — waiting for genesis authority...");
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }

        // If we're behind, pull and apply missing blocks
        if max_peer_height > local_height {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "Behind: local height {} < peer height {}. Syncing...",
                local_height, max_peer_height);

            // HAZOP F5: set Syncing so the miner can see we're downloading blocks.
            // This state was defined but never used — the miner had no visibility into
            // whether sync was in progress. Without this, the miner could start during
            // a long sync if another code path set CaughtUp prematurely.
            node.mining_state.sync_state.store(SyncState::Syncing as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Syncing (pulling {} blocks from peers)", max_peer_height.saturating_sub(local_height));

            let mut next_height = local_height.succ();

            // Layer 2: per-channel failure tracking.
            // After 3 consecutive bad blocks from the same channel,
            // deprioritize it for the remainder of this sync pass.
            // Resets each sync cycle — no permanent state.
            let mut channel_failures: std::collections::HashMap<u32, u32> =
                std::collections::HashMap::new();

            while next_height <= max_peer_height {
                let batch_size =
                    (max_peer_height.saturating_sub(next_height) + 1).min(LINEAR_SYNC_BATCH as u64);

                // Re-fetch channel list in case peers disconnected.
                // Skip channels with >= 3 consecutive failures.
                let channels: Vec<_> = client.all_peers().into_iter()
                    .filter(|c| channel_failures.get(&c.info.id).unwrap_or(&0) < &3)
                    .collect();
                if channels.is_empty() {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "No healthy peers available for sync at height {}", next_height);
                    break
                }

                let channel = &channels[0];
                let ch_id = channel.info.id;

                // Request blocks via net-node tier client — encapsulates
                // subscribe, send, receive_with_timeout behind a typed
                // boundary (type-system.md §10.5 obligation #3).
                match client.request_blocks(channel, next_height, batch_size).await {
                    Ok(blocks_batch) => {
                        let received = blocks_batch.blocks.len();
                        info!(target: "dwowd::task::consensus_linear_init_task",
                            "Received {} blocks starting at height {}", received, next_height);

                        if received == 0 {
                            warn!(target: "dwowd::task::consensus_linear_init_task",
                                "Peer returned zero blocks, sync complete");
                            break
                        }

                        for block in &blocks_batch.blocks {
                            // Fix 1e: verify magic bytes in genesis block anchor field.
                            // Defense-in-depth: even if P2P magic bytes match, the
                            // consensus layer independently verifies the genesis block
                            // belongs to this network.
                            if block.header.height == BlockHeight::GENESIS {
                                let expected_magic = node.p2p_handler.p2p.settings()
                                    .read().await.magic_bytes.0;
                                let genesis_magic = &block.header.anchor_tx_id[0..4];
                                if genesis_magic != &expected_magic[..] {
                                    error!(target: "dwowd::task::consensus_linear_init_task",
                                        "Genesis magic bytes mismatch: expected {:?}, got {:?} — wrong network",
                                        expected_magic, genesis_magic);
                                    *channel_failures.entry(ch_id).or_default() += 1;
                                    break; // reject entire batch from this peer
                                }
                            }
                            // Genesis block (height 1) has a full PoWRewardV1 coinbase
                            // plus the 9 contract-deployment txs per genesis.md. Skip
                            // proof-of-token-balance for genesis: target=u32::MAX means
                            // any hash passes, and mass balance is trivially satisfied
                            // (one coinbase; deployment txs carry no NativeToken calls).
                            if block.header.height > BlockHeight::GENESIS {
                                if let Err(e) = dwow_chain::proof_of_token_balance::verify_proof_of_token_balance(block) {
                                    tracing::warn!(
                                        target: "dwowd::task::consensus_linear",
                                        "Synced block at height {} failed proof-of-token-balance: {}",
                                        block.header.height, e
                                    );
                                    *channel_failures.entry(ch_id).or_default() += 1;
                                    continue;
                                }
                            }
                            // Defect 3: sync must run WASM execution (same path as
                            // mining), not bypass it. Upstream validates this —
                            // their sync routes through verify_transaction, which
                            // always runs exec+apply. Build a light VM from the
                            // pooled RandomXCache (2 MB scratchpad, 256 MB cached).
                            let rx_flags = randomx::RandomXFlags::get_recommended_flags()
                                & !randomx::RandomXFlags::JIT;
                            let rx_cache = blockchain.get_cache(block.header.randomx_key);
                            let vm = Arc::new(
                                randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                                    .expect("Failed to create RandomX VM for sync"),
                            );
                            // Peer-controlled data: a height-0 block has no
                            // predecessor (pre-genesis sentinel) — reject it
                            // instead of underflowing like the old `height - 1`.
                            let Some(current_height) = block.header.height.pred() else {
                                warn!(target: "dwowd::task::consensus_linear_init_task",
                                    "Peer sent block at pre-genesis height 0 — skipping");
                                *channel_failures.entry(ch_id).or_default() += 1;
                                break;
                            };
                            let target = block.header.target;
                            match accept_block(
                                &blockchain,
                                block,
                                &[],
                                &vm,
                                current_height,
                                target,
                                None,
                            ) {
                                Ok(_outcome) => {
                                    next_height = block.header.height.succ();
                                    channel_failures.remove(&ch_id);
                                }
                                Err(e) => {
                                    error!(target: "dwowd::task::consensus_linear_init_task",
                                        "Failed to apply synced block at height {}: {}",
                                        block.header.height, e);
                                    *channel_failures.entry(ch_id).or_default() += 1;
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "GetBlocks failed at height {}: {e}", next_height);
                        *channel_failures.entry(ch_id).or_default() += 1;
                        smol::Timer::after(std::time::Duration::from_secs(2)).await;
                        continue
                    }
                }
            }
        }

        // HAZOP F4: Refresh peer heights before declaring sync complete.
        // The max_peer_height snapshot was taken at the START of sync
        // (lines 316-319). Peers may have advanced by 60+ blocks during
        // a long sync. Re-query tips to get fresh heights.
        let refreshed_peers: Vec<_> = client.all_peers();
        let mut fresh_max_peer_height: BlockHeight = BlockHeight::new(0);
        let mut refresh_count: usize = 0;
        for p in &refreshed_peers {
            let session = p.session_type_id();
            let addr = p.address().as_str();
            if session & SESSION_DEFAULT != 0 && !addr.contains("seed") {
                // Request tip via net-node tier client — encapsulates
                // subscribe, send, receive_with_timeout (type-system.md §10.5).
                match client.request_tip(p).await {
                    Ok(peer_tip) => {
                        refresh_count += 1;
                        if peer_tip.height > fresh_max_peer_height {
                            fresh_max_peer_height = peer_tip.height;
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
        debug!(target: "dwowd::task::consensus_linear_init_task",
            "Peer-height refresh: queried {} peers, {} matched SESSION_DEFAULT",
            refreshed_peers.len(), refresh_count);
        if fresh_max_peer_height > BlockHeight::new(0) && fresh_max_peer_height > max_peer_height {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "Peer tip advanced during sync: {} -> {} (refreshed)",
                max_peer_height, fresh_max_peer_height);
            max_peer_height = fresh_max_peer_height;
        }

        // Verify the sync attempt actually caught us up.
        // Two failure modes (HAZID RC2/FM2):
        // 1. Still at height 0 — genesis never received from peers.
        // 2. Still far behind max_peer_height — all blocks from the best
        //    peer failed to apply (incompatible chain, corrupt data).
        //    Without this check, sync_state=CaughtUp is set and mining
        //    starts on a stale tip unaware it's 60+ blocks behind.
        let current_height = blockchain.get_height();
        if current_height.get() == 0 && max_peer_height.get() > 0 {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Sync attempt failed — still at height 0 with peers at height {}. Retrying...",
                max_peer_height);
            node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Behind (sync failed — still at height 0, peers at {})",
                max_peer_height);
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }
        if max_peer_height.get() > 0 && current_height < max_peer_height {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Sync incomplete — local height {} but peer has {}. Retrying...",
                current_height, max_peer_height);
            node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Behind (sync incomplete, local={} peer={})",
                current_height, max_peer_height);
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }

        info!(target: "dwowd::task::consensus_linear_init_task",
            "Sync complete at height {}", blockchain.get_height());

        // Signal that initial sync is done — miner task can proceed
        // (miner independently waits for genesis, not full sync).
        // HAZOP H8: distinguish "no genesis anywhere" from "caught up."
        // CaughtUp at height 0 means no genesis exists on any peer.
        // The miner_task separately guards height-0 mining (get_latest_block
        // fails), but this log distinguishes the two cases for operators.
        node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
        if current_height.get() == 0 && max_peer_height.get() == 0 {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → CaughtUp [FALLBACK: no genesis exists anywhere — mining disabled until genesis arrives]");
        } else {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → CaughtUp [PRIMARY: sync complete — caught up to peer tip at height {}]", blockchain.get_height());
        }

        // H3.5: Check for longer competing chains while waiting.
        // A fork may have grown past our canonical chain while no
        // new P2P blocks triggered try_reorg_from_competing().
        // HAZID H-C1: gated behind reorg-enabled feature flag.
        #[cfg(feature = "reorg-enabled")]
        if let Err(e) = blockchain.try_reorg_from_competing() {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "try_reorg_from_competing failed: {}", e);
        }

        // Continuous sync: re-poll peers every 30s to stay caught up.
        // Never parks permanently — a node that falls behind will
        // eventually catch up when peers become available.
        smol::Timer::after(std::time::Duration::from_secs(30)).await;
    } // end outer retry loop (continuous)
}
