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
use crate::{DwowNodePtr, Result};

/// Auxiliary structure representing node consensus init task configuration.
#[derive(Clone)]
pub struct ConsensusInitTaskConfig {
    /// Skip syncing process and start node right away
    pub skip_sync: bool,
    /// Optional sync checkpoint height
    pub checkpoint_height: Option<u32>,
    /// Optional sync checkpoint hash
    pub checkpoint: Option<String>,
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
        node.mining_state.sync_complete.store(true, Ordering::SeqCst);
        return std::future::pending().await
    }

    // Get the dwowd linear blockchain wrapper (full validation)
    let blockchain = match &node.chain_state {
        Some(lb) => lb.clone(),
        None => {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "No linear blockchain configured, parking forever");
            node.mining_state.sync_complete.store(true, Ordering::SeqCst);
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
        // forever — a genesis authority needs sync_complete=true to mine.
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
            // sync when peers eventually connect (sync_complete can be
            // set true even if we're behind — mining just produces
            // stale blocks that get reorganized).
            if local_height >= 1 && wait_iters >= 10 {
                info!(target: "dwowd::task::consensus_linear_init_task",
                    "No peers after 10s, proceeding with local height {}",
                    local_height);
                node.mining_state.sync_complete.store(true, Ordering::SeqCst);
                return std::future::pending().await
            }
        }

        let local_height = blockchain.get_height();
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Connected to peers, local height: {}", local_height);

        // Query all peers for their best height.
        // Two-layer filter: only query channels that look like real dwowd nodes.
        // Layer 1: Docker bridge gateway is always infrastructure — log and skip.
        // Layer 2: Peer must have at least one SESSION_DEFAULT bit set.
        //          SESSION_DEFAULT covers INBOUND|OUTBOUND|MANUAL|SEED|DIRECT.
        //          Manual peers (SESSION_MANUAL=0b100) are real nodes added via
        //          PEER_ADDR config — they do handle GetTip/GetBlocks.
        let mut max_peer_height: u64 = local_height;

        let all_peers = p2p.hosts().peers();
        let peers: Vec<_> = all_peers.iter()
            .filter(|c| {
                let session = c.session_type_id();
                let addr = c.address().as_str();
                let is_docker_gateway = addr.contains("172.18.0.1");
                let is_full_node = session & SESSION_DEFAULT != 0;

                if is_docker_gateway {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Skipping Docker gateway peer {} (not a real node)", addr);
                } else if !is_full_node {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Skipping non-node peer {} session={:#b} (missing SESSION_DEFAULT)",
                        addr, session);
                }
                is_full_node && !is_docker_gateway
            })
            .cloned()
            .collect();
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Have {} full-node peers ({} total connections), querying tips...",
            peers.len(), all_peers.len());

        for (i, channel) in peers.iter().enumerate() {
            let peer_addr = channel.address().clone();
            let session = channel.session_type_id();
            info!(target: "dwowd::task::consensus_linear_init_task",
                "TRACE: querying peer {}/{} addr={} session={:?}",
                i + 1, peers.len(), peer_addr.as_str(), session);

            info!(target: "dwowd::task::consensus_linear_init_task",
                "TRACE: about to call subscribe_msg::<Tip>() on peer {}", i + 1);
            let sub_result = channel.subscribe_msg::<Tip>().await;
            info!(target: "dwowd::task::consensus_linear_init_task",
                "TRACE: subscribe_msg::<Tip>() returned on peer {}: {}",
                i + 1, if sub_result.is_ok() { "Ok" } else { "Err" });

            let Ok(tip_sub) = sub_result else {
                warn!(target: "dwowd::task::consensus_linear_init_task",
                    "Failed to subscribe to Tip messages on channel addr={}",
                    peer_addr.as_str());
                continue
            };

            if channel.send(&GetTip).await.is_err() {
                warn!(target: "dwowd::task::consensus_linear_init_task",
                    "Failed to send GetTip to channel");
                continue
            }

            match tip_sub.receive_with_timeout(5).await {
                Ok(tip) => {
                    info!(target: "dwowd::task::consensus_linear_init_task",
                        "Peer height: {} (hash: {})", tip.height, tip.hash);
                    if tip.height > max_peer_height {
                        max_peer_height = tip.height;
                    }
                }
                Err(_) => {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "GetTip timed out or failed for channel");
                    continue
                }
            }
        }

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

            while next_height <= max_peer_height {
                let batch_size = (max_peer_height - next_height + 1).min(LINEAR_SYNC_BATCH as u64);

                // Re-fetch channel list in case peers disconnected
                let channels = p2p.hosts().peers();
                if channels.is_empty() {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Lost all peers during sync at height {}", next_height);
                    break
                }

                let channel = &channels[0];

                // Subscribe to Blocks responses before sending the request
                let Ok(blocks_sub) = channel.subscribe_msg::<Blocks>().await else {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Failed to subscribe to Blocks messages, retrying...");
                    smol::Timer::after(std::time::Duration::from_secs(1)).await;
                    continue
                };

                let request = GetBlocks { start_height: next_height, count: batch_size };
                if channel.send(&request).await.is_err() {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Failed to send GetBlocks, retrying...");
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
                            if let Err(e) = crate::proof_of_token_balance::verify_proof_of_token_balance(block) {
                                tracing::warn!(
                                    target: "dwowd::task::consensus_linear",
                                    "Synced block at height {} failed proof-of-token-balance: {}",
                                    block.header.height, e
                                );
                                continue;
                            }
                            match blockchain.apply_block_with_uncles(block, &[]).await {
                                Ok(()) => {
                                    next_height = block.header.height + 1;
                                }
                                Err(e) => {
                                    error!(target: "dwowd::task::consensus_linear_init_task",
                                        "Failed to apply synced block at height {}: {}",
                                        block.header.height, e);
                                    // Continue with remaining blocks; the failed
                                    // block will be re-requested on next retry
                                }
                            }
                        }
                    }
                    Err(_) => {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "GetBlocks timed out at height {}, retrying...", next_height);
                        smol::Timer::after(std::time::Duration::from_secs(2)).await;
                        continue
                    }
                }
            }
        }

        // If we're still at height 0 after the sync attempt, genesis was not
        // received. This can happen if GetBlocks times out (peer not responding,
        // MAX_BYTES mismatch, network issue). Retry the outer loop instead of
        // declaring sync complete and parking forever at height 0.
        let current_height = blockchain.get_height();
        if current_height == 0 && max_peer_height > 0 {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Sync attempt failed — still at height 0 with peers at height {}. Retrying...",
                max_peer_height);
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }

        info!(target: "dwowd::task::consensus_linear_init_task",
            "Sync complete at height {}", blockchain.get_height());

        node.mining_state.sync_complete.store(true, Ordering::SeqCst);

        // Park forever — block production is triggered via RPC or stratum miner
        std::future::pending().await
    } // end outer retry loop
}
