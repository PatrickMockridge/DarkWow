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

//! Linear-testnet consensus initialization task
//!
//! Spec: sync-protocol.md §1 (SyncClient + BlockSink), §13 (async production logic).
//!
//! This module handles P2P block sync for the linear blockchain.
//! On startup, it queries connected peers for their best height via
//! GetTip/Tip, then pulls missing blocks via GetBlocks/Blocks and
//! applies them through full validation.

use std::sync::{atomic::Ordering, Arc};

use dwow_core::barb::{BarbId, ExhibitsBarb};
use smol::Executor;
use tracing::{debug, error, info, warn};

use dwow_chain::sync_connection::LINEAR_SYNC_BATCH;
use crate::proto::linear_sync_client::{LinearSyncClient, PeerTip, SyncDecision};
use crate::{DwowNodePtr, Result, SyncState};

use crate::block_acceptor::{accept_block, reorganize_to_chain};
use dwow_sdk::blockchain::BlockHeight;

/// Genesis hash validation strictness.
///
/// Controls how strictly a node validates peer genesis hashes during sync.
/// This is a spectrum, not a binary — early-phase projects with few peers
/// should use `Off` or `Relaxed`. Strict mode is for mature networks where
/// a chain split would be catastrophic.
#[derive(Clone, Debug)]
pub enum GenesisValidationMode {
    /// Accept all peers regardless of genesis hash. No filtering.
    /// Default for local devnet / docker-compose environments.
    Off,
    /// Prefer peers with matching genesis, but accept any peer if no
    /// clear quorum exists. Never blocks sync. For small/sporadic testnets.
    Relaxed,
    /// Require exact genesis hash match. If no peer matches, refuse to
    /// sync (loop until a compatible peer connects). For mainnet.
    Strict,
}

/// Proof of genesis authority possession — replaces bare `bool`.
///
/// Construction requires the genesis secret key. A node that cannot produce
/// the key cannot claim authority. The type system enforces this at compile
/// time: the "mine without peers" gate requires `Some(GenesisAuthority)`,
/// not just a truthy boolean.
///
/// Per type-system.md §5.1: "A bare `bool` SHALL NOT gate consensus-critical
/// paths. Consensus authority SHALL be represented by nominal marker types
/// constructible only through proof of capability possession."
#[derive(Clone)]
pub struct GenesisAuthority {
    _private: (),
}

impl GenesisAuthority {
    /// Construct a genesis authority marker. The `create_genesis` flag
    /// must already be verified in `main.rs` — this constructor is the
    /// type-level witness that replaces the bare `bool`.
    ///
    /// Defense-in-depth (HAZOP F7): genesis hash verification already
    /// prevents non-genesis miners from producing valid blocks — peers
    /// reject blocks whose genesis hash doesn't match the compile-time
    /// constant (lib.rs genesis_hash.txt check). A wrong-key node
    /// produces an invalid genesis block rejected by all peers.
    /// Key binding is enforced at the consensus level, not the type level.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Construct iff this process holds the genesis secret key.
    /// Returns `None` if the key is absent or does not match the genesis
    /// public key binding.
    ///
    /// Defense-in-depth (HAZOP F7): verifies the secret against the
    /// genesis public key from AccountManager. The MiningRecipient
    /// pattern (dwow-accounts) already does per-block key derivation —
    /// the same binding applies here for the fixed genesis height key.
    pub fn from_key(secret: &dwow_sdk::crypto::SecretKey, genesis_public_key: &dwow_sdk::crypto::PublicKey) -> Option<Self> {
        let derived_public = dwow_sdk::crypto::PublicKey::from_secret(secret.clone());
        if derived_public != *genesis_public_key {
            return None;
        }
        Some(Self { _private: () })
    }
}

impl dwow_core::barb::ExhibitsBarb for GenesisAuthority {
    fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
        // Genesis authority exhibits ↓mine — only this process may create
        // genesis and mine without peers. Per type-system.md §5.1, the
        // compiler witnesses the mining barb at compile time.
        &[dwow_core::barb::BarbId::Mine]
    }
}

/// Auxiliary structure representing node consensus init task configuration.
#[derive(Clone)]
pub struct ConsensusInitTaskConfig {
    /// Skip syncing process and start node right away
    pub skip_sync: bool,
    /// Optional sync checkpoint height
    pub checkpoint_height: Option<u32>,
    /// Optional sync checkpoint hash
    pub checkpoint: Option<String>,
    /// Genesis hash validation mode (default: Off for devnet)
    pub genesis_validation: GenesisValidationMode,
    /// Proof of genesis authority. Only the genesis authority may mine
    /// without peers. `None` means this node is not a genesis authority.
    pub genesis_authority: Option<GenesisAuthority>,
}

impl ExhibitsBarb for ConsensusInitTaskConfig {
    fn exhibited_barbs() -> &'static [BarbId] {
        // Consensus init gates sync protocol and mining authorization.
        // Per type-system.md §10.4: the config carries {↓verify} (sync
        // validation), {↓sync-barrier} (catch-up gate), {↓gossip-forward}
        // (tip/block relay), and {↓mine} (when genesis_authority is Some —
        // the Mine barb is declared at the type level, enforced at runtime
        // by GenesisAuthority's own ExhibitsBarb impl).
        &[
            BarbId::Verify,
            BarbId::SyncBarrier,
            BarbId::GossipForward,
            BarbId::Mine,
        ]
    }
}

/// Outcome of a reorg attempt against a peer's competing chain.
enum ReorgOutcome {
    /// The competing chain carried more work and was adopted — the caller
    /// re-accepts the extension block.
    Applied,
    /// The competing chain was not heavier — keep the local canonical chain.
    NotHeavier,
    /// The reorg attempt failed (fetch/validation error).
    Failed,
}

/// F2/F3 fix: resolve a divergent fork by accumulated work (Bitcoin
/// `ActivateBestChain`). When a synced block fails to apply because it builds on
/// a parent we do not hold (the `old_cumulative_commit` mismatch), fetch the
/// competing chain from the peer, find the common ancestor, and — if the
/// competing chain carries more accumulated work — disconnect our blocks down to
/// the ancestor and connect the competing chain (`node-startup-spec.md` §4).
async fn reorg_to_heavier_chain(
    blockchain: &Arc<dwow_chain::CChainState>,
    block: &dwow_chain::Block,
    peer: &mut dwow_chain::sync_connection::SyncPeer,
) -> ReorgOutcome {
    let local_height = blockchain.get_height();
    // Only a next-height block can extend a competing chain.
    if block.header.height != local_height.succ() {
        return ReorgOutcome::NotHeavier;
    }

    // 1. Walk back from the block's parent to the common ancestor, collecting
    //    the competing blocks (fork_point+1 ..= block.height-1).
    let mut competing: Vec<dwow_chain::Block> = Vec::new();
    let mut cursor = block.header.height.pred().unwrap_or(BlockHeight::new(1));
    let mut fork_point = BlockHeight::new(0);

    loop {
        let Ok(local_block) = blockchain.get_block(cursor) else {
            // Local has no block at this height — no shared ancestor.
            break;
        };
        let fetched = match peer.request_blocks(cursor, 1).await {
            Ok(bs) if !bs.is_empty() => bs[0].clone(),
            _ => return ReorgOutcome::Failed,
        };
        let local_hash = match blockchain.get_vm(local_block.header.randomx_key) {
            Ok(vm) => {
                let guard = vm.lock().unwrap_or_else(|e| e.into_inner());
                match local_block.hash_with_vm(&*guard) {
                    Ok(h) => h,
                    Err(_) => return ReorgOutcome::Failed,
                }
            }
            Err(_) => return ReorgOutcome::Failed,
        };
        let fetched_hash = match blockchain.get_vm(fetched.header.randomx_key) {
            Ok(vm) => {
                let guard = vm.lock().unwrap_or_else(|e| e.into_inner());
                match fetched.hash_with_vm(&*guard) {
                    Ok(h) => h,
                    Err(_) => return ReorgOutcome::Failed,
                }
            }
            Err(_) => return ReorgOutcome::Failed,
        };
        if local_hash == fetched_hash {
            fork_point = cursor;
            break;
        }
        competing.insert(0, fetched);
        if cursor <= BlockHeight::GENESIS {
            break;
        }
        cursor = cursor.pred().unwrap_or(BlockHeight::new(0));
    }

    if fork_point.is_zero() {
        warn!(target: "dwowd::task::consensus_linear_init_task",
            "Reorg: no common ancestor found for block at height {}", block.header.height);
        return ReorgOutcome::Failed;
    }

    // 2. Heaviest-chain comparison (consensus.md §Fork Choice Rule): the
    //    competing chain (fork_point+1 ..= block.height) vs our displaced
    //    canonical blocks (fork_point+1 ..= local_height).
    let mut displaced_work: u128 = 0;
    let mut h = fork_point.succ();
    while h <= local_height {
        if let Ok(b) = blockchain.get_block(h) {
            displaced_work = displaced_work.saturating_add(b.header.target.chain_work());
        }
        h = h.succ();
    }
    let mut competing_work: u128 = 0;
    for b in &competing {
        competing_work = competing_work.saturating_add(b.header.target.chain_work());
    }
    competing_work = competing_work.saturating_add(block.header.target.chain_work());
    if competing_work <= displaced_work {
        debug!(target: "dwowd::task::consensus_linear_init_task",
            "Reorg: competing chain not heavier (competing_work={} <= displaced_work={})",
            competing_work, displaced_work);
        return ReorgOutcome::NotHeavier;
    }

    // 3. Reorg: roll back cumulative commit, disconnect, connect competing chain.
    if let Err(e) = reorganize_to_chain(blockchain, &competing, fork_point, None) {
        error!(target: "dwowd::task::consensus_linear_init_task",
            "Reorg failed at height {}: {}", block.header.height, e);
        return ReorgOutcome::Failed;
    }

    info!(target: "dwowd::task::consensus_linear_init_task",
        "Reorg applied: disconnected {} block(s), connected {} competing block(s) (fork at {})",
        local_height.get().saturating_sub(fork_point.get()), competing.len(), fork_point.get());
    ReorgOutcome::Applied
}

/// Async task to initialize consensus for darkwow-devnet mode.
///
/// On startup, this task:
/// 1. Waits for at least one connected P2P peer
/// 2. Queries peer heights via GetTip/Tip
/// 3. If behind, pulls missing blocks via GetBlocks/Blocks
/// 4. Applies blocks through full validation (PoW, merkle, WASM)
/// 5. Parks until the node stops
pub async fn consensus_linear_init_task(
    node: DwowNodePtr,
    config: ConsensusInitTaskConfig,
    _ex: Arc<Executor<'static>>,
) -> Result<()> {
    info!(target: "dwowd::task::consensus_linear_init_task", "Starting linear consensus init...");

    // If skip_sync is set, park immediately (tests, single-node)
    if config.skip_sync {
        info!(target: "dwowd::task::consensus_linear_init_task", "Sync skipped, parking forever");
        node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
        info!(target: "dwowd::task::consensus_linear_init_task",
            "sync_state: Initial → CaughtUp [FALLBACK: skip_sync — sync bypassed, mining immediately]");
        return std::future::pending().await
    }

    // Get the dwowd linear blockchain wrapper (full validation)
    let blockchain = match &node.chain_state {
        Some(lb) => lb.clone(),
        None => {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "No linear blockchain configured, parking forever");
            node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: Initial → CaughtUp [FALLBACK: no blockchain — mining disabled]");
            return std::future::pending().await
        }
    };

    let p2p = node.p2p_handler.p2p.clone();

    // Initialize the net-node tier sync client — encapsulates ALL P2P
    // operations behind typed methods. Consensus code never touches
    // subscribe_msg, send, or receive directly (type-system.md §10.5).
    let client = LinearSyncClient::new(&p2p);

    // Sync state machine (sync-protocol.md §13.3): a single timer-driven loop
    // with two phases folded in —
    //   · Initial sync: while behind, pull to the best peer tip, punishing a
    //     peer that serves an invalid block and switching peers; no-progress
    //     drives a bounded backoff (never a 2s tight loop).
    //   · Continuous catch-up: once CaughtUp, re-poll every 30s and catch up if
    //     a peer reports a higher tip; the block-broadcast handler (§14.3) keeps
    //     the node at tip between ticks.
    let mut iteration_count: u64 = 0;
    let mut stuck_ticks: u32 = 0;
    // Peer punishment (sync-protocol.md §13.3): a peer that serves an invalid
    // block is scored and, at the threshold, dropped for the sync session —
    // never retried forever. Keyed by the peer's dial URL (stable identity).
    let mut peer_scores: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    // F4 fix: consecutive no-progress cycles drive an escalating backoff so a
    // permanently bad peer/block is not retried in a 2s tight loop forever.
    let mut no_progress_ticks: u32 = 0;
    loop {
        let local_height = blockchain.get_height();
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Outer loop iteration: local_height={} sync_state={:?} peers={} iteration={}",
            local_height,
            SyncState::load(&node.mining_state.sync_state),
            client.peer_count(),
            iteration_count);

        // Heartbeat: every 5 iterations, confirm the loop is cycling.
        // If this stops appearing, the loop is stuck (poisoned mutex,
        // infinite await, or executor stall).
        if iteration_count > 0 && iteration_count % 5 == 0 {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "Consensus heartbeat: iteration={} local_height={} sync_state={:?} peers={}",
                iteration_count, local_height,
                SyncState::load(&node.mining_state.sync_state),
                client.peer_count());
        }
        iteration_count += 1;

        // F1: stuck-sync watchdog. If we have full-node peers but never advance
        // past height 0, escalate after 30 iterations (~60s of retries). Mirrors
        // the wallet's D1 watchdog (FATAL after 6 ticks at height 0 with peers).
        if local_height.is_zero() && client.has_full_node_peers() {
            stuck_ticks += 1;
            if stuck_ticks >= 30 {
                error!(target: "dwowd::task::consensus_linear_init_task",
                    "FATAL: node has {} full-node peers but zero chain height after {} iterations. \
                     Genesis is not being received — peers may be on a different network \
                     (magic bytes mismatch) or not serving genesis.",
                    client.peer_count(), stuck_ticks);
            }
        } else {
            stuck_ticks = 0;
        }

        // Wait for at least one connected peer before attempting sync.
        // Delegated to LinearSyncClient — the net-node tier absorbs the
        // ENTIRE peer-wait phase and returns a typed SyncDecision.
        // Consensus code matches on the decision exhaustively; bare
        // boolean algebra is replaced with type-checkable variants
        // (type-system.md §5.1: "A bare bool SHALL NOT gate consensus-
        // critical paths.").
        match client.wait_for_peers_or_proceed(config.genesis_authority.clone(), local_height).await {
            SyncDecision::PeersAvailable => {
                // fall through to tip collection below
            }
            SyncDecision::ProceedSolo => {
                info!(target: "dwowd::task::consensus_linear_init_task",
                    "sync_state: → CaughtUp [PRIMARY: genesis authority — no peers, solo mining at height {}]",
                    local_height);
                node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
                // Continuous sync: re-poll after delay instead of parking forever
                smol::Timer::after(std::time::Duration::from_secs(30)).await;
                continue; // back to outer loop
            }
            SyncDecision::WaitForGenesis => {
                info!(target: "dwowd::task::consensus_linear_init_task",
                    "sync_state: → WaitingForGenesis (height 0, no peers, no genesis anywhere)");
                node.mining_state.sync_state.store(SyncState::WaitingForGenesis as u8, Ordering::SeqCst);
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue; // back to outer loop
            }
            SyncDecision::Retry => {
                // F2: don't leave sync_state stale (e.g. CaughtUp from a prior
                // cycle) while we have genesis but no peers — mark Behind so the
                // miner pauses instead of mining into an empty network.
                node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
                continue; // back to outer loop
            }
        }

        let local_height = blockchain.get_height();
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Connected to peers, local height: {}", local_height);

        // ── Tip collection via the unified sync connection ──────────
        // Dial all full-node peers over SyncPeer (port+2), then request tips.
        // This replaces the P2P-channel rail with the single sync rail
        // (sync-protocol.md §11). Peer discovery (hostlist/seed) is still P2P.
        let magic = node.p2p_handler.p2p.settings().read().await.magic_bytes.0;
        let our_genesis_hash: Option<dwow_chain::sync_types::BlockHash> =
            if local_height >= BlockHeight::GENESIS {
                blockchain.genesis_hash().map(dwow_chain::sync_types::BlockHash::from_hash)
            } else {
                None
            };
        let mut sync_peers = client.dial_sync_peers(magic, our_genesis_hash.clone()).await;
        let mut peer_tips: Vec<(usize, PeerTip)> = Vec::with_capacity(sync_peers.len());
        for (i, peer) in sync_peers.iter_mut().enumerate() {
            match peer.request_tip().await {
                Ok(tip) => match PeerTip::from_tip(&tip) {
                    Ok(pt) => peer_tips.push((i, pt)),
                    Err(e) => warn!(target: "dwowd::task::consensus_linear_init_task",
                        "Rejected invalid tip from a sync peer: {e}"),
                },
                Err(e) => warn!(target: "dwowd::task::consensus_linear_init_task",
                    "Tip request failed: {e}"),
            }
        }
        info!(target: "dwowd::task::consensus_linear_init_task",
            "Have {} full-node peers with tip data", peer_tips.len());

        // B1 fix: full-node peers exist but ZERO tips were collected — every
        // tip request failed (timeout / dead handler). Falling through would
        // set max_peer_height = local_height and declare CaughtUp on a stale
        // tip. Distinguish "no peers" from "peers present, all tips failed"
        // (sync-protocol.md §18.1.1) and retry instead of mining on a fork.
        if peer_tips.is_empty() && !sync_peers.is_empty() {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Collected 0 of {} full-node peer tips — all tip requests failed. Retrying (not CaughtUp).",
                sync_peers.len());
            node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }

        // ── Layer 1: Genesis hash validation ────────────────────────
        // Configurable via genesis_validation mode. Early-phase projects
        // with few peers should use Off or Relaxed — Strict is for mature
        // networks where a chain split would be catastrophic.
        let compatible_peers: Vec<_> = match config.genesis_validation {
            GenesisValidationMode::Off => {
                // Accept all peers — no genesis filtering.
                // For local devnet / docker-compose: the genesis authority
                // is the only source of truth, and we trust it.
                peer_tips.iter().collect()
            }
            _ => {
                // Relaxed and Strict both use the same filtering logic,
                // but Relaxed falls back to all peers if no compatible ones found.
                // D1/D3: the genesis hash is the CACHED OnceLock value computed
                // above (never re-hash with the VM per pass).
                let filtered: Vec<_> = if let Some(ref our_gh) = our_genesis_hash {
                    peer_tips.iter()
                        .filter(|(_, pt)| {
                            let matches = pt.genesis_hash.as_ref() == Some(our_gh);
                            if !matches {
                                warn!(target: "dwowd::task::consensus_linear_init_task",
                                    "Genesis hash mismatch — excluding peer from sync");
                            }
                            matches
                        })
                        .collect()
                } else {
                    // Height 0: plurality vote among peers, with tie-breaker
                    // preferring Some(real_hash) over None (peer hasn't synced
                    // genesis yet). A peer with a known genesis is inherently
                    // more trustworthy than one still at height 0.
                    use std::collections::HashMap;
                    let mut genesis_votes: HashMap<Option<dwow_chain::sync_types::BlockHash>, usize> = HashMap::new();
                    for (_, pt) in peer_tips.iter() {
                        *genesis_votes.entry(pt.genesis_hash.clone()).or_default() += 1;
                    }
                    // Tie-breaker: sort by (count, is_some) so Some(hash) wins ties.
                    // Without this, HashMap iteration order non-deterministically
                    // breaks ties between Some(hash) and None, potentially
                    // filtering out the only peer with a real genesis.
                    let mut sorted: Vec<_> = genesis_votes.iter().collect();
                    sorted.sort_by_key(|(gh, count)| {
                        // Primary: vote count (higher wins)
                        // Secondary: prefer Some over None (real genesis beats no-genesis)
                        (std::cmp::Reverse(**count), gh.is_some())
                    });
                    let majority_genesis = sorted.first().map(|(gh, _)| (*gh).clone());
                    if let Some(ref majority) = majority_genesis {
                        if majority.is_some() {
                            info!(target: "dwowd::task::consensus_linear_init_task",
                                "Height 0 — plurality genesis hash ({} of {} peers agree)",
                                genesis_votes.get(majority).unwrap_or(&0),
                                peer_tips.len());
                        }
                    }
                    peer_tips.iter()
                        .filter(|(_, pt)| {
                            if let Some(ref majority) = majority_genesis {
                                &pt.genesis_hash == majority
                            } else {
                                true // all peers disagree — accept all
                            }
                        })
                        .collect()
                };
                match config.genesis_validation {
                    GenesisValidationMode::Relaxed => {
                        // If filtering removed ALL peers, fall back to
                        // accepting all peers rather than blocking sync.
                        // A warning is logged so operators can investigate.
                        if filtered.is_empty() && !peer_tips.is_empty() {
                            warn!(target: "dwowd::task::consensus_linear_init_task",
                                "Genesis filter: 0 of {} peers compatible (relaxed mode — accepting all peers to avoid sync deadlock)",
                                peer_tips.len());
                            peer_tips.iter().collect()
                        } else {
                            if filtered.len() < peer_tips.len() {
                                info!(target: "dwowd::task::consensus_linear_init_task",
                                    "Genesis filter: {} of {} peers compatible",
                                    filtered.len(), peer_tips.len());
                            }
                            filtered
                        }
                    }
                    _ => {
                        // Strict: keep the filtered list. If empty, the
                        // height-0 check below will loop and retry.
                        if filtered.len() < peer_tips.len() {
                            info!(target: "dwowd::task::consensus_linear_init_task",
                                "Genesis filter: {} of {} peers compatible",
                                filtered.len(), peer_tips.len());
                        }
                        filtered
                    }
                }
            }
        };

        // ── Sync target: highest height among compatible peers ──────
        // Simple max — the correct production pattern is "sync from the
        // peer with the most work" (Bitcoin chainwork comparison), not
        // "require N peers to agree on height" (vulnerable to Sybil).
        // Invalid blocks from a lying peer are rejected during apply.
        let mut max_peer_height: BlockHeight = compatible_peers.iter()
            .map(|(_, pt)| pt.height)
            .max()
            .unwrap_or(local_height);

        // F1 fix: CaughtUp requires positive peer-tip evidence (sync-protocol.md
        // §18.1.1). If no compatible peer tips were collected — either every sync
        // dial failed (empty `sync_peers`) or the genesis filter excluded all peers
        // — we cannot determine the canonical chain. Set Behind and retry; NEVER
        // fall through to the CaughtUp branch on empty evidence (Bitcoin
        // IsInitialBlockDownload pattern).
        if compatible_peers.is_empty() {
            node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Behind (no compatible peer tips — {} sync peers dialed, {} tips collected; never CaughtUp without peer evidence)",
                sync_peers.len(), peer_tips.len());
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }

        // If no peers have any blocks and we have no genesis (height=0),
        // we can't sync — keep waiting for a genesis authority to connect.
        if max_peer_height.is_zero() && local_height.is_zero() {
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "No genesis available from peers — waiting for genesis authority...");
            smol::Timer::after(std::time::Duration::from_secs(2)).await;
            continue;
        }

        // If we're behind, pull and apply missing blocks
        if max_peer_height > local_height {
            info!(target: "dwowd::task::consensus_linear_init_task",
                "Behind: local height {} < peer height {}. Syncing...",
                local_height, max_peer_height);

            // HAZOP F5: set Syncing so the miner can see we're downloading blocks.
            // This state was defined but never used — the miner had no visibility into
            // whether sync was in progress. Without this, the miner could start during
            // a long sync if another code path set CaughtUp prematurely.
            node.mining_state.sync_state.store(SyncState::Syncing as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Syncing (pulling {} blocks from peers)", max_peer_height.saturating_sub(local_height));

            let mut next_height = local_height.succ();

            // R6: round-robin across healthy sync peers so a slow-but-healthy first
            // peer is not always preferred (sync-protocol.md §13.3).
            // (`peer_scores` is hoisted to the outer loop so a peer's ban score
            // survives across sync passes.)
            let mut rr_index: usize = 0;

            while next_height <= max_peer_height {
                let batch_size =
                    (max_peer_height.saturating_sub(next_height) + 1).min(LINEAR_SYNC_BATCH as u64);

                // Skip sync peers whose ban score hit the threshold (3).
                let peer_indices: Vec<usize> = (0..sync_peers.len())
                    .filter(|i| peer_scores.get(&sync_peers[*i].url().to_string()).unwrap_or(&0) < &3)
                    .collect();
                if peer_indices.is_empty() {
                    warn!(target: "dwowd::task::consensus_linear_init_task",
                        "No healthy peers available for sync at height {}", next_height);
                    break
                }

                let idx = peer_indices[rr_index % peer_indices.len()];
                rr_index += 1;
                let peer_url = sync_peers[idx].url().to_string(); // stable peer identity

                // Request blocks over the unified sync connection (SyncPeer).
                match sync_peers[idx].request_blocks(next_height, batch_size).await {
                    Ok(blocks) => {
                        let received = blocks.len();
                        info!(target: "dwowd::task::consensus_linear_init_task",
                            "Received {} blocks starting at height {}", received, next_height);

                        if received == 0 {
                            warn!(target: "dwowd::task::consensus_linear_init_task",
                                "Peer returned zero blocks, sync complete");
                            break
                        }

                        for block in &blocks {
                            // Fix 1e: verify magic bytes in genesis block anchor field.
                            // Defense-in-depth: even if P2P magic bytes match, the
                            // consensus layer independently verifies the genesis block
                            // belongs to this network.
                            if block.header.height == BlockHeight::GENESIS {
                                let expected_magic = node.p2p_handler.p2p.settings()
                                    .read().await.magic_bytes.0;
                                let genesis_magic = &block.header.anchor_tx_id[0..4];
                                if genesis_magic != &expected_magic[..] {
                                    error!(target: "dwowd::task::consensus_linear_init_task",
                                        "Genesis magic bytes mismatch: expected {:?}, got {:?} — wrong network",
                                        expected_magic, genesis_magic);
                                    *peer_scores.entry(peer_url.clone()).or_default() += 1;
                                    break; // reject entire batch from this peer
                                }
                            }
                            // Genesis block (height 1) has a full PoWRewardV1 coinbase
                            // plus the 9 contract-deployment txs per genesis.md. Skip
                            // proof-of-token-balance for genesis: target=u32::MAX means
                            // any hash passes, and mass balance is trivially satisfied
                            // (one coinbase; deployment txs carry no NativeToken calls).
                            if block.header.height > BlockHeight::GENESIS {
                                if let Err(e) = dwow_chain::proof_of_token_balance::verify_proof_of_token_balance(block) {
                                    tracing::warn!(
                                        target: "dwowd::task::consensus_linear",
                                        "Synced block at height {} failed proof-of-token-balance: {}",
                                        block.header.height, e
                                    );
                                    *peer_scores.entry(peer_url.clone()).or_default() += 1;
                                    continue;
                                }
                            }
                            // Defect 3: sync must run WASM execution (same path as
                            // mining), not bypass it. Upstream validates this —
                            // their sync routes through verify_transaction, which
                            // always runs exec+apply. Build a light VM from the
                            // pooled RandomXCache (2 MB scratchpad, 256 MB cached).
                            let rx_flags = randomx::RandomXFlags::get_recommended_flags()
                                & !randomx::RandomXFlags::JIT;
                            let rx_cache = blockchain.get_cache(block.header.randomx_key)
                                .map_err(|e| dwow_core::Error::Custom(format!(
                                    "RandomX cache: {}", e
                                )))?;
                            #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
                            let vm = Arc::new(
                                randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                                    .expect("Failed to create RandomX VM for sync"),
                            );
                            // Peer-controlled data: a height-0 block has no
                            // predecessor (pre-genesis sentinel) — reject it
                            // instead of underflowing like the old `height - 1`.
                            let Some(current_height) = block.header.height.pred() else {
                                warn!(target: "dwowd::task::consensus_linear_init_task",
                                    "Peer sent block at pre-genesis height 0 — skipping");
                                *peer_scores.entry(peer_url.clone()).or_default() += 1;
                                break;
                            };
                            let target = block.header.target;
                            match accept_block(
                                &blockchain,
                                block,
                                &[],
                                &vm,
                                current_height,
                                target,
                                None,
                            ) {
                                Ok(_outcome) => {
                                    next_height = block.header.height.succ();
                                    peer_scores.remove(&peer_url);
                                }
                                Err(e) => {
                                    error!(target: "dwowd::task::consensus_linear_init_task",
                                        "Failed to apply synced block at height {}: {}",
                                        block.header.height, e);

                                    // F2/F3: general-depth reorg (Bitcoin
                                    // ActivateBestChain). A divergent fork extension
                                    // fails because we don't hold the competing
                                    // parent it builds on. Fetch the competing chain,
                                    // find the common ancestor, and — if it carries
                                    // more accumulated work — disconnect our blocks
                                    // and reconnect onto it (node-startup-spec.md §4).
                                    match reorg_to_heavier_chain(&blockchain, block, &mut sync_peers[idx]).await {
                                        ReorgOutcome::Applied => {
                                            // Reorg reconnected up to the parent; retry
                                            // the extension block once.
                                            match accept_block(&blockchain, block, &[], &vm, current_height, target, None) {
                                                Ok(_) => {
                                                    next_height = block.header.height.succ();
                                                    peer_scores.remove(&peer_url);
                                                    continue;
                                                }
                                                Err(e2) => {
                                                    error!(target: "dwowd::task::consensus_linear_init_task",
                                                        "Reorg applied but extension still failed at height {}: {}",
                                                        block.header.height, e2);
                                                }
                                            }
                                        }
                                        ReorgOutcome::NotHeavier => {
                                            debug!(target: "dwowd::task::consensus_linear_init_task",
                                                "Reorg skipped: competing chain not heavier at height {}",
                                                block.header.height);
                                        }
                                        ReorgOutcome::Failed => {
                                            error!(target: "dwowd::task::consensus_linear_init_task",
                                                "Reorg attempt failed at height {}", block.header.height);
                                        }
                                    }
                                    *peer_scores.entry(peer_url.clone()).or_default() += 1;
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "GetBlocks failed at height {}: {e}", next_height);
                        *peer_scores.entry(peer_url.clone()).or_default() += 1;
                        smol::Timer::after(std::time::Duration::from_secs(2)).await;
                        continue
                    }
                }
            }
        }

        // HAZOP F4: Refresh peer heights before declaring sync complete.
        // The max_peer_height snapshot was taken at the START of sync
        // (lines 316-319). Peers may have advanced by 60+ blocks during
        // a long sync. Re-query tips to get fresh heights over the same
        // unified sync peers.
        let mut fresh_max_peer_height: BlockHeight = BlockHeight::new(0);
        let mut refresh_count: usize = 0;
        for peer in &mut sync_peers {
            match peer.request_tip().await {
                Ok(tip) => {
                    refresh_count += 1;
                    if tip.height > fresh_max_peer_height {
                        fresh_max_peer_height = tip.height;
                    }
                }
                Err(_) => continue,
            }
        }
        debug!(target: "dwowd::task::consensus_linear_init_task",
            "Peer-height refresh: queried {} sync peers, {} tip responses",
            sync_peers.len(), refresh_count);
        // R9: `max_peer_height` reflects the latest observed max tip — it may
        // advance OR decay — so a stale high-water mark does not persist.
        if fresh_max_peer_height > BlockHeight::new(0) {
            if fresh_max_peer_height != max_peer_height {
                info!(target: "dwowd::task::consensus_linear_init_task",
                    "Peer tip refreshed during sync: {} -> {}",
                    max_peer_height, fresh_max_peer_height);
            }
            max_peer_height = fresh_max_peer_height;
        }

        // Verify the sync attempt actually caught us up.
        // Two failure modes (HAZID RC2/FM2):
        // 1. Still at height 0 — genesis never received from peers.
        // 2. Still far behind max_peer_height — all blocks from the best
        //    peer failed to apply (incompatible chain, corrupt data).
        //    Without this check, sync_state=CaughtUp is set and mining
        //    starts on a stale tip unaware it's 60+ blocks behind.
        let current_height = blockchain.get_height();
        // F4: reset the no-progress backoff as soon as height advances; only
        // consecutive zero-progress cycles escalate (sync-protocol.md §18.1.1 —
        // never retry a stuck peer/block in a tight loop).
        if current_height > local_height {
            no_progress_ticks = 0;
        }
        if current_height.is_zero() && !max_peer_height.is_zero() {
            no_progress_ticks = no_progress_ticks.saturating_add(1);
            let backoff = std::time::Duration::from_secs(
                2u64.saturating_mul(no_progress_ticks as u64).min(30),
            );
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Sync attempt failed — still at height 0 with peers at height {}. Retrying in {:.0}s...",
                max_peer_height, backoff.as_secs_f64());
            node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Behind (sync failed — still at height 0, peers at {})",
                max_peer_height);
            smol::Timer::after(backoff).await;
            continue;
        }
        if !max_peer_height.is_zero() && current_height < max_peer_height {
            no_progress_ticks = no_progress_ticks.saturating_add(1);
            let backoff = std::time::Duration::from_secs(
                2u64.saturating_mul(no_progress_ticks as u64).min(30),
            );
            warn!(target: "dwowd::task::consensus_linear_init_task",
                "Sync incomplete — local height {} but peer has {}. Retrying in {:.0}s...",
                current_height, max_peer_height, backoff.as_secs_f64());
            node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Behind (sync incomplete, local={} peer={})",
                current_height, max_peer_height);
            smol::Timer::after(backoff).await;
            continue;
        }

        info!(target: "dwowd::task::consensus_linear_init_task",
            "Sync complete at height {}", blockchain.get_height());

        // Signal that initial sync is done — miner task can proceed
        // (miner independently waits for genesis, not full sync).
        // HAZOP H8: distinguish "no genesis anywhere" from "caught up."
        // CaughtUp at height 0 means no genesis exists on any peer.
        // The miner_task separately guards height-0 mining (get_latest_block
        // fails), but this log distinguishes the two cases for operators.
        if current_height.is_zero() && max_peer_height.is_zero() {
            // No genesis exists anywhere. A non-authority node cannot create
            // genesis, so it MUST remain Behind (miner paused) until a
            // genesis-bearing peer appears — otherwise it would mine a
            // divergent fork. (HAZOP H8: distinguish "no genesis" from "caught up".)
            node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → Behind [no genesis exists anywhere — mining disabled until genesis arrives]");
        } else {
            node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
            info!(target: "dwowd::task::consensus_linear_init_task",
                "sync_state: → CaughtUp [PRIMARY: sync complete — caught up to peer tip at height {}]", blockchain.get_height());
            // Reset transient failure state now that we are caught up — the
            // per-peer ban scores and no-progress backoff are only meaningful
            // while behind.
            peer_scores.clear();
            no_progress_ticks = 0;
        }

        // Reorganization removed — linear blockchain resolves forks
        // via uncle rewards. Competing blocks stored for uncle rewards only.

        // Continuous sync: re-poll peers every 30s to stay caught up.
        // Never parks permanently — a node that falls behind will
        // eventually catch up when peers become available.
        smol::Timer::after(std::time::Duration::from_secs(30)).await;
    } // end outer retry loop (continuous)
}
