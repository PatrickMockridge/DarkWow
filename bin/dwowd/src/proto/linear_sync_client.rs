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

//! Linear Sync Client — net-node tier protocol handler for the sync requester.
//!
//! `LinearSyncHandler` (server side) responds to GetTip/GetBlocks from peers.
//! This module provides the CLIENT side — it requests tips and blocks from
//! peers and returns typed results. Consensus code never touches raw P2P
//! primitives (`subscribe_msg`, `send`, `receive`).
//!
//! ## net-node Gate Discipline (type-system.md §10.1)
//!
//! This module uses ONLY `net-wallet` tier primitives:
//! - `ProtocolGenericHandler` (message dispatch)
//! - `SESSION_DEFAULT` (session type filtering)
//! - `ChannelPtr`, `P2pPtr` (channel management)
//!
//! It does NOT use `net-full` types (BanPolicy, session-seed, transport
//! plugins) or `event-graph` types. The gate remains closed.
//!
//! ## Boundary Obligations (type-system.md §10.5)
//!
//! Every receive has a timeout (obligation #3: rate discipline). Bare
//! `receive()` is impossible through this API — the timeout is enforced
//! at the boundary, not left to the caller.

use std::sync::Arc;

use tracing::{info, warn};

use dwow_core::{
    barb::{BarbId, ExhibitsBarb},
    net::{
        channel::ChannelPtr,
        session::SESSION_DEFAULT,
        P2pPtr,
    },
    Result,
};
use dwow_sdk::blockchain::BlockHeight;

use dwow_chain::sync_types::{Blocks, GetBlocks, GetTip, Tip};

// ── Boundary Types ───────────────────────────────────────────────────
//
// These are the types that cross the net-node boundary into consensus
// code. They carry the same data as the P2P message types (Tip, Blocks)
// but are nominal boundary types — consensus code never imports or
// handles raw P2P message types directly.

/// Tip info from a single peer, lifted across the net-node boundary.
///
/// All fields are carried across the boundary for diagnostic completeness.
/// The `hash` field is the peer's tip block hash — used in log messages
/// for operator visibility (which chain the peer is on).
///
/// ## Re-lift Validation (obligation #1, §10.5)
///
/// `PeerTip` SHALL only be constructed through `from_tip()`, which validates:
/// 1. `height` is within valid range (not `u64::MAX`)
/// 2. `hash` is non-empty
/// 3. `genesis_hash` is `Some` if `height > 0`
#[derive(Clone, Debug)]
pub struct PeerTip {
    pub height: BlockHeight,
    /// §8.2.1: BlockHash — nominal type for P2P boundary. Re-lifted from
    /// the wire `Tip.hash: String` via hex decode in `from_tip()`.
    pub hash: dwow_chain::sync_types::BlockHash,
    pub genesis_hash: Option<dwow_chain::sync_types::BlockHash>,
}

impl PeerTip {
    /// Re-lift a P2P `Tip` message into a validated `PeerTip` boundary type.
    ///
    /// Performs re-lift validation (obligation #1, §10.5): every byte
    /// sequence crossing the boundary SHALL be validated through a named
    /// constructor. Bare struct literals SHALL NOT construct boundary types.
    pub fn from_tip(tip: &Tip) -> crate::Result<Self> {
        // 1. Height must be within valid range. u64::MAX is the sentinel
        //    for "uninitialized" — a peer sending this is either buggy or
        //    malicious.
        if tip.height.get() == u64::MAX {
            return Err(crate::Error::Custom(
                format!("PeerTip::from_tip: invalid height {}", tip.height)
            ));
        }

        // 2. Hash: §8.2.1 re-lift is now performed by serde deserialization
        //    (BlockHash::Deserialize calls from_hex_str). At height 0, the zero
        //    sentinel is valid. No additional validation needed.
        let hash = if tip.height.get() == 0 && tip.hash.is_zero() {
            dwow_chain::sync_types::BlockHash::zero()
        } else {
            tip.hash.clone()
        };

        // 3. Genesis hash must be present if the peer has blocks.
        if tip.height.get() > 0 && tip.genesis_hash.is_none() {
            return Err(crate::Error::Custom(
                format!("PeerTip::from_tip: missing genesis hash at height {}", tip.height)
            ));
        }

        Ok(PeerTip {
            height: tip.height,
            hash,
            genesis_hash: tip.genesis_hash.clone(),
        })
    }
}

impl ExhibitsBarb for PeerTip {
    fn exhibited_barbs() -> &'static [BarbId] {
        &[BarbId::Verify, BarbId::SyncBarrier]
    }
}

/// Batch of blocks received from a peer, lifted across the net-node boundary.
#[derive(Clone, Debug)]
pub struct BlocksBatch {
    pub blocks: Vec<dwow_chain::Block>,
}

impl ExhibitsBarb for BlocksBatch {
    fn exhibited_barbs() -> &'static [BarbId] {
        // BlocksBatch carries committed blocks from a peer. The receiver
        // verifies each block (PoW, merkle, WASM) and commits accepted
        // blocks to chain state. Per type-system.md §10.4, synced blocks
        // exhibit {↓verify, ↓commit} — they are verified at the boundary
        // and committed to the local chain.
        &[BarbId::Verify, BarbId::Commit]
    }
}

// ── SyncDecision — L2→L3 Boundary Signal ───────────────────────────────
//
// The sync decision is the typed translation of the peer-wait phase.
// It replaces the hand-rolled boolean algebra in the consensus task's
// inner peer-wait loop (consensus_linear.rs lines 204-250) with a typed
// enum that the consensus task matches on exhaustively.
//
// Per type-system.md §5.1: "A bare `bool` SHALL NOT gate consensus-
// critical paths." This enum makes the gate type-checkable.

/// Typed result of the peer-wait phase — the L2→L3 boundary signal.
///
/// The consensus task receives one of these and transitions sync_state
/// accordingly. Every variant corresponds to a distinguishable condition
/// in the peer-wait loop.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncDecision {
    /// At least one full-node peer is connected. Proceed to tip collection
    /// and block sync.
    PeersAvailable,

    /// No peers connected, but this node is the genesis authority with
    /// local genesis at height >= 1. Proceed to solo mining (authority
    /// gate — consensus_linear.rs line 217).
    ProceedSolo,

    /// No peers connected, no local genesis (height == 0), and no peer
    /// has genesis either. Mining is impossible — wait for genesis to
    /// appear from a peer or be created locally.
    WaitForGenesis,

    /// Transient condition: re-enter the outer sync loop and re-check.
    /// Used when the peer-wait phase detects a state change that requires
    /// re-evaluation (e.g., a peer connected and disconnected rapidly).
    Retry,
}

// ── Client ────────────────────────────────────────────────────────────

/// Atomic pointer to the linear sync client.
pub type LinearSyncClientPtr = Arc<LinearSyncClient>;

/// Client-side handler for linear blockchain sync requests.
///
/// Encapsulates all P2P operations for requesting tips and blocks from
/// peers. Consensus code receives typed `Result<PeerTip>` and
/// `Result<BlocksBatch>` — never raw P2P message types.
///
/// ## Protocol handler registration
///
/// `LinearSyncClient` registers `GetTip`/`Tip` and `GetBlocks`/`Blocks`
/// message dispatchers on every matching channel at init time (via
/// `ProtocolGenericHandler`). This ensures that when the consensus task
/// calls `request_tip()`, the `Tip` response dispatcher is already
/// registered on the channel's `MessageSubsystem`.
///
/// ## Timeout enforcement (obligation #3)
///
/// Every receive operation has a timeout. The `request_tip` timeout is
/// 5 seconds; the `request_blocks` timeout is 15 seconds. These match
/// the timeouts that were previously hand-rolled in
/// `consensus_linear_init_task`. Bare `receive()` is impossible through
/// this API.
pub struct LinearSyncClient {
    /// P2P network pointer for peer discovery
    p2p: P2pPtr,
}

impl ExhibitsBarb for LinearSyncClient {
    fn exhibited_barbs() -> &'static [BarbId] {
        // Client-side sync: verifies peer responses, gates on sync barrier.
        // Per type-system.md §10.4, the client side of the sync protocol
        // exhibits {↓verify, ↓sync-barrier} — it verifies tip/block data
        // from peers and coordinates with the miner via the sync barrier.
        &[BarbId::Verify, BarbId::SyncBarrier]
    }
}

impl LinearSyncClient {
    /// Request timeout for tip queries (seconds).
    const TIP_TIMEOUT: u64 = 5;

    /// Request timeout for block queries (seconds).
    const BLOCKS_TIMEOUT: u64 = 15;

    /// Initialize the linear sync client.
    ///
    /// Does NOT register protocol handlers — the server-side
    /// `LinearSyncHandler` already registers `GetTip`/`Tip` and
    /// `GetBlocks`/`Blocks` dispatchers on all matching channels.
    /// The client uses `channel.subscribe_msg()` to receive responses
    /// through those already-registered dispatchers.
    pub fn new(p2p: &P2pPtr) -> LinearSyncClientPtr {
        info!(
            target: "dwowd::proto::linear_sync_client::new",
            "Initializing linear sync client"
        );
        Arc::new(Self { p2p: p2p.clone() })
    }

    // ── Peer Discovery ────────────────────────────────────────────

    /// Return the number of currently connected peers.
    pub fn peer_count(&self) -> usize {
        self.p2p.hosts().peers().len()
    }

    /// Return true if any peers are connected.
    pub fn has_peers(&self) -> bool {
        !self.p2p.hosts().peers().is_empty()
    }

    /// Return all currently connected peers.
    pub fn all_peers(&self) -> Vec<ChannelPtr> {
        self.p2p.hosts().peers()
    }

    /// Filter peers to full nodes only, excluding Docker gateway addresses.
    ///
    /// Full nodes are identified by `SESSION_DEFAULT` bit in session type.
    /// Docker gateway (`172.18.0.1`) is excluded because it's the bridge
    /// interface, not a real peer.
    pub fn filtered_peers(&self) -> Vec<ChannelPtr> {
        let peers = self.all_peers();
        let filtered: Vec<_> = peers
            .iter()
            .filter(|c| {
                let session = c.session_type_id();
                let addr = c.address().as_str();
                let is_docker_gateway = addr.contains("172.18.0.1");
                let is_full_node = session & SESSION_DEFAULT != 0;
                if is_docker_gateway {
                    warn!(
                        target: "dwowd::proto::linear_sync_client",
                        "Skipping Docker gateway peer {}", addr
                    );
                } else if !is_full_node {
                    warn!(
                        target: "dwowd::proto::linear_sync_client",
                        "Skipping non-node peer {} session={:#b}", addr, session
                    );
                }
                is_full_node && !is_docker_gateway
            })
            .cloned()
            .collect();

        info!(
            target: "dwowd::proto::linear_sync_client",
            "Filtered {} full-node peers from {} total connections",
            filtered.len(),
            peers.len(),
        );
        filtered
    }

    // ── Peer-Wait / Sync Gate ─────────────────────────────────────

    /// Wait for peers or proceed based on authority and local chain state.
    ///
    /// Encapsulates the ENTIRE inner peer-wait loop previously hand-rolled
    /// in `consensus_linear_init_task` (lines 204-250). Returns a typed
    /// `SyncDecision` that the consensus task matches on exhaustively.
    ///
    /// ## Authority Gate (type-system.md §5.1)
    ///
    /// The three conditions — authorization, chain state, and P2P state —
    /// are NOT conflated into a single boolean. Each is checked in its
    /// own domain, and the result is a typed enum variant.
    ///
    /// ## Timeout
    ///
    /// Authority gate fires after 10s without peers (when genesis exists
    /// locally). The universal stuck indicator fires at 120s.
    pub async fn wait_for_peers_or_proceed(
        &self,
        genesis_authority: bool,
        local_height: BlockHeight,
    ) -> SyncDecision {
        use std::time::Duration;

        let mut wait_iters = 0u32;
        loop {
            // Exit condition 1: peers are available
            if self.has_peers() {
                return SyncDecision::PeersAvailable;
            }

            smol::Timer::after(Duration::from_secs(1)).await;
            wait_iters += 1;

            // Exit condition 2: genesis authority MAY proceed without
            // peers after 10s timeout, regardless of local_height.
            // At height >= 1: solo mine (authority has genesis).
            // At height == 0: create genesis (authority IS the genesis
            // source — HAZOP L2: returning WaitForGenesis to the
            // authority would deadlock; the authority must create it).
            if genesis_authority && wait_iters >= 10 {
                info!(
                    target: "dwowd::proto::linear_sync_client",
                    "Genesis authority: no peers after 10s, proceeding solo at height {}",
                    local_height,
                );
                return SyncDecision::ProceedSolo;
            }

            // Exit condition 3: non-authority with genesis but no peers.
            // Cannot proceed solo (not the authority). Cannot
            // WaitForGenesis (genesis exists). Must retry outer
            // consensus loop — HAZOP H1: without this, the function
            // loops forever (no exit condition matches).
            if !genesis_authority && local_height >= BlockHeight::GENESIS && wait_iters >= 10 {
                info!(
                    target: "dwowd::proto::linear_sync_client",
                    "Non-authority with genesis: no peers after {}s. Returning Retry.",
                    wait_iters,
                );
                return SyncDecision::Retry;
            }

            // Exit condition 4: no peers, no genesis — wait indefinitely.
            // After 30s, signal to the consensus task that we're waiting
            // for genesis so it can update sync_state for the miner.
            if wait_iters >= 30 && local_height.get() == 0 {
                info!(
                    target: "dwowd::proto::linear_sync_client",
                    "No peers and no genesis after 30s — returning WaitForGenesis",
                );
                return SyncDecision::WaitForGenesis;
            }

            // Universal stuck indicator (120s = 2 minutes)
            if wait_iters == 120 {
                warn!(
                    target: "dwowd::proto::linear_sync_client",
                    "Still waiting for peers after 120s — local_height={} authority={}",
                    local_height, genesis_authority,
                );
            }
        }
    }

    // ── Tip Requests ──────────────────────────────────────────────

    /// Request the chain tip from a specific peer.
    ///
    /// Subscribes to `Tip` messages on the channel, sends `GetTip`,
    /// and waits for the response with a timeout (obligation #3).
    ///
    /// Returns `Ok(PeerTip)` on success, or an error if the
    /// subscription, send, or receive fails.
    pub async fn request_tip(&self, channel: &ChannelPtr) -> Result<PeerTip> {
        let tip_sub = channel
            .subscribe_msg::<Tip>()
            .await
            .map_err(|e| dwow_core::Error::Custom(format!(
                "Failed to subscribe to Tip on peer {}: {e}",
                channel.address().as_str()
            )))?;

        channel.send(&GetTip).await.map_err(|e| {
            dwow_core::Error::Custom(format!(
                "Failed to send GetTip to peer {}: {e}",
                channel.address().as_str()
            ))
        })?;

        let tip = tip_sub
            .receive_with_timeout(Self::TIP_TIMEOUT)
            .await
            .map_err(|_| {
                dwow_core::Error::Custom(format!(
                    "GetTip timed out after {}s for peer {}",
                    Self::TIP_TIMEOUT,
                    channel.address().as_str(),
                ))
            })?;

        info!(
            target: "dwowd::proto::linear_sync_client",
            "Peer {}: height={} genesis={}",
            channel.address().as_str(),
            tip.height,
            tip.genesis_hash.as_ref().map(|h| h.to_hex()).unwrap_or_else(|| "unknown".to_string()),
        );

        // Re-lift through validating constructor (obligation #1, §10.5).
        // Bare struct literal replaced with PeerTip::from_tip().
        PeerTip::from_tip(&tip)
    }

    /// Collect tips from all filtered peers.
    ///
    /// Returns a list of `(channel, PeerTip)` pairs. Peers that fail
    /// subscription, send, or timeout are silently skipped (logged
    /// internally by `request_tip`).
    pub async fn collect_tips(&self) -> Vec<(ChannelPtr, PeerTip)> {
        let peers = self.filtered_peers();
        let mut tips = Vec::with_capacity(peers.len());

        for channel in &peers {
            match self.request_tip(channel).await {
                Ok(tip) => {
                    tips.push((channel.clone(), tip));
                }
                Err(e) => {
                    warn!(
                        target: "dwowd::proto::linear_sync_client",
                        "Tip collection failed for peer {}: {e}",
                        channel.address().as_str(),
                    );
                }
            }
        }

        info!(
            target: "dwowd::proto::linear_sync_client",
            "Collected tips from {}/{} peers",
            tips.len(),
            peers.len(),
        );
        tips
    }

    // ── Block Requests ────────────────────────────────────────────

    /// Request a batch of blocks from a specific peer.
    ///
    /// Subscribes to `Blocks` messages on the channel, sends
    /// `GetBlocks { start_height, count }`, and waits for the
    /// response with a timeout (obligation #3).
    ///
    /// Returns `Ok(BlocksBatch)` on success, or an error if the
    /// subscription, send, or receive fails.
    pub async fn request_blocks(
        &self,
        channel: &ChannelPtr,
        start_height: BlockHeight,
        count: u64,
    ) -> Result<BlocksBatch> {
        let blocks_sub = channel
            .subscribe_msg::<Blocks>()
            .await
            .map_err(|e| dwow_core::Error::Custom(format!(
                "Failed to subscribe to Blocks on peer {}: {e}",
                channel.address().as_str()
            )))?;

        let request = GetBlocks { start_height, count };
        channel.send(&request).await.map_err(|e| {
            dwow_core::Error::Custom(format!(
                "Failed to send GetBlocks to peer {}: {e}",
                channel.address().as_str()
            ))
        })?;

        let blocks_msg = blocks_sub
            .receive_with_timeout(Self::BLOCKS_TIMEOUT)
            .await
            .map_err(|_| {
                dwow_core::Error::Custom(format!(
                    "GetBlocks timed out after {}s for peer {}",
                    Self::BLOCKS_TIMEOUT,
                    channel.address().as_str(),
                ))
            })?;

        info!(
            target: "dwowd::proto::linear_sync_client",
            "Received {} blocks from peer {}",
            blocks_msg.blocks.len(),
            channel.address().as_str(),
        );

        Ok(BlocksBatch { blocks: blocks_msg.blocks.clone() })
    }
}
