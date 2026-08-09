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

//! Barb vocabulary — the 24 observable actions (type-system.md §1.1).
//!
//! This module is unconditionally compiled: barbs are type-system interior
//! vocabulary (a 1:1 mirror of the Lean4 `Barb` inductive), not networking
//! infrastructure. They are referenced by compilation units inside and
//! outside the `net` module. Placing them behind a networking feature gate
//! is a category error — the barb vocabulary is the static type language
//! for describing process behavior; the boundary obligations in `net`
//! (channel.rs bans, hosts.rs quarantine, metering.rs rate-limit) are the
//! runtime enforcement that uses this vocabulary as a tool (§10.5:
//! "statically-proven interior ⊆ absorber boundary ⊆ dynamic residue").
//!
//! Re-exported as `crate::net::barb_trait` for backward compatibility.

/// Barb identifiers for the 22 observable actions defined in Types.lean.
///
/// These correspond 1:1 to the `Barb` inductive in the Lean4 proofs.
/// Every barb is a compile-time constant — no runtime string matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarbId {
    // Authorization barbs (§1.1 rows 1-14)
    Spend,
    View,
    Nullify,
    Commit,
    Prove,
    Verify,
    Dispatch,
    Gate,
    Denominate,
    ProveInclusion,
    Encrypt,
    Derive,
    Discover,
    Mine,
    // Fee signalling barbs (type-system.md §1.1 rows 23-24)
    FeeWindowAdvertise,
    FeeWindowDiscover,
    // Concurrency barbs (§1.1 rows 15-22)
    Concurrent,
    Merge,
    SyncBarrier,
    Broadcast,
    RateLimit,
    GossipForward,
    QuorumQuery,
    DagParent,
}

impl BarbId {
    /// Returns true if this barb is blockchain-only.
    /// These barbs SHALL NOT cross the quarantine boundary to the event graph.
    pub fn is_blockchain_barb(&self) -> bool {
        matches!(
            self,
            BarbId::Spend | BarbId::Nullify | BarbId::Commit | BarbId::Mine
        )
    }

    /// Returns true if this barb is event-graph-only.
    pub fn is_event_graph_barb(&self) -> bool {
        matches!(self, BarbId::DagParent | BarbId::QuorumQuery | BarbId::RateLimit)
    }

    /// Returns true if this barb is a concurrency barb.
    pub fn is_concurrency_barb(&self) -> bool {
        matches!(
            self,
            BarbId::Concurrent
                | BarbId::Merge
                | BarbId::SyncBarrier
                | BarbId::Broadcast
                | BarbId::RateLimit
                | BarbId::GossipForward
                | BarbId::QuorumQuery
                | BarbId::DagParent
        )
    }
}

/// Marker trait for types that declare their observable behaviors.
///
/// A protocol handler implementing this trait declares its barb set at
/// compile time. The barb set is the type-level witness of the process's
/// behavioral position in the concurrent interaction graph.
///
/// # Safety
///
/// The declared barbs MUST match the actual runtime behavior. A false
/// declaration (claiming barbs the process does not exhibit, or omitting
/// barbs it does exhibit) is a specification violation.
///
/// # Example
///
/// ```ignore
/// impl ExhibitsBarb for ProtocolEventGraph {
///     fn exhibited_barbs() -> &'static [BarbId] {
///         &[BarbId::DagParent, BarbId::Broadcast, BarbId::RateLimit, BarbId::QuorumQuery]
///     }
/// }
/// ```
pub trait ExhibitsBarb {
    /// The set of barbs this process exhibits.
    fn exhibited_barbs() -> &'static [BarbId];

    /// Returns true if this process exhibits the given barb.
    fn exhibits(barb: BarbId) -> bool {
        Self::exhibited_barbs().contains(&barb)
    }

    /// Returns true if this process exhibits all given barbs.
    fn exhibits_all(barbs: &[BarbId]) -> bool {
        barbs.iter().all(|b| Self::exhibits(*b))
    }

    /// Returns true if this process exhibits any blockchain-only barb.
    /// Used for quarantine enforcement.
    fn has_blockchain_barbs() -> bool {
        Self::exhibited_barbs().iter().any(|b| b.is_blockchain_barb())
    }

    /// Returns true if this process exhibits any event-graph-only barb.
    fn has_event_graph_barbs() -> bool {
        Self::exhibited_barbs().iter().any(|b| b.is_event_graph_barb())
    }
}

/// Bridging safety check: a message from `Source` can be forwarded to
/// `Destination` only if no incompatible barbs cross the quarantine boundary.
///
/// A blockchain process (carrying ↓spend, ↓nullify, ↓commit, ↓mine) SHALL
/// NOT route messages through an event-graph channel. A process with
/// blockchain barbs routed through the event graph would leak capability
/// semantics across the quarantine boundary.
pub fn bridge_safe<Source: ExhibitsBarb, Dest: ExhibitsBarb>() -> bool {
    // If source is blockchain-only and dest is event-graph, block
    if Source::has_blockchain_barbs() && Dest::has_event_graph_barbs() {
        return false;
    }
    // If source is event-graph-only and dest is blockchain, allow
    // (event graph → blockchain is the safe direction)
    true
}

// ── Production protocol handler marker types (type-system.md §10.4) ──
// Each marker type declares the barb set of its corresponding production
// protocol handler. The dwowd crate independently implements ExhibitsBarb
// on the real handler types; these markers serve as the library-level
// reference and are gated by the cardinality snapshot test below.
//
// Per MOC review (Change 8): Mine is excluded from LinearBroadcastMarker
// because the broadcast handler receives/validates/applies/relays blocks
// but does not create or mine them — mining occurs in miner_task.

/// Marker for linear blockchain broadcast handler.
/// Barbs: Commit (block accepted), Verify (PoW + WASM validated),
/// Broadcast (gossip relay), GossipForward (network propagation).
pub struct LinearBroadcastMarker;
impl ExhibitsBarb for LinearBroadcastMarker {
    fn exhibited_barbs() -> &'static [BarbId] {
        &[BarbId::Commit, BarbId::Verify, BarbId::Broadcast, BarbId::GossipForward]
    }
}

/// Marker for linear blockchain sync handler.
/// Barbs: Verify (block validation), SyncBarrier (catch-up boundary),
/// GossipForward (network propagation).
pub struct LinearSyncMarker;
impl ExhibitsBarb for LinearSyncMarker {
    fn exhibited_barbs() -> &'static [BarbId] {
        &[BarbId::Verify, BarbId::SyncBarrier, BarbId::GossipForward]
    }
}

/// Marker for transaction relay protocol handler.
/// Barbs: Verify (tx validation), Broadcast (gossip relay),
/// GossipForward (network propagation).
pub struct ProtocolTxMarker;
impl ExhibitsBarb for ProtocolTxMarker {
    fn exhibited_barbs() -> &'static [BarbId] {
        &[BarbId::Verify, BarbId::Broadcast, BarbId::GossipForward]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BlockchainMiner;
    impl ExhibitsBarb for BlockchainMiner {
        fn exhibited_barbs() -> &'static [BarbId] {
            &[BarbId::Mine, BarbId::Commit, BarbId::Verify, BarbId::Spend, BarbId::Nullify]
        }
    }

    struct EventGraphNode;
    impl ExhibitsBarb for EventGraphNode {
        fn exhibited_barbs() -> &'static [BarbId] {
            &[BarbId::DagParent, BarbId::Broadcast, BarbId::RateLimit, BarbId::QuorumQuery]
        }
    }

    struct BlockchainObserver;
    impl ExhibitsBarb for BlockchainObserver {
        fn exhibited_barbs() -> &'static [BarbId] {
            // A read-only observer verifies proofs and relays messages. It
            // does NOT publish commitments — ↓commit is "Publishes the public
            // face of a capability" (type-system.md §1.1) and is a quarantined
            // blockchain barb (§10.4).
            &[BarbId::Verify, BarbId::GossipForward]
        }
    }

    struct ProtocolPing;
    impl ExhibitsBarb for ProtocolPing {
        fn exhibited_barbs() -> &'static [BarbId] {
            // PingPong is a pure keepalive — no blockchain or event-graph barbs.
            &[]
        }
    }

    struct ProtocolVersion;
    impl ExhibitsBarb for ProtocolVersion {
        fn exhibited_barbs() -> &'static [BarbId] {
            // Session establishment — no blockchain or event-graph barbs.
            &[]
        }
    }

    struct ProtocolAddress;
    impl ExhibitsBarb for ProtocolAddress {
        fn exhibited_barbs() -> &'static [BarbId] {
            // Address discovery + relay = gossip forwarding. No blockchain
            // or event-graph barbs (↓gossip-forward is a concurrency barb).
            &[BarbId::GossipForward]
        }
    }

    struct ProtocolSeed;
    impl ExhibitsBarb for ProtocolSeed {
        fn exhibited_barbs() -> &'static [BarbId] {
            // Seed sync is hostlist bootstrapping — gossip forwarding only.
            &[BarbId::GossipForward]
        }
    }

    /// Barb-set cardinality snapshot (type-system.md §10.5 — metric M4).
    /// An unreviewed change to the declared barb sets of production protocol
    /// handlers SHALL fail this test. The snapshot is the evidence that the
    /// 22-dimension space has not drifted toward a degenerate absorber
    /// (per-channel barb-set cardinality approaching the union of both
    /// path-sets). When a new handler legitimately declares additional
    /// barbs, this test is the single point of review — update the expected
    /// value here with the MoC decision recorded in the commit message.
    #[test]
    fn test_notify_on_barb_set_growth() {
        // ── P2P core handlers ──
        assert_eq!(BlockchainMiner::exhibited_barbs().len(), 5);
        assert_eq!(EventGraphNode::exhibited_barbs().len(), 4);
        assert_eq!(BlockchainObserver::exhibited_barbs().len(), 2);
        // ── Production protocol handler fixtures ──
        assert_eq!(ProtocolPing::exhibited_barbs().len(), 0);
        assert_eq!(ProtocolVersion::exhibited_barbs().len(), 0);
        assert_eq!(ProtocolAddress::exhibited_barbs().len(), 1);
        assert_eq!(ProtocolSeed::exhibited_barbs().len(), 1);
        // ── Production marker types (Change 8) ──
        assert_eq!(LinearBroadcastMarker::exhibited_barbs().len(), 4);
        assert_eq!(LinearSyncMarker::exhibited_barbs().len(), 3);
        assert_eq!(ProtocolTxMarker::exhibited_barbs().len(), 3);
    }

    #[test]
    fn test_blockchain_to_eventgraph_blocked() {
        // Blockchain → event-graph: blocked because blockchain has Spend/Nullify/Commit
        assert!(!bridge_safe::<BlockchainMiner, EventGraphNode>());
    }

    #[test]
    fn test_eventgraph_to_blockchain_allowed() {
        // Event-graph → blockchain: allowed
        assert!(bridge_safe::<EventGraphNode, BlockchainMiner>());
    }

    #[test]
    fn test_observer_to_eventgraph_allowed() {
        // Observer has no blockchain barbs (no Spend/Nullify/Commit/Mine)
        // but has GossipForward → allowed through
        assert!(!BlockchainObserver::has_blockchain_barbs());
        assert!(bridge_safe::<BlockchainObserver, EventGraphNode>());
    }

    /// Converse witness of the §10.4 SHALL NOT: a process that publishes
    /// commitments (↓commit) is a blockchain-barb carrier and is BLOCKED
    /// from event-graph channels — commits cross only via the dedicated
    /// typed bridge (`bridge_chain_evg`, see bridge_channel.rs).
    #[test]
    fn test_commit_publisher_to_eventgraph_blocked() {
        struct CommitPublisher;
        impl ExhibitsBarb for CommitPublisher {
            fn exhibited_barbs() -> &'static [BarbId] {
                &[BarbId::Commit, BarbId::Verify, BarbId::GossipForward]
            }
        }
        assert!(CommitPublisher::has_blockchain_barbs());
        assert!(!bridge_safe::<CommitPublisher, EventGraphNode>());
    }
}
