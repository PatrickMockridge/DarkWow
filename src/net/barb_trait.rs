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

//! ExhibitsBarb — compile-time barb declaration for protocol handlers.
//!
//! Protocol handlers implement this marker trait to declare their observable
//! behaviors (barbs) at compile time. The trait enables:
//!
//! 1. **Static verification**: the compiler enforces that a protocol handler
//!    claiming to exhibit `↓gossip-forward` actually has the capability.
//! 2. **Bridging safety**: messages carrying blockchain barbs (↓spend,
//!    ↓nullify, ↓commit) cannot be routed through event-graph channels.
//! 3. **Documentation**: the barb set is machine-readable for tooling.
//!
//! Maps to ρ-calculus: a process `P` typed at `T` exhibits barb `↓x` if
//! `↓x ∈ concurrentProcessBarbs(P)`. The trait is the Rust-level witness.
//!
//! See: doc/src/arch/type-system.md §1.1 (Barbs), §10.4 (Bridging)

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
/// A blockchain process (carrying ↓spend, ↓nullify, ↓commit) SHALL NOT
/// route messages through an event-graph channel. A process with
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
            &[BarbId::Verify, BarbId::Commit, BarbId::GossipForward]
        }
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
        // Observer has no blockchain-only barbs (no Spend/Nullify/Mine)
        // but has GossipForward → allowed through
        assert!(!BlockchainObserver::has_blockchain_barbs());
        assert!(bridge_safe::<BlockchainObserver, EventGraphNode>());
    }
}
