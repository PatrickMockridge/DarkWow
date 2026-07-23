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
//!
//! ## Protocol types (G1: single definition)
//!
//! Sync message types are imported from `dwow_chain::sync_types` — the
//! canonical definition shared by wallet, mining node, and observer nodes.
//! No node defines its own copy.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, error, info, warn};

use dwow_core::net::P2pPtr;
use dwow_sdk::blockchain::BlockHeight;

// G1: Single definition of sync types — import from shared module, never define locally.
use dwow_chain::sync_types::{Blocks, GetBlocks, GetTip, Tip};

use crate::wallet_error::Result;
use crate::DwwPtr;

/// Fixed batch size for GetBlocks requests — matches dwowd LINEAR_SYNC_BATCH
const LINEAR_SYNC_BATCH: u64 = 20;

// ============================================================================
// HighestPeerTip — atomic, monotonic, BlockHeight-typed
// ============================================================================
//
// G5: Public API exposes BlockHeight. AtomicU64 is an internal detail.
// G3: The .get() calls inside set_max/get are at the hardware atomic boundary —
//     the ONE permitted use of .get() outside persistence boundaries.

/// Highest peer tip seen. Updated on each Tip response.
pub struct HighestPeerTip(AtomicU64);

impl HighestPeerTip {
    pub fn new() -> Self { Self(AtomicU64::new(0)) }

    /// Returns the highest peer tip as a BlockHeight.
    /// G3: .get() at atomic boundary — audited.
    pub fn get(&self) -> BlockHeight {
        BlockHeight::new(self.0.load(Ordering::Relaxed))
    }

    /// Updates the highest peer tip if the given height exceeds the current value.
    /// G3: .get() at atomic boundary — audited.
    pub fn set_max(&self, height: BlockHeight) {
        let _ = self.0.fetch_update(Ordering::Release, Ordering::Relaxed, |c| {
            if height.get() > c { Some(height.get()) } else { None }
        });
    }
}

// ============================================================================
// Sync loop — uses dwow_core::net P2P channels
// ============================================================================
//
// G4: All height variables use BlockHeight. Counters/batch sizes are u64.
// G7: Height arithmetic uses named methods (succ, checked_sub, Ord).

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
    let zero_height = BlockHeight::new(0); // pre-genesis sentinel (G6)

    eprintln!("[sync] Sync task started");
    info!(target: "dww::wallet::sync", "Wallet sync task running — P2p handles peer discovery");

    // D1: Stuck-sync watchdog — if we have peers but never advance past height 0,
    // the sync protocol is not exchanging data. Emit FATAL after 6 consecutive ticks (60s).
    let mut stuck_ticks: u32 = 0;
    const STUCK_TICK_LIMIT: u32 = 6;

    loop {
        smol::Timer::after(Duration::from_secs(10)).await;

        // Phase 1: Check peer connectivity — extract data, then DROP the read lock.
        // D6: Read lock was held across the entire tick body (GetTip 5s×N + GetBlocks 30s),
        // blocking any future write path. Extract only what we need and drop explicitly.
        let (local, peer_count, p2p_opt) = {
            let dww_r = dww.read().await;
            // G2: chain_height() returns Result<BlockHeight> — must handle error explicitly
            let local = match dww_r.wallet.chain_height() {
                Ok(h) => h,
                Err(e) => {
                    error!(target: "dww::wallet::sync",
                        "chain_height failed: {e} — skipping sync tick");
                    continue;
                }
            };
            let peer_count = dww_r.p2p.as_ref()
                .map(|p| p.hosts().peers().len())
                .unwrap_or(0);
            let p2p_opt = dww_r.p2p.clone();
            (local, peer_count, p2p_opt)
        }; // D6: read lock DROPPED here — network I/O below does not hold it

        eprintln!("[sync] Tick: local={} peers={}", local.get(), peer_count);
        info!(target: "dww::wallet::sync",
            "Sync tick: local_height={}, peer_count={}", local.get(), peer_count);

        // D1: Stuck-sync watchdog
        if peer_count > 0 && local == zero_height {
            stuck_ticks += 1;
            if stuck_ticks >= STUCK_TICK_LIMIT {
                error!(target: "dww::wallet::sync",
                    "FATAL: Wallet has {} peers but zero chain height after {} consecutive ticks ({}s). \
                     P2P connected but sync protocol is not exchanging data. \
                     Peer LinearSyncHandlers may not be ready, or version mismatch.",
                    peer_count, stuck_ticks, stuck_ticks * 10);
                eprintln!(
                    "[sync] FATAL: Wallet has {} peers but zero chain height after {} consecutive ticks ({}s). \
                     P2P connected but sync protocol is not exchanging data.",
                    peer_count, stuck_ticks, stuck_ticks * 10);
            }
        } else if local > zero_height {
            stuck_ticks = 0; // Reset — we're making progress
        }

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

        // G4: best_tip is BlockHeight
        let mut best_tip = zero_height;
        let mut tip_timeouts: u32 = 0;
        let mut tip_votes: std::collections::BTreeMap<String, (BlockHeight, u32)> =
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
                    "Peer tip: height={}", tip.height.get());
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
            } else {
                // D7: Tip response timeout — track consecutive failures
                tip_timeouts += 1;
                if tip_timeouts % 3 == 0 {
                    warn!(target: "dww::wallet::sync",
                        "Tip response timeout: {} peers failed to respond in this tick ({} total consecutive timeouts across {} peers). \
                         Peers may not have LinearSyncHandler running.",
                        tip_timeouts, tip_timeouts, channel_list.len());
                }
            }
        }

        // G7: BlockHeight Ord comparison
        if best_tip <= local {
            // ── Reorg detection ──────────────────────────────────────
            let mut reorg_trigger = false;
            // G7: local > zero_height, not local > 0
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

            // G7: checked_sub (returns Option<u64> — the count, not a height)
            // None = next_height > best_tip, which we already guard above
            let remaining = best_tip.get().checked_sub(next_height.get())
                .unwrap_or(0)
                .saturating_add(1);
            let batch_size = LINEAR_SYNC_BATCH.min(remaining);
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
                let height = block.header.height;
                match dww_r.insert_synced_block(block) {
                    Ok(()) => {
                        info!(target: "dww::wallet::sync",
                            "Inserted block {}", height.get());
                        // G7: height advancement via succ()
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
