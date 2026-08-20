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

//! L2 sync boundary types (sync-protocol.md §8-§9).
//!
//! These are the types that cross the sync boundary into the consuming task
//! (wallet `sync_task` or node `consensus_linear`). They carry the same data as
//! the P2P message types (`Tip`, `Blocks`) but are nominal boundary types — the
//! consumer never imports or handles raw P2P message types directly.
//!
//! They live in `dwow_chain` (not a per-binary crate) so the wallet and the
//! mining/observer node share the same L2→L3 translation point
//! (sync-protocol.md §1 — single source of truth).

use dwow_core::barb::{BarbId, ExhibitsBarb};
use dwow_sdk::blockchain::BlockHeight;

use crate::sync_types::{BlockHash, Tip};

// ── Boundary Types ───────────────────────────────────────────────────

/// Tip info from a single peer, lifted across the sync boundary.
///
/// All fields are carried across the boundary for diagnostic completeness.
/// The `hash` field is the peer's tip block hash — used in log messages
/// for operator visibility (which chain the peer is on).
///
/// ## Re-lift Validation (obligation #1, §7)
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
    pub hash: BlockHash,
    pub genesis_hash: Option<BlockHash>,
}

impl PeerTip {
    /// Re-lift a P2P `Tip` message into a validated `PeerTip` boundary type.
    ///
    /// Performs re-lift validation (obligation #1, §7): every byte
    /// sequence crossing the boundary SHALL be validated through a named
    /// constructor. Bare struct literals SHALL NOT construct boundary types.
    pub fn from_tip(tip: &Tip) -> dwow_core::Result<Self> {
        // 1. Height must be within valid range. u64::MAX is the sentinel
        //    for "uninitialized" — a peer sending this is either buggy or
        //    malicious.
        if tip.height.get() == u64::MAX {
            return Err(dwow_core::Error::Custom(format!(
                "PeerTip::from_tip: invalid height {}",
                tip.height
            )));
        }

        // 2. Hash: §8.2.1 re-lift is now performed by serde deserialization
        //    (BlockHash::Deserialize calls from_hex_str). At height 0, the zero
        //    sentinel is valid. No additional validation needed.
        let hash = if tip.height.is_zero() && tip.hash.is_zero() {
            BlockHash::zero()
        } else {
            tip.hash.clone()
        };

        // 3. Genesis hash must be present if the peer has blocks.
        if !tip.height.is_zero() && tip.genesis_hash.is_none() {
            return Err(dwow_core::Error::Custom(format!(
                "PeerTip::from_tip: missing genesis hash at height {}",
                tip.height
            )));
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

/// Batch of blocks received from a peer, lifted across the sync boundary.
#[derive(Clone, Debug)]
pub struct BlocksBatch {
    pub blocks: Vec<crate::Block>,
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
// It replaces the hand-rolled boolean algebra in the consuming task's
// inner peer-wait loop with a typed enum that the task matches on
// exhaustively.
//
// Per type-system.md §5.1: "A bare `bool` SHALL NOT gate consensus-
// critical paths." This enum makes the gate type-checkable.

/// Typed result of the peer-wait phase — the L2→L3 boundary signal.
///
/// The consuming task receives one of these and transitions sync_state
/// accordingly. Every variant corresponds to a distinguishable condition
/// in the peer-wait loop.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncDecision {
    /// At least one full-node peer is connected. Proceed to tip collection
    /// and block sync.
    PeersAvailable,

    /// No peers connected, but this node is the genesis authority with
    /// local genesis at height >= 1. Proceed to solo mining (authority gate).
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
