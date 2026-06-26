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
//! Wire-compatible with dwowd's linear sync. Same messages, same flow:
//! GetTip → Tip, GetBlocks → Blocks. Uses wallet-owned P2P (p2p_wallet).
//! Zero dependency on dwow_core::net.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::p2p_wallet::{connect_peer, P2pWalletPtr};
use crate::wallet_error::Result;
use dwow_chain::Block;
use dwow_serial::{AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, FutAsyncReadExt, FutAsyncWriteExt};

use crate::DwwPtr;

/// Fixed batch size for GetBlocks requests — matches dwowd LINEAR_SYNC_BATCH
const LINEAR_SYNC_BATCH: u64 = 20;

// ============================================================================
// Message Types — wire-compatible with dwowd
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

// Async codec for serde_json + varint framing
use async_trait::async_trait;

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

// ============================================================================
// Varint encoding (async — used by codec)
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

// ============================================================================
// Sync loop — wallet-owned P2P, no dwow_core::net
// ============================================================================

/// Run the wallet sync loop. Uses wallet-owned P2P (p2p_wallet.rs).
pub async fn run_wallet_sync(
    p2p: P2pWalletPtr,
    dww: DwwPtr,
    highest_peer_tip: Arc<HighestPeerTip>,
) -> Result<()> {
    info!(target: "drk::wallet::sync", "Wallet sync task starting...");

    loop {
        smol::Timer::after(Duration::from_secs(2)).await;

        let local_height = {
            let dww_r = dww.read().await;
            dww_r.chain.get_height().unwrap_or(0)
        };

        // Get connected peer addresses
        let peer_addrs = {
            let p2p_r = p2p.read().expect("p2p read lock poisoned");
            p2p_r.peers()
        };

        if peer_addrs.is_empty() {
            debug!(target: "drk::wallet::sync",
                "No peers available. Waiting for connections...");
            continue;
        }

        // Phase 1: Query peers for chain tip
        let mut max_peer_height: u64 = local_height;
        let mut tip_votes: HashMap<String, usize> = HashMap::new();

        for addr in &peer_addrs {
            // Connect to peer (if not already connected, PeerConnection is created fresh)
            let (tls_config, magic_bytes, datastore, localnet) = {
                let p2p_r = p2p.read().expect("p2p read lock poisoned");
                (p2p_r.tls_config.clone(), p2p_r.magic_bytes, p2p_r.config.datastore.clone(), p2p_r.config.localnet)
            };
            let mut conn = match connect_peer(
                addr,
                &tls_config,
                magic_bytes,
                local_height,
                datastore.map(|s| std::path::PathBuf::from(s)),
                localnet,
            ).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            if conn.send("lineargettip", &GetTip).await.is_err() {
                continue;
            }

            match smol::future::or(
                async {
                    let (name, payload) = conn.recv().await?;
                    if name == "lineartip" {
                        let tip: Tip = serde_json::from_slice(&payload)
                            .map_err(|e| crate::wallet_error::Error::Custom(e.to_string()))?;
                        Ok(tip)
                    } else {
                        Err(crate::wallet_error::Error::Custom(format!("unexpected msg: {name}")))
                    }
                },
                async {
                    smol::Timer::after(Duration::from_secs(5)).await;
                    Err(crate::wallet_error::Error::Custom("tip timeout".into()))
                },
            ).await {
                Ok(tip) => {
                    debug!(target: "drk::wallet::sync",
                        "Peer tip: height={}, hash={}", tip.height, tip.hash);
                    if tip.height > max_peer_height {
                        max_peer_height = tip.height;
                    }
                    highest_peer_tip.set_max(tip.height);
                    *tip_votes.entry(tip.hash.clone()).or_default() += 1;
                }
                Err(_) => continue,
            }
        }

        if tip_votes.len() > 1 {
            warn!(target: "drk::wallet::sync",
                "PEERS DISAGREE ON TIP: {} distinct hashes.", tip_votes.len());
        }

        let majority_hash = tip_votes.iter().max_by_key(|(_, count)| *count).map(|(h, _)| h.clone());
        if let Some(ref current_tip) = majority_hash {
            let dww_read = dww.read().await;
            let mut last_hash = dww_read.last_synced_tip_hash.lock().await;
            if let Some(ref last) = *last_hash {
                if last != current_tip && max_peer_height == local_height {
                    warn!(target: "drk::wallet::sync",
                        "REORG: tip hash changed at height {} (was {}, now {})",
                        local_height, last, current_tip);
                    let mut output = vec![];
                    if let Err(e) = dww_read.reset(&mut output) {
                        error!(target: "drk::wallet::sync", "Auto-rescan failed: {}", e);
                    }
                }
            }
            *last_hash = Some(current_tip.clone());
        }

        // Phase 2: Fetch missing blocks
        if max_peer_height > local_height {
            info!(target: "drk::wallet::sync",
                "Behind: local={}, peer={}. Syncing...", local_height, max_peer_height);

            let mut next_height = local_height + 1;

            while next_height <= max_peer_height {
                let batch_size = (max_peer_height - next_height + 1).min(LINEAR_SYNC_BATCH);

                // Connect to a peer for this batch
                let addr = match peer_addrs.first() {
                    Some(a) => a.clone(),
                    None => break,
                };

                let (tls_config, datastore, localnet, magic_bytes) = {
                    let p2p_r = p2p.read().expect("p2p read lock poisoned");
                    (p2p_r.tls_config.clone(), p2p_r.config.datastore.clone(), p2p_r.config.localnet, p2p_r.config.magic_bytes)
                };
                let mut conn = match connect_peer(
                    &addr,
                    &tls_config,
                    magic_bytes,
                    local_height,
                    datastore.map(|s| std::path::PathBuf::from(s)),
                    localnet,
                ).await {
                    Ok(c) => c,
                    Err(_) => {
                        warn!(target: "drk::wallet::sync", "Failed to connect for GetBlocks");
                        continue;
                    }
                };

                let request = GetBlocks { start_height: next_height, count: batch_size };
                if conn.send("lineargetblocks", &request).await.is_err() {
                    warn!(target: "drk::wallet::sync", "GetBlocks send failed, retrying...");
                    continue;
                }

                match smol::future::or(
                    async {
                        let (name, payload) = conn.recv().await?;
                        if name == "linearblocks" {
                            let blocks: Blocks = serde_json::from_slice(&payload)
                                .map_err(|e| crate::wallet_error::Error::Custom(e.to_string()))?;
                            Ok(blocks)
                        } else {
                            Err(crate::wallet_error::Error::Custom(format!("unexpected: {name}")))
                        }
                    },
                    async {
                        smol::Timer::after(Duration::from_secs(10)).await;
                        Err(crate::wallet_error::Error::Custom("blocks timeout".into()))
                    },
                ).await {
                    Ok(response) => {
                        let count = response.blocks.len();
                        for block in &response.blocks {
                            let dww_r = dww.read().await;
                            // Scan BEFORE insert — if we crash after scan but
                            // before insert, the block is re-fetched and
                            // re-scanned. Scan and insert both use sled/SQLite
                            // internal concurrency — read lock is sufficient.
                            let scan_ok = if let Ok(mut scan_cache) = dww_r.scan_cache() {
                                dww_r.scan_block_linear(&mut scan_cache, block).is_ok()
                            } else {
                                false
                            };
                            if !scan_ok {
                                error!(target: "drk::wallet::sync",
                                    "Scan failed for block {} — aborting batch",
                                    block.header.height);
                                break;
                            }
                            match dww_r.insert_synced_block(block) {
                                Ok(()) => {}
                                Err(e) => {
                                    warn!(target: "drk::wallet::sync",
                                        "Failed to insert block {}: {}",
                                        block.header.height, e);
                                }
                            }
                        }
                        debug!(target: "drk::wallet::sync",
                            "Synced {} blocks starting at height {}", count, next_height);
                        next_height += count as u64;
                    }
                    Err(_) => {
                        warn!(target: "drk::wallet::sync",
                            "GetBlocks timed out at height {}, retrying...", next_height);
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
        tip.set_max(10);
        assert_eq!(tip.get(), 42);
        tip.set_max(100);
        assert_eq!(tip.get(), 100);
    }
}
