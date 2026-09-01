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

//! Linear Sync Client — net-node tier peer discovery + sync gate for the
//! sync requester.
//!
//! Spec: sync-protocol.md §1 (SyncClient peer discovery), §13.3 (peer discipline).
//!
//! `SyncServer` (`dwow_chain::sync_connection`) serves GetTip/GetBlocks to
//! peers over the unified `port+2` rail. This module is the CLIENT side of the
//! sync gate: it discovers full-node peers, waits for peers (or proceeds solo),
//! and dials them onto `SyncPeer`. The tip/block request flow itself lives in
//! `dwow_chain::sync_connection::SyncPeer` — consensus code never touches raw
//! P2P primitives (`subscribe_msg`, `send`, `receive`).
//!
//! ## net-node Gate Discipline (type-system.md §10.1)
//!
//! This module uses ONLY `net-wallet` tier primitives:
//! - `SESSION_DEFAULT` (session type filtering)
//! - `ChannelPtr`, `P2pPtr` (channel management)
//!
//! It does NOT use `net-full` types (BanPolicy, session-seed, transport
//! plugins) or `event-graph` types. The gate remains closed.

use std::sync::Arc;

use tracing::{info, warn};

use dwow_core::{
    barb::{BarbId, ExhibitsBarb},
    net::{
        channel::ChannelPtr,
        session::SESSION_DEFAULT,
        P2pPtr,
    },
};
use dwow_sdk::blockchain::BlockHeight;

// L2 boundary types are shared (dwow_chain::sync_boundary) — re-exported here
// so existing node code keeps importing from this module without drift.
pub use dwow_chain::sync_boundary::{BlocksBatch, PeerTip, SyncDecision};

// ── Client ────────────────────────────────────────────────────────────

/// Atomic pointer to the linear sync client.
pub type LinearSyncClientPtr = Arc<LinearSyncClient>;

/// Client-side peer discovery + sync gate for linear blockchain sync.
///
/// Discovers full-node peers (`filtered_peers`), gates the sync start on peer
/// availability (`wait_for_peers_or_proceed`), and dials them onto the unified
/// `SyncPeer` rail (`dial_sync_peers`). Tip/block requests are performed by
/// `SyncPeer` itself, not this module.
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
    /// Docker bridge gateway address excluded from full-node peer discovery.
    /// This is the NAT/bridge peer a container sees on `docker-compose`
    /// (subnet 172.18.0.0/16); it is not a real node and must not be treated
    /// as a sync source. A1: named constant, not a magic string in the filter.
    const DOCKER_GATEWAY_ADDR: &str = "172.18.0.1";

    /// The wallet binary's Cargo package name. The wallet is client-only — it
    /// runs no `SyncServer`, so it cannot serve blocks and is not a sync source.
    /// The version handshake exposes the peer's app_name
    /// (the wallet's `env!("CARGO_PKG_NAME")` = "dwow_wallet"); match against it
    /// explicitly rather than against this daemon's own name, so test peers
    /// (which use the default "dwow_core" app_name) are still treated as nodes.
    const WALLET_APP_NAME: &str = "dwow_wallet";

    /// Initialize the linear sync client.
    ///
    /// Holds the P2P pointer for peer discovery and the sync gate. It does
    /// not open any connection itself — peers are dialed onto the unified
    /// rail by `dial_sync_peers`.
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
                // H8: exact host match, not substring — `contains` matched
                // 172.18.0.10/172.18.0.100 as well as the real 172.18.0.1 bridge.
                let is_docker_gateway = c.address().host_str() == Some(Self::DOCKER_GATEWAY_ADDR);
                let is_full_node = session & SESSION_DEFAULT != 0;
                // A wallet (client-only) does not serve blocks, so it is not a
                // sync source. The version handshake stores the peer's app_name
                // on Channel::version; match the wallet's package name exactly.
                let is_wallet = c.version.get()
                    .map(|v| v.app_name == Self::WALLET_APP_NAME)
                    .unwrap_or(false);
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
                } else if is_wallet {
                    warn!(
                        target: "dwowd::proto::linear_sync_client",
                        "Skipping wallet peer {} (client-only, not a sync source)", addr
                    );
                }
                // M6.1: liveness — a channel whose task has stopped is a zombie
                // (session established but dead), not a sync source.
                let is_alive = !c.is_stopped();
                is_full_node && !is_docker_gateway && !is_wallet && is_alive
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

    /// Return true if any full-node peer (a real sync source) is connected.
    ///
    /// A2: `has_peers()` counts wallets too, but a wallet does not serve
    /// blocks — it is not a sync source. This predicates on full nodes only
    /// (SESSION_DEFAULT, non-gateway), so a wallet-only node is not treated
    /// as "peers available" for sync.
    pub fn has_full_node_peers(&self) -> bool {
        self.all_peers().iter().any(|c| {
            let session = c.session_type_id();
            let is_wallet = c.version.get()
                .map(|v| v.app_name == Self::WALLET_APP_NAME)
                .unwrap_or(false);
            session & SESSION_DEFAULT != 0
                && c.address().host_str() != Some(Self::DOCKER_GATEWAY_ADDR)
                && !is_wallet
                && !c.is_stopped()
        })
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
        genesis_authority: Option<crate::task::GenesisAuthority>,
        local_height: BlockHeight,
    ) -> SyncDecision {
        use std::time::Duration;

        let mut wait_iters = 0u32;
        loop {
            // Exit condition 1: full-node peers are available (a wallet-only
            // connection is NOT a sync source — A2).
            if self.has_full_node_peers() {
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
            if genesis_authority.is_some() && wait_iters >= 10 {
                info!(
                    target: "dwowd::proto::linear_sync_client",
                    "Genesis authority: no peers after 10s, proceeding solo at height {}",
                    local_height,
                );
                return SyncDecision::ProceedSolo;
            }

            // Exit condition 3: non-authority with genesis but no peers.
            // M2.1: a node that holds genesis and has no peers to sync from
            // is caught up to the only chain it knows — it SHALL reach
            // CaughtUp, not remain Behind forever (sync-protocol.md §13.3).
            if genesis_authority.is_none() && local_height >= BlockHeight::GENESIS && wait_iters >= 10 {
                info!(
                    target: "dwowd::proto::linear_sync_client",
                    "Non-authority with genesis: no peers after {}s. Proceeding solo (caught up to local tip).",
                    wait_iters,
                );
                return SyncDecision::ProceedSolo;
            }

            // Exit condition 4: no peers, no genesis — wait indefinitely.
            // After 30s, signal to the consensus task that we're waiting
            // for genesis so it can update sync_state for the miner.
            if wait_iters >= 30 && local_height.is_zero() {
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
                    local_height, genesis_authority.is_some(),
                );
            }
        }
    }

    // ── Unified sync connection (SyncPeer) ────────────────────────

    /// Dial all full-node peers over the **unified** sync connection (`SyncPeer`
    /// on the dedicated `port+2` listener), replacing the P2P-channel tip/block
    /// requests with the single sync rail (sync-protocol.md §11). Peer discovery
    /// (hostlist/seed → `filtered_peers`) remains a P2P concern; the sync protocol
    /// itself is `SyncPeer`/`SyncServer`.
    pub async fn dial_sync_peers(
        &self,
        magic: [u8; 4],
        genesis_hash: Option<dwow_chain::sync_types::BlockHash>,
    ) -> Vec<dwow_chain::sync_connection::SyncPeer> {
        let mut peers = Vec::new();
        for channel in self.filtered_peers() {
            let mut url = channel.address().clone();
            if let Some(port) = url.port() {
                let _ = url.set_port(Some(port + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
            }
            match dwow_chain::sync_connection::SyncPeer::dial(
                url.clone(),
                magic,
                genesis_hash.clone(),
                std::time::Duration::from_secs(15),
            )
            .await
            {
                Ok(peer) => peers.push(peer),
                Err(e) => {
                    warn!(
                        target: "dwowd::proto::linear_sync_client",
                        "dial sync peer {url} failed: {e}"
                    );
                }
            }
        }
        peers
    }
}
