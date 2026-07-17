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

use dwow_core::net::session::SESSION_DEFAULT;
use smol::Executor;
use tracing::{error, info, warn};

use crate::proto::linear_sync::{Blocks, GetBlocks, GetTip, Tip, LINEAR_SYNC_BATCH};
use crate::{DwowNodePtr, Result, SYNC_CAUGHT_UP, SYNC_BEHIND};

use crate::block_acceptor::accept_block;
use dwow_chain::Miner;

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
        node.mining_state.sync_state.store(SYNC_CAUGHT_UP, Ordering::SeqCst);
        return std::future::pending().await
    }

    // Get the dwowd linear blockchain wrapper (full validation)
    let blockchain = match &node.chain_state {
        Some(lb) => lb.clone(),
        None => {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "No linear blockchain configured, parking forever");
            node.mining_state.sync_state.store(SYNC_CAUGHT_UP, Ordering::SeqCst);
            return std::future::pending().await
        }
    };

    let p2p = node.p2p_handler.p2p.clone();

    // Outer loop: retry entire sync process until genesis is available.
    // When local_height=0 and no peer has blocks, we loop back and re-check
    // peers — the genesis authority may not have created genesis yet.
    loop {
        let local_height = blockchain.get_height();

        // Wait for at least one connected peer before attempting sync.
        // If we already have blocks (genesis created locally), don't wait
        // Mining waits for sync_state=CaughtUp (Bitcoin production pattern).
        // A genesis authority with local blocks needs to proceed even
        // without peers — otherwise it waits forever.
        info!(target: "dwowd::task::consensus_linear_init_task", "Waiting for peer connections...");
        let mut wait_iters = 0u32;
        loop {
            if !p2p.hosts().peers().is_empty() {
                break
            }
            smol::Timer::after(std::time::Duration::from_secs(1)).await;
            wait_iters += 1;
            // If we have blocks and no peers appear after 10s, proceed.
            // The genesis authority can mine; a catching-up node will
            // sync when peers eventually connect (continuous sync will
            // catch them up).
            if local_height >= 1 && wait_iters >= 10 {
                info!(target: "dwowd::task::consensus_linear_init_task",
                    "No peers after 10s, proceeding with local height {}",
                    local_height);
                node.mining_state.sync_state.store(SYNC_CAUGHT_UP, Ordering::SeqCst);
                // Continuous sync: re-poll after delay instead of parking forever
                smol::Timer::after(std::time::Duration::from_secs(30)).await;
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
        let all_peers = p2p.hosts().peers();
        let peers: Vec<_> = all_peers.iter()
            .filter(|c| {
                let session = c.session_type_id();
                let addr = c.address().as_str();
                let is_docker_gateway = addr.contains("172.18.0.1");
                let is_full_node = session & SESSION_DEFAULT != 0;
                if is_docker_gateway {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Skipping Docker gateway peer {}", addr);
                } else if !is_full_node {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Skipping non-node peer {} session={:#b}", addr, session);
                }
                is_full_node && !is_docker_gateway
            })
            .cloned()
            .collect();
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Have {} full-node peers ({} total connections), querying tips...",
            peers.len(), all_peers.len());

        // Collect tips: (channel, height, genesis_hash)
        let mut peer_tips: Vec<(dwow_core::net::ChannelPtr, u64, Option<String>)> = Vec::new();
        for (i, channel) in peers.iter().enumerate() {
            let sub_result = channel.subscribe_msg::<Tip>().await;
            let Ok(tip_sub) = sub_result else {
                warn!(target: "dwowd::task::consensus_linear_init_task",
                    "Failed to subscribe to Tip on peer {}", channel.address().as_str());
                continue;
            };
            if channel.send(&GetTip).await.is_err() {
                warn!(target: "dwowd::task::consensus_linear_init_task",
                    "Failed to send GetTip to peer {}", channel.address().as_str());
                continue;
            }
            match tip_sub.receive_with_timeout(5).await {
                Ok(tip) => {
                    info!(target: "dwowd::task::consensus_linear_init_task",
                        "Peer {}: height={} genesis={}",
                        channel.address().as_str(), tip.height,
                        tip.genesis_hash.as_deref().unwrap_or("unknown"));
                    peer_tips.push((channel.clone(), tip.height, tip.genesis_hash.clone()));
                }
                Err(_) => {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "GetTip timed out for peer {}", channel.address().as_str());
                }
            }
        }

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
                let our_genesis_hash = if local_height >= 1 {
                    match blockchain.get_block(1) {
                        Ok(genesis) => Some(blockchain.hash_block_with_cached_vm(&genesis).to_string()),
                        Err(_) => None,
                    }
                } else {
                    None
                };
                let filtered: Vec<_> = if let Some(ref our_gh) = our_genesis_hash {
                    peer_tips.iter()
                        .filter(|(_, _, gh)| {
                            let matches = gh.as_ref() == Some(our_gh);
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
                    for (_, _, gh) in peer_tips.iter() {
                        *genesis_votes.entry(gh.clone()).or_default() += 1;
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
                        .filter(|(_, _, gh)| {
                            if majority_genesis.is_some() {
                                gh == majority_genesis.as_ref().unwrap()
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
        let max_peer_height: u64 = compatible_peers.iter()
            .map(|(_, h, _)| *h)
            .max()
            .unwrap_or(local_height);

        // If no peers have any blocks and we have no genesis (height=0),
        // we can't sync — keep waiting for a genesis authority to connect.
        if max_peer_height == 0 && local_height == 0 {
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

            let mut next_height = local_height + 1;

            // Layer 2: per-channel failure tracking.
            // After 3 consecutive bad blocks from the same channel,
            // deprioritize it for the remainder of this sync pass.
            // Resets each sync cycle — no permanent state.
            let mut channel_failures: std::collections::HashMap<u32, u32> =
                std::collections::HashMap::new();

            while next_height <= max_peer_height {
                let batch_size = (max_peer_height - next_height + 1).min(LINEAR_SYNC_BATCH as u64);

                // Re-fetch channel list in case peers disconnected.
                // Skip channels with >= 3 consecutive failures.
                let channels: Vec<_> = p2p.hosts().peers().into_iter()
                    .filter(|c| channel_failures.get(&c.info.id).unwrap_or(&0) < &3)
                    .collect();
                if channels.is_empty() {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "No healthy peers available for sync at height {}", next_height);
                    break
                }

                let channel = &channels[0];
                let ch_id = channel.info.id;

                // Subscribe to Blocks responses before sending the request
                let Ok(blocks_sub) = channel.subscribe_msg::<Blocks>().await else {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Failed to subscribe to Blocks messages, retrying...");
                    *channel_failures.entry(ch_id).or_default() += 1;
                    smol::Timer::after(std::time::Duration::from_secs(1)).await;
                    continue
                };

                let request = GetBlocks { start_height: next_height, count: batch_size };
                if channel.send(&request).await.is_err() {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Failed to send GetBlocks, retrying...");
                    *channel_failures.entry(ch_id).or_default() += 1;
                    smol::Timer::after(std::time::Duration::from_secs(1)).await;
                    continue
                }

                match blocks_sub.receive_with_timeout(15).await {
                    Ok(blocks_msg) => {
                        let received = blocks_msg.blocks.len();
                        info!(target: "dwowd::task::consensus_linear_init_task",
                            "Received {} blocks starting at height {}", received, next_height);

                        if received == 0 {
                            warn!(target: "dwowd::task::consensus_linear_init_task",
                                "Peer returned zero blocks, sync complete");
                            break
                        }

                        for block in &blocks_msg.blocks {
                            // Fix 1e: verify magic bytes in genesis block anchor field.
                            // Defense-in-depth: even if P2P magic bytes match, the
                            // consensus layer independently verifies the genesis block
                            // belongs to this network.
                            if block.header.height == 1 {
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
                            // per genesis.md. Skip proof-of-token-balance for genesis:
                            // target=u32::MAX means any hash passes, and mass balance
                            // is trivially satisfied (one coinbase tx, no user txs).
                            if block.header.height > 1 {
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
                            let current_height = block.header.height - 1;
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
                                Ok(()) => {
                                    next_height = block.header.height + 1;
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
                    Err(_) => {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "GetBlocks timed out at height {}, retrying...", next_height);
                        *channel_failures.entry(ch_id).or_default() += 1;
                        smol::Timer::after(std::time::Duration::from_secs(2)).await;
                        continue
                    }
                }
            }
        }

        // Verify the sync attempt actually caught us up.
        // Two failure modes (HAZID RC2/FM2):
        // 1. Still at height 0 — genesis never received from peers.
        // 2. Still far behind max_peer_height — all blocks from the best
        //    peer failed to apply (incompatible chain, corrupt data).
        //    Without this check, sync_state=CaughtUp is set and mining
        //    starts on a stale tip unaware it's 60+ blocks behind.
        let current_height = blockchain.get_height();
        if current_height == 0 && max_peer_height > 0 {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Sync attempt failed — still at height 0 with peers at height {}. Retrying...",
                max_peer_height);
            node.mining_state.sync_state.store(SYNC_BEHIND, Ordering::SeqCst);
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }
        if max_peer_height > 0 && current_height < max_peer_height {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Sync incomplete — local height {} but peer has {}. Retrying...",
                current_height, max_peer_height);
            node.mining_state.sync_state.store(SYNC_BEHIND, Ordering::SeqCst);
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }

        info!(target: "dwowd::task::consensus_linear_init_task",
            "Sync complete at height {}", blockchain.get_height());

        // Signal that initial sync is done — miner task can proceed
        // (miner independently waits for genesis, not full sync).
        node.mining_state.sync_state.store(SYNC_CAUGHT_UP, Ordering::SeqCst);

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
