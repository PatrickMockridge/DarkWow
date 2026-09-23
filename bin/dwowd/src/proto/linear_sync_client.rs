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
//! Spec: sync-protocol.md §8 (unified SyncPeer rail), §13.3 (pull loop + peer discipline).
//!
//! `SyncServer` (`dwow_chain::sync_connection`) serves GetTip/GetBlocks to
//! peers over the unified `port+2` rail. This module is the CLIENT side of the
//! sync gate: it discovers full-node peers, waits for peers (or proceeds solo),
//! and dials them onto `SyncPeer`. The tip/block request flow itself lives in
//! `dwow_chain::sync_connection::SyncPeer` — consensus code never touches raw
//! P2P channel primitives (the unified rail is plain TCP).
//!
//! ## net-node Gate Discipline (type-system.md §10.1)
//!
//! This module uses ONLY `net-wallet` tier primitives:
//! - `SESSION_DEFAULT` (session type filtering)
//! - `ChannelPtr`, `P2pPtr` (channel management)
//!
//! It does NOT use `net-full` types (BanPolicy, session-seed, transport
//! plugins) or `event-graph` types. The gate remains closed.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tracing::{info, warn};

use dwow_core::{
    barb::{BarbId, ExhibitsBarb},
    net::{
        channel::ChannelPtr,
        session::SESSION_DEFAULT,
        P2pPtr,
    },
};

// L2 boundary types are shared (dwow_chain::sync_boundary) — re-exported here
// so existing node code keeps importing from this module without drift.
pub use dwow_chain::sync_boundary::PeerTip;

// ── Client ────────────────────────────────────────────────────────────

/// Atomic pointer to the linear sync client.
pub type LinearSyncClientPtr = Arc<LinearSyncClient>;

/// Persistent per-peer misbehaviour scores (`OBL-C33`).
///
/// Spec `sync-protocol.md` §13.3: *"Peer discipline SHALL be a single persistent score (Bitcoin Core
/// `Misbehaving()`): a peer that serves an **invalid block** is disconnected; a peer that times out is
/// simply skipped and the next peer tried."* This is that score, keyed by the peer's dialed URL — the
/// stable identity a `SyncPeer` carries, since a peer object is fresh on every dial and cannot hold a
/// memory of its own.
///
/// Deliberately one counter and no taxonomy: the spec's next sentence rules out a deadness/slowness
/// taxonomy, bounded backoff, a heartbeat and a watchdog, as machinery with no production analogue. So
/// timeouts do not call [`PeerScores::penalise`] at all, and nothing decays — which is why
/// `MISBEHAVIOUR_LIMIT` is 1: one invalid block is the rule, not a budget.
struct PeerScores {
    scores: Mutex<HashMap<String, u32>>,
}

impl PeerScores {
    /// Score at which a peer is disconnected and never re-dialed. A constant so the rule is named rather
    /// than spelled as a literal at the call site.
    const MISBEHAVIOUR_LIMIT: u32 = 1;

    fn new() -> Self {
        Self { scores: Mutex::new(HashMap::new()) }
    }

    /// Record one misbehaviour by the peer at `url` and return its score.
    ///
    /// Called for a block that is malformed, for the wrong network, or fails its own proof — never for a
    /// timeout, and never for an `accept_block` failure, which may be a legitimate fork between two honest
    /// nodes (`accept_block`'s caller already reorgs in that case).
    fn penalise(&self, url: &url::Url) -> u32 {
        let mut scores = self.scores.lock().unwrap_or_else(|e| e.into_inner());
        let score = scores.entry(url.to_string()).or_insert(0);
        *score = score.saturating_add(1);
        *score
    }

    /// Whether `url` has reached the limit and must not be dialed again.
    fn is_banned(&self, url: &url::Url) -> bool {
        let scores = self.scores.lock().unwrap_or_else(|e| e.into_inner());
        match scores.get(&url.to_string()) {
            Some(score) => *score >= Self::MISBEHAVIOUR_LIMIT,
            None => false,
        }
    }
}

/// Client-side peer discovery + sync gate for linear blockchain sync.
///
/// Discovers full-node peers (`filtered_peers`) and dials them onto the unified
/// `SyncPeer` rail (`dial_sync_peers`). Tip/block requests are performed by
/// `SyncPeer` itself, not this module.
pub struct LinearSyncClient {
    /// P2P network pointer for peer discovery
    p2p: P2pPtr,
    /// Per-peer misbehaviour scores, kept across passes (`OBL-C33`).
    peer_scores: PeerScores,
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
        Arc::new(Self { p2p: p2p.clone(), peer_scores: PeerScores::new() })
    }

    // ── Peer Discovery ────────────────────────────────────────────

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
                // NOTE: liveness is tested by the sync dial itself (port+2),
                // NOT by the P2P channel's is_stopped() — the two rails are
                // separate sockets, and a "stopped" P2P channel can still have a
                // live sync listener. Filtering on is_stopped() here parked the
                // node with "peers=3 but no full-node peer".
                is_full_node && !is_docker_gateway && !is_wallet
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
    /// A2: the raw P2P peer set counts wallets too, but a wallet does not
    /// serve blocks — it is not a sync source. This predicates on full nodes
    /// only (SESSION_DEFAULT, non-gateway), so a wallet-only node is not
    /// treated as "peers available" for sync.
    pub fn has_full_node_peers(&self) -> bool {
        self.all_peers().iter().any(|c| {
            let session = c.session_type_id();
            let is_wallet = c.version.get()
                .map(|v| v.app_name == Self::WALLET_APP_NAME)
                .unwrap_or(false);
            session & SESSION_DEFAULT != 0
                && c.address().host_str() != Some(Self::DOCKER_GATEWAY_ADDR)
                && !is_wallet
        })
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
        // M5.2: bound the sequential dial phase — do not serially dial every
        // discovered peer (a dead peer costs one 15s timeout each pass). Cap at
        // a small fan-out; the pull loop rotates across the successfully-dialed
        // set, and peers that fail to dial are simply not in it.
        const MAX_SYNC_PEERS: usize = 8;
        let mut peers = Vec::new();
        for channel in self.filtered_peers().into_iter().take(MAX_SYNC_PEERS) {
            let mut url = channel.address().clone();
            if let Some(port) = url.port() {
                let _ = url.set_port(Some(port + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
            }
            // A peer that has served an invalid block is not dialed again (`OBL-C33`) — this is the
            // "disconnected" half of the spec's rule, and it is what makes the score *persistent*: the
            // pull pass that caught the peer has long since dropped its connection by the time the next
            // tick runs, so without this the score would only re-derive what the last pass already knew.
            // Note it is checked per dial, not by filtering the peer list, so a ban takes effect on the
            // very next pass rather than at the next discovery refresh.
            if self.peer_scores.is_banned(&url) {
                warn!(
                    target: "dwowd::proto::linear_sync_client",
                    "skipping {url}: the peer reached the misbehaviour limit \
                     ({} invalid block(s), sync-protocol.md §13.3)",
                    PeerScores::MISBEHAVIOUR_LIMIT
                );
                continue;
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

    /// Score one misbehaviour by the peer at `url` (`OBL-C33`). See [`PeerScores::penalise`] for what
    /// does and does not count as one.
    pub fn penalise(&self, url: &url::Url) -> u32 {
        let score = self.peer_scores.penalise(url);
        warn!(
            target: "dwowd::proto::linear_sync_client",
            "peer {url} served an invalid block — score {score} of {}",
            PeerScores::MISBEHAVIOUR_LIMIT
        );
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(port: u16) -> url::Url {
        url::Url::parse(&format!("tcp+tls://127.0.0.1:{port}")).expect("test url")
    }

    /// `OBL-C33` — a peer that serves an invalid block is scored, and at the limit it is never dialed
    /// again while every other peer is unaffected.
    ///
    /// The persistence is the point, and it is what a per-pass skip cannot express: the client outlives
    /// the pass that observed the misbehaviour.
    #[test]
    fn test_a_misbehaving_peer_is_banned_and_others_are_not() {
        let scores = PeerScores::new();
        let bad = peer(10001);
        let good = peer(10002);

        assert!(!scores.is_banned(&bad), "control: an unpenalised peer is dialable");
        assert!(!scores.is_banned(&good), "control: and so is an unrelated one");

        assert_eq!(scores.penalise(&bad), 1, "the first misbehaviour scores one");
        assert!(
            scores.is_banned(&bad),
            "at the limit ({}) the peer must not be dialed again",
            PeerScores::MISBEHAVIOUR_LIMIT
        );
        assert!(
            !scores.is_banned(&good),
            "control: scoring one peer must not touch another — the score is keyed by URL, not global"
        );

        // Score again: the counter is monotonic and the peer stays banned.
        assert_eq!(scores.penalise(&bad), 2, "the score accumulates rather than resetting per pass");
        assert!(scores.is_banned(&bad), "and the ban persists");
    }
}
