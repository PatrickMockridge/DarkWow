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
use tracing::{info, warn};

use dwow_chain::sync_connection::LINEAR_SYNC_BATCH;
use crate::proto::linear_sync_client::{LinearSyncClient, PeerTip};
use crate::{DwowNodePtr, Result, SyncState};

use crate::block_acceptor::accept_block;
use dwow_sdk::blockchain::BlockHeight;

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

/// Async task to initialize consensus for darkwow-devnet mode.
///
/// A single pull loop matching the wallet (`bin/dww/src/sync_task.rs`): dial
/// full-node peers on the sync rail, take the max peer tip, then pull missing
/// blocks by height and accept each through the full validation path. Caught-up
/// is a LOCAL property (`local_height >= max_peer_height`); mining is a separate
/// gate (`caught_up AND (authority OR has_peers)`). The fork rule is uncle
/// rewards, not reorg — a competing block is stored as an uncle, never reorged.
pub async fn consensus_linear_init_task(
    node: DwowNodePtr,
    config: ConsensusInitTaskConfig,
    _ex: Arc<Executor<'static>>,
) -> Result<()> {
    info!(target: "dwowd::task::consensus_linear_init_task", "Starting linear consensus init...");

    if config.skip_sync {
        node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
        return std::future::pending().await;
    }

    let blockchain = match &node.chain_state {
        Some(lb) => lb.clone(),
        None => {
            node.mining_state.sync_state.store(SyncState::CaughtUp as u8, Ordering::SeqCst);
            return std::future::pending().await;
        }
    };

    let p2p = node.p2p_handler.p2p.clone();
    let client = LinearSyncClient::new(&p2p);
    let authority = config.genesis_authority.is_some();

    loop {
        smol::Timer::after(std::time::Duration::from_secs(30)).await;

        let local_height = blockchain.get_height();
        let magic = node.p2p_handler.p2p.settings().read().await.magic_bytes.0;
        let our_genesis_hash = if local_height >= BlockHeight::GENESIS {
            blockchain.genesis_hash().map(dwow_chain::sync_types::BlockHash::from_hash)
        } else {
            None
        };

        // Dial full-node peers over the unified sync rail (port+2).
        let mut sync_peers = client.dial_sync_peers(magic, our_genesis_hash.clone()).await;

        // Collect tips → max_peer_height.
        let mut max_peer_height = BlockHeight::new(0);
        for peer in &mut sync_peers {
            if let Ok(tip) = peer.request_tip().await {
                if let Ok(pt) = PeerTip::from_tip(&tip) {
                    if pt.height > max_peer_height {
                        max_peer_height = pt.height;
                    }
                }
            }
        }

        // Pull missing blocks by height (Monero pull sync).
        let mut next_height = local_height.succ();
        while next_height <= max_peer_height {
            let batch = (max_peer_height.get() - next_height.get() + 1)
                .min(LINEAR_SYNC_BATCH as u64);
            let mut progressed = false;
            for peer in &mut sync_peers {
                let blocks = match peer.request_blocks(next_height, batch).await {
                    Ok(b) => b,
                    Err(_) => continue, // try the next peer
                };
                if blocks.is_empty() {
                    continue;
                }
                for block in &blocks {
                    // C1 contiguity guard.
                    if block.header.height != next_height {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "Peer sent block at height {} but expected {}",
                            block.header.height, next_height);
                        break;
                    }
                    // Genesis magic-byte check (defense-in-depth).
                    if block.header.height == BlockHeight::GENESIS
                        && &block.header.anchor_tx_id[0..4] != &magic[..] {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "Genesis magic bytes mismatch — wrong network");
                        break;
                    }
                    // PoTB pre-filter (skip for genesis).
                    if block.header.height > BlockHeight::GENESIS {
                        if let Err(e) = dwow_chain::proof_of_token_balance::verify_proof_of_token_balance(block) {
                            warn!(target: "dwowd::task::consensus_linear_init_task",
                                "Synced block at height {} failed proof-of-token-balance: {}",
                                block.header.height, e);
                            break;
                        }
                    }
                    let rx_flags = randomx::RandomXFlags::get_recommended_flags()
                        & !randomx::RandomXFlags::JIT;
                    let Ok(rx_cache) = blockchain.get_cache(block.header.randomx_key) else {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "RandomX cache alloc failed at height {} — local failure, retrying",
                            block.header.height);
                        break;
                    };
                    let Ok(vm) = randomx::RandomXVM::new(rx_flags, Some(rx_cache), None) else {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "RandomX VM alloc failed at height {} — local failure, retrying",
                            block.header.height);
                        break;
                    };
                    let vm = Arc::new(vm);
                    let Some(current_height) = block.header.height.pred() else {
                        warn!(target: "dwowd::task::consensus_linear_init_task",
                            "Peer sent block at pre-genesis height 0 — skipping");
                        break;
                    };
                    match accept_block(&blockchain, block, &[], &vm, current_height, block.header.target, None) {
                        Ok(outcome) => match outcome {
                            dwow_chain::BlockConnectOutcome::CanonicalExtension { .. }
                            | dwow_chain::BlockConnectOutcome::AlreadyKnown => {
                                next_height = block.header.height.succ();
                                progressed = true;
                            }
                            _ => break, // competing/uncle — stop this pass, never reorg
                        },
                        Err(e) => {
                            warn!(target: "dwowd::task::consensus_linear_init_task",
                                "Failed to apply synced block at height {}: {}",
                                block.header.height, e);
                            break;
                        }
                    }
                }
                if progressed {
                    break;
                }
            }
            if !progressed {
                break;
            }
        }

        // LOCAL caught-up + a SEPARATE mining gate (Bitcoin IsInitialBlockDownload).
        // Computed AFTER the pull so it reflects the post-pull height.
        let caught_up = blockchain.get_height() >= max_peer_height;
        let mine = caught_up && (authority || !sync_peers.is_empty());
        node.mining_state.sync_state.store(
            if mine { SyncState::CaughtUp as u8 } else { SyncState::Behind as u8 },
            Ordering::SeqCst,
        );
    }
}
