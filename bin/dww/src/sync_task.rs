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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use dwow_core::net::{
    metering::MeteringConfiguration,
    Message, P2pPtr,
};
use crate::wallet_error::Result;
use dwow_chain::Block;
use dwow_serial::{AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, FutAsyncReadExt, FutAsyncWriteExt};

use crate::DwwPtr;

/// Fixed batch size for GetBlocks requests — matches dwowd LINEAR_SYNC_BATCH
const LINEAR_SYNC_BATCH: u64 = 20;

// ============================================================================
// Message Types — wire-compatible with dwowd (serde_json + varint framing)
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

// Register as P2P messages so dwow_core::net channels can send/receive them
dwow_core::impl_p2p_message!(
    GetBlocks, "lineargetblocks", 20 * 1024 * 1024, 10,
    dwow_core::net::metering::MeteringConfiguration { threshold: 20, sleep_step: 500, expiry_time: dwow_core::util::time::NanoTimestamp::from_secs(5) }
);
dwow_core::impl_p2p_message!(
    Blocks, "linearblocks", 20 * 1024 * 1024, 10,
    dwow_core::net::metering::MeteringConfiguration { threshold: 20, sleep_step: 500, expiry_time: dwow_core::util::time::NanoTimestamp::from_secs(5) }
);
dwow_core::impl_p2p_message!(
    GetTip, "lineargettip", 1024, 5,
    dwow_core::net::metering::MeteringConfiguration { threshold: 10, sleep_step: 500, expiry_time: dwow_core::util::time::NanoTimestamp::from_secs(5) }
);
dwow_core::impl_p2p_message!(
    Tip, "lineartip", 1024, 5,
    dwow_core::net::metering::MeteringConfiguration { threshold: 10, sleep_step: 500, expiry_time: dwow_core::util::time::NanoTimestamp::from_secs(5) }
);

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
// Sync loop — uses dwow_core::net P2P channels
// ============================================================================

/// Run the wallet sync loop using dwow_core::net P2P channels.
///
/// Flow:
///   1. Wait for connected peers (hostlist Gold/White entries with channels)
///   2. For each connected channel, register sync dispatchers, send GetTip
///   3. Collect Tip responses, update highest_peer_tip
///   4. While local < peer_tip: send GetBlocks to best peer, insert blocks
///   5. Repeat every 10 seconds
pub async fn run_wallet_sync(
    _p2p: P2pPtr,
    dww: DwwPtr,
    highest_peer_tip: Arc<HighestPeerTip>,
) -> Result<()> {
    eprintln!("[sync] Sync task started");
    info!(target: "dww::wallet::sync", "Wallet sync task running — P2p handles peer discovery");

    loop {
        smol::Timer::after(Duration::from_secs(10)).await;

        // Phase 1: Check peer connectivity
        let dww_r = dww.read().await;
        let local = dww_r.wallet.chain_height().map(|h| h).unwrap_or(0);
        let peer_count = dww_r.p2p.as_ref()
            .map(|p| p.hosts().peers().len())
            .unwrap_or(0);
        let p2p_opt = dww_r.p2p.clone();

        eprintln!("[sync] Tick: local={} peers={}", local, peer_count);
        info!(target: "dww::wallet::sync",
            "Sync tick: local_height={}, peer_count={}", local, peer_count);

        // Wait for peers — seed() in init_p2p() handles initial connection.
        // Mining node consensus task polls the same way (consensus_linear.rs:98).
        if peer_count == 0 {
            continue;
        }

        // Phase 2: Discover peer tips via GetTip/Tip
        let p2p = match p2p_opt {
            Some(ref p) => p.clone(),
            None => continue,
        };

        let channel_list = p2p.hosts().peers();

        let mut best_tip: u64 = 0;
        let mut tip_votes: std::collections::BTreeMap<String, (u64, u32)> =
            std::collections::BTreeMap::new();
        for ch in &channel_list {
            // Ensure dispatchers exist for sync message types
            ch.add_dispatch::<GetTip>().await;
            ch.add_dispatch::<Tip>().await;

            let tip_sub = match ch.subscribe_msg::<Tip>().await {
                Ok(s) => s,
                Err(_) => continue,
            };

            if ch.send(&GetTip).await.is_err() {
                continue;
            }

            // Wait for Tip with 5s timeout
            let tip_result = smol::future::or(
                async { tip_sub.receive().await },
                async {
                    smol::Timer::after(Duration::from_secs(5)).await;
                    Err(dwow_core::Error::ChannelTimeout)
                },
            ).await;

            if let Ok(tip) = tip_result {
                debug!(target: "dww::wallet::sync",
                    "Peer tip: height={}", tip.height);
                highest_peer_tip.set_max(tip.height);
                if tip.height > best_tip {
                    best_tip = tip.height;
                }
                // Track hash votes at each height for reorg detection
                if !tip.hash.is_empty() {
                    let entry = tip_votes.entry(tip.hash.clone())
                        .or_insert((tip.height, 0));
                    entry.1 += 1;
                }
            }
        }

        if best_tip <= local {
            // ── Reorg detection ──────────────────────────────────────
            // We're synced. Compare the majority tip hash with our
            // last known tip hash. A change at the same height
            // indicates a chain fork — the old chain was reorganized.
            let mut reorg_trigger = false;
            if local > 0 && !tip_votes.is_empty() {
                let majority = tip_votes.iter()
                    .max_by_key(|(_, (_, count))| *count)
                    .map(|(hash, (height, count))| (hash.clone(), *height, *count));
                if let Some((tip_hash, tip_height, votes)) = majority {
                    {
                        let mut last_hash = dww_r.last_synced_tip_hash.lock().await;
                        if let Some(ref last) = *last_hash {
                            if last != &tip_hash && tip_height == local {
                                reorg_trigger = true;
                                eprintln!(
                                    "[sync] REORG DETECTED: tip hash changed at height {} \
                                     (was {}, now {}) with {}/{} peer votes — triggering auto-reset",
                                    local, last, tip_hash, votes, tip_votes.len()
                                );
                                warn!(target: "dww::wallet::sync",
                                    "REORG DETECTED: tip hash changed at height {} \
                                     (was {}, now {}) — triggering auto-reset",
                                    local, last, tip_hash);
                            }
                        }
                        *last_hash = Some(tip_hash);
                    }
                }
            }

            debug!(target: "dww::wallet::sync",
                "Already at tip: local={}, peer={}", local, best_tip);

            if reorg_trigger {
                drop(dww_r);
                let dww_w = dww.write().await;
                let mut output = vec![];
                if let Err(e) = dww_w.reset(&mut output) {
                    error!(target: "dww::wallet::sync",
                        "Auto-reset after reorg failed: {e}");
                } else {
                    info!(target: "dww::wallet::sync",
                        "Auto-reset after reorg complete. Wallet will rescan from genesis.");
                }
                for line in &output {
                    eprintln!("[sync] reset: {line}");
                }
                continue;
            }
            continue;
        }

        // Phase 3: Fetch missing blocks via GetBlocks/Blocks
        eprintln!("[sync] Behind tip: local={} peer_tip={} — fetching blocks", local, best_tip);
        info!(target: "dww::wallet::sync",
            "Behind tip: local={}, peer={} — fetching blocks", local, best_tip);

        let mut next_height = local + 1;
        'fetch: for ch in &channel_list {
            if next_height > best_tip {
                break 'fetch;
            }

            ch.add_dispatch::<GetBlocks>().await;
            ch.add_dispatch::<Blocks>().await;

            let blocks_sub = match ch.subscribe_msg::<Blocks>().await {
                Ok(s) => s,
                Err(_) => continue,
            };

            let batch_size = LINEAR_SYNC_BATCH.min(best_tip - next_height + 1);
            let request = GetBlocks { start_height: next_height, count: batch_size };

            if ch.send(&request).await.is_err() {
                continue;
            }

            // Wait for Blocks with 30s timeout
            let blocks_result = smol::future::or(
                async { blocks_sub.receive().await },
                async {
                    smol::Timer::after(Duration::from_secs(30)).await;
                    Err(dwow_core::Error::ChannelTimeout)
                },
            ).await;

            let blocks_msg = match blocks_result {
                Ok(b) => b,
                Err(_) => continue,
            };

            let dww_r = dww.read().await;
            for block in &blocks_msg.blocks {
                let height = block.header.height.get();
                match dww_r.insert_synced_block(block) {
                    Ok(()) => {
                        info!(target: "dww::wallet::sync",
                            "Inserted block {}", height);
                        next_height = height + 1;
                    }
                    Err(e) => {
                        error!(target: "dww::wallet::sync",
                            "Failed to insert block {}: {e}", height);
                        break 'fetch;
                    }
                }
            }
            drop(dww_r);
        }

        info!(target: "dww::wallet::sync",
            "Sync cycle complete: local_height={}", next_height - 1);
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
