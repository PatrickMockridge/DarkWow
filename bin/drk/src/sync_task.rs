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

//! Wallet P2P chain sync task.
//!
//! Wire-compatible with dwowd's linear sync (bin/dwowd/src/proto/linear_sync.rs).
//! Same messages, same flow: GetTip → Tip, GetBlocks → Blocks.
//!
//! Follows the same pattern as dwowd's consensus_linear_init_task
//! (bin/dwowd/src/task/consensus_linear.rs).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use dwow_core::{
    impl_p2p_message,
    net::{
        metering::MeteringConfiguration, session::SESSION_DEFAULT, Message, P2pPtr,
    },
    util::time::NanoTimestamp,
    Error, Result,
};
use dwow_chain::Block;
use dwow_serial::{AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, FutAsyncReadExt, FutAsyncWriteExt};

use crate::DwwPtr;

/// Fixed batch size for GetBlocks requests — matches dwowd LINEAR_SYNC_BATCH
const LINEAR_SYNC_BATCH: u64 = 20;

const LINEAR_SYNC_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 20, sleep_step: 500,
    expiry_time: NanoTimestamp::from_secs(5),
};

// ============================================================================
// Message Types — wire-compatible with dwowd's linear_sync protocol
// Same serialization format (serde_json + varint length prefix).
// Same P2P message names ("lineargettip", "lineartip", "lineargetblocks", "linearblocks").
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlocks {
    pub start_height: u64,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blocks {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetTip;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tip {
    pub height: u64,
    pub hash: String,
}

macro_rules! impl_json_message_codec {
    ($ty:ty) => {
        #[async_trait]
        impl AsyncEncodable for $ty {
            async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
                let bytes = serde_json::to_vec(self)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let mut len = 0;
                len += varint_encode(bytes.len(), s).await?;
                len += s.write(&bytes).await?;
                Ok(len)
            }
        }
        #[async_trait]
        impl AsyncDecodable for $ty {
            async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
                let len = varint_decode(d).await?;
                let mut buf = vec![0u8; len];
                d.read_exact(&mut buf).await?;
                serde_json::from_slice(&buf)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            }
        }
    };
}

impl_json_message_codec!(GetBlocks);
impl_json_message_codec!(Blocks);
impl_json_message_codec!(GetTip);
impl_json_message_codec!(Tip);

const MAX_SMALL_JSON_BYTES: u64 = 256;
const MAX_BLOCKS_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TIP_BYTES: u64 = 256;

impl_p2p_message!(GetBlocks, "lineargetblocks", MAX_SMALL_JSON_BYTES, 1, LINEAR_SYNC_METERING_CONFIGURATION);
impl_p2p_message!(Blocks, "linearblocks", MAX_BLOCKS_BYTES, 1, LINEAR_SYNC_METERING_CONFIGURATION);
impl_p2p_message!(GetTip, "lineargettip", 0, 1, LINEAR_SYNC_METERING_CONFIGURATION);
impl_p2p_message!(Tip, "lineartip", MAX_TIP_BYTES, 1, LINEAR_SYNC_METERING_CONFIGURATION);

// ============================================================================
// Varint encoding
// ============================================================================

async fn varint_encode<W: AsyncWrite + Unpin + Send>(mut value: usize, s: &mut W) -> std::io::Result<usize> {
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { byte |= 0x80; }
        len += FutAsyncWriteExt::write(s, &[byte]).await?;
        if value == 0 { break; }
    }
    Ok(len)
}

async fn varint_decode<R: AsyncRead + Unpin + Send>(d: &mut R) -> std::io::Result<usize> {
    let mut result = 0;
    let mut shift = 0;
    loop {
        let mut buf = [0u8; 1];
        FutAsyncReadExt::read_exact(d, &mut buf).await?;
        let byte = buf[0];
        result |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 { break; }
        shift += 7;
    }
    Ok(result)
}

// ============================================================================
// HighestPeerTip — atomic, monotonic
// ============================================================================

/// Highest peer tip seen. Updated on each Tip response.
pub struct HighestPeerTip(pub AtomicU64);

impl HighestPeerTip {
    pub fn new() -> Self { Self(AtomicU64::new(0)) }

    pub fn get(&self) -> u64 { self.0.load(Ordering::Relaxed) }

    pub fn set_max(&self, height: u64) {
        let _ = self.0.fetch_update(Ordering::Release, Ordering::Relaxed, |c| {
            if height > c { Some(height) } else { None }
        });
    }
}

/// Run the wallet sync loop: query peers, fetch blocks, scan for capabilities.
/// This is spawned as a background task after init_p2p().
pub async fn run_wallet_sync(
    p2p: P2pPtr,
    dww: DwwPtr,
    highest_peer_tip: Arc<HighestPeerTip>,
) -> Result<()> {
    info!(target: "drk::wallet::sync", "Wallet sync task starting...");

    let mut dispatchers_registered = false;

    loop {
        smol::Timer::after(std::time::Duration::from_secs(2)).await;

        let local_height = {
            let dww_r = dww.read().await;
            dww_r.chain.get_height().unwrap_or(0)
        };

        // Get connected peers (same pattern as dwowd consensus_linear_init_task)
        let all_peers = p2p.hosts().peers();
        let peers: Vec<_> = all_peers.iter()
            .filter(|c| {
                let session = c.session_type_id();
                let addr = c.address().as_str();
                let is_docker_gateway = addr.contains("172.18.0.1");
                let is_full_node = session & SESSION_DEFAULT != 0;
                is_full_node && !is_docker_gateway
            })
            .cloned()
            .collect();

        if peers.is_empty() {
            debug!(target: "drk::wallet::sync",
                "No peers available. Waiting for connections...");
            continue;
        }

        // HAZOP #3: Register dispatchers ONCE, not every loop iteration.
        // Re-registering every 10s causes metering inflation (60+ dispatchers after 10 min).
        if !dispatchers_registered {
            for channel in &peers {
                let subsys = channel.message_subsystem();
                subsys.add_dispatch::<dwow_core::tx::Transaction>().await;
                subsys.add_dispatch::<Tip>().await;
                subsys.add_dispatch::<Blocks>().await;
            }
            dispatchers_registered = true;
        }

        // Phase 1: Query all peers for their chain tip
        let mut max_peer_height: u64 = local_height;

        for channel in &peers {
            let Ok(tip_sub) = channel.subscribe_msg::<Tip>().await else {
                continue;
            };

            if channel.send(&GetTip).await.is_err() {
                continue;
            }

            match tip_sub.receive_with_timeout(5).await {
                Ok(tip) => {
                    debug!(target: "drk::wallet::sync",
                        "Peer tip: height={}, hash={}", tip.height, tip.hash);
                    if tip.height > max_peer_height {
                        max_peer_height = tip.height;
                    }
                    highest_peer_tip.set_max(tip.height);
                }
                Err(_) => continue,
            }
        }

        // Phase 2: Fetch missing blocks
        if max_peer_height > local_height {
            info!(target: "drk::wallet::sync",
                "Behind: local={}, peer={}. Syncing...",
                local_height, max_peer_height);

            let mut next_height = local_height + 1;

            while next_height <= max_peer_height {
                let batch_size = (max_peer_height - next_height + 1).min(LINEAR_SYNC_BATCH);

                // Find a peer to request from
                let channel = match peers.first() {
                    Some(c) => c,
                    None => break,
                };

                let Ok(blocks_sub) = channel.subscribe_msg::<Blocks>().await else {
                    // Do NOT advance next_height on subscribe failure.
                    // HAZOP #1: advancing past the gap creates permanent chain gaps
                    // where caps in skipped blocks are never discovered.
                    warn!(target: "drk::wallet::sync",
                        "Failed to subscribe to Blocks, retrying same height...");
                    continue;
                };

                let request = GetBlocks {
                    start_height: next_height,
                    count: batch_size,
                };

                if channel.send(&request).await.is_err() {
                    warn!(target: "drk::wallet::sync",
                        "Failed to send GetBlocks, retrying...");
                    continue;
                }

                match blocks_sub.receive_with_timeout(10).await {
                    Ok(response) => {
                        let count = response.blocks.len();
                        for block in &response.blocks {
                            let mut dww_w = dww.write().await;
                            match dww_w.insert_synced_block(block) {
                                Ok(()) => {
                                    // Scan block for capabilities immediately
                                    if let Ok(mut scan_cache) = dww_w.scan_cache() {
                                        if let Err(e) = dww_w.scan_block_linear(
                                            &mut scan_cache, block,
                                        ) {
                                            error!(target: "drk::wallet::sync",
                                                "Scan failed for block {}: {} — stopping batch to prevent Merkle tree corruption",
                                                block.header.height, e);
                                            break; // Don't advance past failed block
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(target: "drk::wallet::sync",
                                        "Failed to insert block {}: {}",
                                        block.header.height, e);
                                }
                            }
                        }
                        debug!(target: "drk::wallet::sync",
                            "Synced {} blocks starting at height {}",
                            count, next_height);
                        next_height += count as u64;
                    }
                    Err(_) => {
                        warn!(target: "drk::wallet::sync",
                            "GetBlocks timed out at height {}, retrying...",
                            next_height);
                        continue;
                    }
                }
            }

            let new_height = { dww.read().await.chain.get_height().unwrap_or(0) };
            info!(target: "drk::wallet::sync",
                "Sync complete: height {} → {}", local_height, new_height);
        } else {
            debug!(target: "drk::wallet::sync",
                "Synced: local={}, peer={}", local_height, max_peer_height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highest_peer_tip_initial() {
        let tip = HighestPeerTip::new();
        assert_eq!(tip.get(), 0);
    }

    #[test]
    fn test_highest_peer_tip_monotonic() {
        let tip = HighestPeerTip::new();
        tip.set_max(42);
        assert_eq!(tip.get(), 42);
        tip.set_max(10); // lower — should not decrease
        assert_eq!(tip.get(), 42);
        tip.set_max(100);
        assert_eq!(tip.get(), 100);
    }
}
