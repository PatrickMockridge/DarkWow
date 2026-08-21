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
//! Syncs over the unified sync connection (`dwow_chain::sync_connection`),
//! the same code path the mining/observer node uses. The wallet dials its
//! configured peers directly — no hostlist, no seed discovery, no ManualSession
//! divergence. Every connection failure is logged (sync-hazop.md R1/R2/R3).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, error, info, warn};

use dwow_core::net::P2pPtr;
use dwow_sdk::blockchain::BlockHeight;

use crate::wallet_error::Result;
use crate::DwwPtr;

/// Fixed batch size for GetBlocks requests.
const LINEAR_SYNC_BATCH: u64 = 20;

/// Dial timeout for a single peer.
const DIAL_TIMEOUT: Duration = Duration::from_secs(15);

// ============================================================================
// HighestPeerTip — atomic, monotonic, BlockHeight-typed
// ============================================================================

/// Highest peer tip seen. Updated on each Tip response.
pub struct HighestPeerTip(AtomicU64);

impl HighestPeerTip {
    pub fn new() -> Self { Self(AtomicU64::new(0)) }

    /// Returns the highest peer tip as a BlockHeight.
    pub fn get(&self) -> BlockHeight {
        BlockHeight::new(self.0.load(Ordering::Relaxed))
    }

    /// Updates the highest peer tip if the given height exceeds the current value.
    #[allow(unused_results)]
    pub fn set_max(&self, height: BlockHeight) {
        let _ = self.0.fetch_update(Ordering::Release, Ordering::Relaxed, |c| {
            if height.get() > c { Some(height.get()) } else { None }
        });
    }
}

// ============================================================================
// Sync loop — unified sync connection (SyncPeer)
// ============================================================================

/// Run the wallet sync loop using the unified sync connection.
///
/// Flow:
///   1. Read local height + configured peers (magic) from `p2p_settings`.
///   2. Dial each peer via `SyncPeer::dial` (TCP+TLS + handshake, logged).
///   3. Request tips, update highest_peer_tip, detect reorg.
///   4. While local < peer_tip: request blocks in batches, insert them.
///   5. Repeat every 10 seconds.
pub async fn run_wallet_sync(
    _p2p: P2pPtr,
    dww: DwwPtr,
    highest_peer_tip: Arc<HighestPeerTip>,
) -> Result<()> {
    let zero_height = BlockHeight::new(0); // pre-genesis sentinel (G6)

    eprintln!("[sync] Sync task started");
    info!(target: "dww::wallet::sync", "Wallet sync task running");

    // D1: Stuck-sync watchdog — if we have peers but never advance past height 0.
    let mut stuck_ticks: u32 = 0;
    const STUCK_TICK_LIMIT: u32 = 6;

    loop {
        smol::Timer::after(Duration::from_secs(10)).await;

        // Phase 1: read local height + configured peers, then DROP the read lock.
        let (local, peer_urls, magic) = {
            let dww_r = dww.read().await;
            let local = match dww_r.wallet.chain_height() {
                Ok(h) => h,
                Err(e) => {
                    error!(target: "dww::wallet::sync",
                        "chain_height failed: {e} — skipping sync tick");
                    continue;
                }
            };
            let (peer_urls, magic) = match dww_r.p2p_settings.as_ref() {
                Some(cfg) => (
                    cfg.peers.iter()
                        .filter_map(|s| url::Url::parse(&s.url).ok())
                        .map(|mut u| {
                            // Dial the dedicated sync listener (peer + SYNC_PORT_OFFSET).
                            if let Some(port) = u.port() {
                                let _ = u.set_port(Some(port + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
                            }
                            u
                        })
                        .collect::<Vec<_>>(),
                    cfg.magic_bytes,
                ),
                None => (Vec::new(), [68, 82, 75, 87]),
            };
            (local, peer_urls, magic)
        };

        eprintln!("[sync] Tick: local={} peers={}", local.get(), peer_urls.len());
        info!(target: "dww::wallet::sync",
            "Sync tick: local_height={}, peer_count={}", local.get(), peer_urls.len());

        // D1: Stuck-sync watchdog
        if !peer_urls.is_empty() && local == zero_height {
            stuck_ticks += 1;
            if stuck_ticks >= STUCK_TICK_LIMIT {
                error!(target: "dww::wallet::sync",
                    "FATAL: Wallet has {} peers but zero chain height after {} consecutive ticks ({}s).",
                    peer_urls.len(), stuck_ticks, stuck_ticks * 10);
                eprintln!(
                    "[sync] FATAL: Wallet has {} peers but zero chain height after {} consecutive ticks ({}s).",
                    peer_urls.len(), stuck_ticks, stuck_ticks * 10);
            }
        } else if local > zero_height {
            stuck_ticks = 0; // Reset — we're making progress
        }

        if peer_urls.is_empty() {
            continue;
        }

        // Phase 2: dial peers via the unified sync connection (logged failures).
        let mut sync_peers = Vec::with_capacity(peer_urls.len());
        for url in &peer_urls {
            match dwow_chain::sync_connection::SyncPeer::dial(
                url.clone(), magic, None, DIAL_TIMEOUT,
            ).await {
                Ok(peer) => sync_peers.push(peer),
                Err(e) => {
                    warn!(target: "dww::wallet::sync", "dial {url} failed: {e}");
                }
            }
        }
        if sync_peers.is_empty() {
            continue;
        }

        // G4: best_tip is BlockHeight
        let mut best_tip = zero_height;
        let mut tip_timeouts: u32 = 0;
        let mut tip_votes: std::collections::BTreeMap<dwow_chain::sync_types::BlockHash, (BlockHeight, u32)> =
            std::collections::BTreeMap::new();
        for peer in &mut sync_peers {
            match peer.request_tip().await {
                Ok(tip) => {
                    debug!(target: "dww::wallet::sync", "Peer tip: height={}", tip.height.get());
                    highest_peer_tip.set_max(tip.height);
                    if tip.height > best_tip {
                        best_tip = tip.height;
                    }
                    // Track hash votes at each height for reorg detection
                    if !tip.hash.is_zero() {
                        let entry = tip_votes.entry(tip.hash.clone())
                            .or_insert((tip.height, 0));
                        entry.1 += 1;
                    }
                }
                Err(e) => {
                    tip_timeouts += 1;
                    if tip_timeouts % 3 == 0 {
                        warn!(target: "dww::wallet::sync",
                            "Tip request failed ({} consecutive failures): {e}", tip_timeouts);
                    }
                }
            }
        }

        // G7: BlockHeight Ord comparison
        if best_tip <= local {
            // ── Reorg detection ──────────────────────────────────────
            let mut reorg_trigger = false;
            if local > zero_height && !tip_votes.is_empty() {
                let majority = tip_votes.iter()
                    .max_by_key(|(_, (_, count))| *count)
                    .map(|(hash, (height, count))| (hash.clone(), *height, *count));
                if let Some((tip_hash, tip_height, votes)) = majority {
                    let dww_r = dww.read().await;
                    let mut last_hash = dww_r.last_synced_tip_hash.lock().await;
                    if let Some(ref last) = *last_hash {
                        if last != &tip_hash && tip_height == local {
                            reorg_trigger = true;
                            eprintln!(
                                "[sync] REORG DETECTED: tip hash changed at height {} \
                                 (was {}, now {}) with {}/{} peer votes — triggering auto-reset",
                                local.get(), last, tip_hash, votes, tip_votes.len()
                            );
                            warn!(target: "dww::wallet::sync",
                                "REORG DETECTED: tip hash changed at height {} \
                                 (was {}, now {}) — triggering auto-reset",
                                local.get(), last, tip_hash);
                        }
                    }
                    *last_hash = Some(tip_hash);
                    drop(last_hash);
                }
            }

            debug!(target: "dww::wallet::sync",
                "Already at tip: local={}, peer={}", local.get(), best_tip.get());

            if reorg_trigger {
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
        eprintln!("[sync] Behind tip: local={} peer_tip={} — fetching blocks", local.get(), best_tip.get());
        info!(target: "dww::wallet::sync",
            "Behind tip: local={}, peer={} — fetching blocks", local.get(), best_tip.get());

        // G7: height advancement via succ(), not + 1
        let mut next_height = local.succ();
        'fetch: for peer in &mut sync_peers {
            if next_height > best_tip {
                break 'fetch;
            }

            // G7: checked_sub on P2P critical path — handle None explicitly.
            let remaining = match best_tip.get().checked_sub(next_height.get()) {
                Some(n) => n.saturating_add(1),
                None => {
                    tracing::warn!(
                        "sync: next_height {} exceeds best_tip {} — resetting fetch window",
                        next_height.get(), best_tip.get()
                    );
                    break 'fetch;
                }
            };
            let batch_size = LINEAR_SYNC_BATCH.min(remaining);

            let blocks = match peer.request_blocks(next_height, batch_size).await {
                Ok(b) => b,
                Err(e) => {
                    debug!(target: "dww::wallet::sync", "request_blocks failed: {e}");
                    continue;
                }
            };

            let dww_r = dww.read().await;
            for block in &blocks {
                let height = block.header.height;
                match dww_r.insert_synced_block(block) {
                    Ok(()) => {
                        info!(target: "dww::wallet::sync", "Inserted block {}", height.get());
                        next_height = height.succ();
                    }
                    Err(e) => {
                        error!(target: "dww::wallet::sync",
                            "Failed to insert block {}: {e}", height.get());
                        break 'fetch;
                    }
                }
            }
            drop(dww_r);
        }

        info!(target: "dww::wallet::sync",
            "Sync cycle complete: local_height={}", next_height.get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highest_peer_tip_monotonic() {
        let tip = HighestPeerTip::new();
        tip.set_max(BlockHeight::new(42));
        assert_eq!(tip.get(), BlockHeight::new(42));
        tip.set_max(BlockHeight::new(10));
        assert_eq!(tip.get(), BlockHeight::new(42));
        tip.set_max(BlockHeight::new(100));
        assert_eq!(tip.get(), BlockHeight::new(100));
    }
}
