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

//! L2 sync boundary types.
//!
//! Spec: sync-protocol.md §1 (process net boundary types), §9 (inherent safety re-lift).
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

/// How strictly this node filters peers by genesis — the model's third parameter (`OBL-C29`).
///
/// The three modes are `chain_validation_model.py:apply_genesis_filter`'s, not invented here: `Off` for
/// tests and for a node that must not filter at all, `Relaxed` never blocks sync, `Strict` lets a
/// mismatch empty the peer set. The model is the specification, so they are ported with its names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenesisFilterMode {
    /// Accept every peer; filter nothing.
    Off,
    /// Filter, but fall back to every peer when the result would be empty — never block sync.
    Relaxed,
    /// Filter, and keep an empty result empty: a node with no compatible peer does not sync.
    Strict,
}

/// Filter peer tips down to the peers this node can sync from — `apply_genesis_filter`, ported from
/// `contrib/model/chain_validation_model.py` (`OBL-C29`).
///
/// Returns the **indices** of the compatible tips, so the caller maps them back to the peers it collected
/// them from. `our_genesis` is this node's genesis hash, or `None` while it has none (height 0).
///
/// Why the port, and why here: the model's docstring says it "matches `consensus_linear.rs:181-258`", but
/// the Rust side implemented only Path A — and in a different place (the per-connection handshake, where
/// the *peer set* is not visible), which is why Path B was deferred there with a reason that still holds:
/// a plurality vote is a multi-peer decision. The peer set *is* visible here, and `PeerTip` has carried
/// `genesis_hash` since the boundary type was written while nothing read it.
///
/// Path B's tie-break is the model's and is load-bearing, not defensive: on equal counts a `Some(hash)`
/// beats `None`, because otherwise the order of a map decides whether the only peer holding a real genesis
/// is filtered out. A `BTreeMap` rather than a `HashMap` for the same reason the model replaced its
/// `Counter` with an explicit sort — the answer must not depend on iteration order at all.
pub fn apply_genesis_filter(
    our_genesis: Option<BlockHash>,
    peer_tips: &[PeerTip],
    mode: GenesisFilterMode,
) -> Vec<usize> {
    if mode == GenesisFilterMode::Off {
        return (0..peer_tips.len()).collect();
    }

    // Path A: this node holds a genesis, so chain identity is an exact hash comparison.
    if let Some(ours) = our_genesis {
        let filtered: Vec<usize> = (0..peer_tips.len())
            .filter(|&i| peer_tips[i].genesis_hash.as_ref() == Some(&ours))
            .collect();
        if mode == GenesisFilterMode::Relaxed && filtered.is_empty() {
            return (0..peer_tips.len()).collect();
        }
        return filtered;
    }

    // Path B: this node holds no genesis, so there is nothing to compare against and the peers vote on
    // which genesis is the chain's. Every peer presenting the winner is kept — including the `None`
    // voters when `None` wins, which is the bootstrapping case the handshake's own deferral describes.
    let mut votes: std::collections::BTreeMap<Option<BlockHash>, usize> = std::collections::BTreeMap::new();
    for tip in peer_tips {
        *votes.entry(tip.genesis_hash.clone()).or_insert(0) += 1;
    }
    // `max_by_key` with `(count, is_some)` is the model's `sorted_votes` key: count first, `Some` over
    // `None` on a tie. (An empty input has no candidate and yields no peers, which is what the model's
    // `if not sorted_votes: return list(peer_tips)` returns for an empty list too.)
    let Some((winner, _)) = votes.iter().max_by_key(|(hash, count)| (**count, hash.is_some())) else {
        return Vec::new();
    };
    let winner = winner.clone();
    (0..peer_tips.len())
        .filter(|&i| peer_tips[i].genesis_hash == winner)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_sdk::blockchain::BlockHeight;

    fn tip(height: u64, hash: BlockHash, genesis_hash: Option<BlockHash>) -> Tip {
        Tip { height: BlockHeight::new(height), hash, genesis_hash }
    }

    /// §7 re-lift validation #1: `u64::MAX` height is the uninitialized sentinel.
    #[test]
    fn test_peer_tip_rejects_sentinel_height() {
        let t = tip(u64::MAX, BlockHash::zero(), None);
        assert!(PeerTip::from_tip(&t).is_err(), "u64::MAX height must be rejected");
    }

    /// §8.2.1: height-0 + zero-hash is the valid "no blocks" sentinel.
    #[test]
    fn test_peer_tip_zero_height_zero_hash_sentinel() {
        let t = tip(0, BlockHash::zero(), None);
        let pt = PeerTip::from_tip(&t).expect("zero-height/zero-hash sentinel is valid");
        assert!(pt.hash.is_zero());
        assert!(pt.genesis_hash.is_none());
    }

    /// §7 re-lift validation #3: positive height requires a genesis hash.
    #[test]
    fn test_peer_tip_missing_genesis_at_positive_height() {
        let h = BlockHash::from_hash(blake3::Hash::from_bytes([0xAAu8; 32]));
        let t = tip(5, h, None);
        assert!(
            PeerTip::from_tip(&t).is_err(),
            "positive height without genesis must be rejected"
        );
    }

    #[test]
    fn test_peer_tip_valid_roundtrip() {
        let h = BlockHash::from_hash(blake3::Hash::from_bytes([0xAAu8; 32]));
        let g = BlockHash::from_hash(blake3::Hash::from_bytes([0xBBu8; 32]));
        let t = tip(5, h.clone(), Some(g.clone()));
        let pt = PeerTip::from_tip(&t).expect("valid tip must re-lift");
        assert_eq!(pt.height, BlockHeight::new(5));
        assert_eq!(pt.hash, h);
        assert_eq!(pt.genesis_hash, Some(g));
    }

    // ── `OBL-C29`: the genesis filter, ported from the model ─────────────────────────────────────
    //
    // Built through `PeerTip::from_tip` rather than as struct literals, because that is the boundary
    // constructor and these are boundary values (§7 obligation #1). Note the consequence: a peer "with no
    // genesis" must be at height 0, since `from_tip` rejects a positive height without one — which is
    // exactly the bootstrapping-joiner case Path B exists for.

    fn genesis(tag: u8) -> BlockHash {
        BlockHash::from_hash(blake3::Hash::from_bytes([tag; 32]))
    }

    fn peer_tip(height: u64, genesis_hash: Option<BlockHash>) -> PeerTip {
        let h = if height == 0 { BlockHash::zero() } else { genesis(height as u8) };
        PeerTip::from_tip(&tip(height, h, genesis_hash)).expect("a valid tip for the boundary")
    }

    /// Path A: with a genesis of our own, chain identity is an exact comparison — and `Relaxed` is the
    /// mode that stops that comparison from ever blocking sync.
    #[test]
    fn test_genesis_filter_path_a_compares_identity() {
        let ours = genesis(0xAA);
        let other = genesis(0xBB);
        let tips = vec![
            peer_tip(10, Some(ours.clone())),
            peer_tip(11, Some(other.clone())),
            peer_tip(0, None),
        ];

        assert_eq!(
            apply_genesis_filter(Some(ours.clone()), &tips, GenesisFilterMode::Strict),
            vec![0],
            "strict: only the peer whose genesis is ours is compatible"
        );
        assert_eq!(
            apply_genesis_filter(Some(ours.clone()), &tips, GenesisFilterMode::Relaxed),
            vec![0],
            "relaxed: a non-empty result is the same result"
        );

        // Where the two modes actually differ: no compatible peer at all.
        let strangers = vec![peer_tip(11, Some(other)), peer_tip(0, None)];
        assert!(
            apply_genesis_filter(Some(ours.clone()), &strangers, GenesisFilterMode::Strict).is_empty(),
            "strict: with no compatible peer the node does not sync"
        );
        assert_eq!(
            apply_genesis_filter(Some(ours), &strangers, GenesisFilterMode::Relaxed).len(),
            2,
            "relaxed: the fallback is every peer — never blocking sync is this mode's whole purpose"
        );
    }

    /// Path B, the model's own tie-breaker test (`contrib/model/chain_validation_model.py`, "Some(hash)
    /// beats None"): at height 0 a peer holding a real genesis and a peer holding none are tied on votes,
    /// and `Some(hash)` must win — otherwise the only peer that could give the node a genesis is the one
    /// filtered out, and the node syncs from nobody.
    #[test]
    fn test_genesis_filter_plurality_tie_break_prefers_some() {
        let h = genesis(0x11);
        let tips = vec![peer_tip(68, Some(h.clone())), peer_tip(0, None)];

        let kept = apply_genesis_filter(None, &tips, GenesisFilterMode::Strict);
        assert_eq!(kept, vec![0], "Some(hash) must beat None on an equal count — the model's tie-breaker");
        assert_eq!(
            tips[kept[0]].genesis_hash.as_ref(),
            Some(&h),
            "and the winner is the real genesis, not the sentinel"
        );
    }

    /// The plurality proper: the genesis most peers present is the chain's, and every peer presenting it
    /// is kept.
    #[test]
    fn test_genesis_filter_plurality_majority_wins() {
        let a = genesis(0x21);
        let b = genesis(0x22);
        let tips = vec![
            peer_tip(5, Some(a.clone())),
            peer_tip(6, Some(a)),
            peer_tip(7, Some(b)),
        ];
        assert_eq!(
            apply_genesis_filter(None, &tips, GenesisFilterMode::Strict),
            vec![0, 1],
            "the genesis two peers present is the chain's; the third peer is filtered out"
        );
    }

    /// `Off` filters nothing — the control that the tests above are not measuring a function which simply
    /// keeps every peer.
    #[test]
    fn test_genesis_filter_off_keeps_every_peer() {
        let tips = vec![peer_tip(10, Some(genesis(0xBB))), peer_tip(0, None)];
        assert_eq!(
            apply_genesis_filter(Some(genesis(0xAA)), &tips, GenesisFilterMode::Off),
            vec![0, 1]
        );
    }
}
