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

//! DAG-substrate block absorber (type-system.md §10.4–§10.5).
//!
//! Subscribes to the EventGraph [`event_pub`] notification channel. When an
//! event arrives whose content begins with `0x42` — the blockchain-event
//! marker byte — it runs the d3 validation sequence and, if the event is
//! well-formed, routes the block into the blockchain's `accept_block` path.
//!
//! This is the absorber boundary: serialized bytes arrive from the DAG
//! (barbs erased), re-lift through the validating sequence below, then
//! touch blockchain state ONLY after validation passes. The boundary is
//! one-directional: `BlockchainAbsorber {Verify, Commit}` ← EventGraph
//! `{DagParent, Broadcast, RateLimit, QuorumQuery}` — the allowed
//! direction per [`bridge_safe`] (§10.4).

use std::sync::Arc;

use dwow_chain::CChainState;
use dwow_chain::execution::MAX_BLOCK_SIZE;
use dwow_core::event_graph::events::Event;
use dwow_mempool::MempoolPtr;
use tracing::{debug, warn};

use crate::proto::linear_broadcast::BlockBroadcast;

/// The blockchain-event marker byte (type-system.md §10.4).
pub const BLOCKCHAIN_EVENT_MARKER: u8 = 0x42;

/// Kind byte for wrapped blockchain events.
/// `0x01` = block (phase 1). `0x02` = transaction (phase 2, reserved).
pub mod kind {
    pub const BLOCK: u8 = 0x01;
    #[allow(dead_code)]
    pub const TX: u8 = 0x02;
}

/// The d3 validation sequence applied to a DAG event whose content starts
/// with `0x42`. Returns `Some(BlockBroadcast)` if the event is a valid
/// blockchain event that should be absorbed, or `None` if it should be
/// silently ignored (legitimate non-blockchain event-graph content, or
/// malformed — the sender-side defenses own peer punishment).
///
/// # Sequence (per §10.5)
/// 1. Marker check — `0x42` on the first byte (caller has already done
///    this; the function returns `None` for non-blockchain events).
/// 2. Kind + minimum-length guard — at least 2 bytes for [marker][kind].
/// 3. Kind dispatch — unknown kind bytes are dropped (metric).
/// 4. Pre-decode length cap — `> MAX_BLOCK_SIZE` is dropped.
/// 5. Decode Frame — `serde_json::from_slice` failure is dropped.
/// 6. Only now touch blockchain state — caller feeds the result into
///    `absorb_block`.
///
/// No ban or peer punishment occurs here — the sender is not attributable
/// post-`dag_insert` (the transport layer's own flood-ban and metering
/// rate-limit at `proto.rs` are the first defense).
pub fn validate_blockchain_event(content: &[u8]) -> Option<BlockBroadcast> {
    // 2. Kind + minimum-length
    if content.len() < 2 {
        return None;
    }
    // 3. Kind dispatch
    if content[1] != kind::BLOCK {
        // Phase 2: tx events are reserved; silently drop for now.
        debug!(
            target: "dwowd::dag_absorber",
            "Unknown blockchain event kind 0x{:02x} — dropped",
            content[1],
        );
        return None;
    }

    let payload = &content[2..];

    // 4. Pre-decode length cap
    if payload.len() > MAX_BLOCK_SIZE {
        warn!(
            target: "dwowd::dag_absorber",
            "0x42 block event payload {} exceeds MAX_BLOCK_SIZE — dropped",
            payload.len(),
        );
        return None;
    }

    // 5. Decode
    let frame: BlockBroadcast = match serde_json::from_slice(payload) {
        Ok(f) => f,
        Err(e) => {
            warn!(
                target: "dwowd::dag_absorber",
                "0x42 block event decode failed: {e} — dropped",
            );
            return None;
        }
    };

    Some(frame)
}

/// Start the DAG absorber background task.
///
/// Subscribes to the EventGraph [`event_pub`] notification channel
/// (darkirc server.rs:317 pattern). For each event whose content starts
/// with `0x42`, runs [`validate_blockchain_event`] and, if valid, routes
/// the block into the blockchain accept path via
/// [`linear_broadcast::absorb_block`].
///
/// The absorber SHALL NOT touch the blockchain sled trees unless validation
/// passes. The [`bridge_safe`] assertion at startup witnesses the §10.4
/// quarantine direction (event-graph → blockchain is the allowed crossing).
pub async fn start_dag_absorber(
    event_graph: &dwow_core::event_graph::EventGraphPtr,
    chain: &Arc<CChainState>,
    _vm: &Arc<randomx::RandomXVM>,
    mempool: &Option<MempoolPtr>,
) {
    let mut event_pub = event_graph.event_pub.clone();

    // §10.4 quarantine assertion — defined in linear_broadcast.rs so it
    // is unit-testable without pulling in the full dag_absorber module.
    crate::proto::linear_broadcast::dag_absorber_barb_check();

    tracing::info!(
        target: "dwowd::dag_absorber",
        "DAG absorber started — listening for 0x42 blockchain events"
    );

    loop {
        let event: Event = event_pub.recv().await;
        if event.content().first() != Some(&BLOCKCHAIN_EVENT_MARKER) {
            continue; // silent ignore — legitimate non-blockchain DAG content
        }
        if let Some(frame) = validate_blockchain_event(event.content()) {
            super::linear_broadcast::absorb_block(
                chain, vm, mempool, &frame,
            ).await;
        }
    }
}
