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

//! Linear blockchain block broadcast module
//!
//! This is a bespoke P2P block broadcasting module for the linear blockchain.
//! Key design principles:
//! - Uses ProtocolGenericHandler for message reception
//! - Simple message type with clear serialization
//! - LinearBlockchain now has interior mutability (Arc<CChainState> pattern)

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::info;

use rand::seq::SliceRandom;
use dwow_core::{
    impl_p2p_message,
    net::{
        metering::MeteringConfiguration,
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        Message, P2pPtr,
    },
    concurrency::ExecutorPtr,
    util::time::NanoTimestamp,
    Result,
};
use dwow_serial::{
    AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite,
    FutAsyncReadExt, FutAsyncWriteExt,
};

use dwow_chain::CChainState;
use dwow_mempool::MempoolPtr;

// ============================================================================
// Message Type
// ============================================================================

/// Message for broadcasting blocks across the P2P network.
/// Simple one-way broadcast - sender broadcasts to all peers,
/// receivers insert blocks locally without rebroadcast.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockBroadcast {
    pub block: dwow_chain::Block,
}

/// Protocol metering configuration
const LINEAR_BROADCAST_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 20,
    sleep_step: 500,
    expiry_time: NanoTimestamp::from_secs(5),
};

// ============================================================================
// P2P Message Registration
// ============================================================================

impl_p2p_message!(
    BlockBroadcast,
    "linearlblock",
    0,
    1,
    LINEAR_BROADCAST_METERING_CONFIGURATION,
    &[dwow_core::net::barb_trait::BarbId::Commit, dwow_core::net::barb_trait::BarbId::Verify]
);

// ============================================================================
// Async Serialization
// ============================================================================

#[async_trait]
impl AsyncEncodable for BlockBroadcast {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        // Use serde_json directly — calling serialize_async(self) would
        // dispatch back to this same encode_async method, causing infinite
        // recursion and stack overflow on the first broadcast (block 2).
        let bytes = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut len = 0;
        len += FutAsyncWriteExt::write(s, &bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for BlockBroadcast {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let mut buf = Vec::new();
        let mut taken = d.take(MAX_BLOCK_SIZE as u64);
        FutAsyncReadExt::read_to_end(&mut taken, &mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

// ============================================================================
// Handler
// ============================================================================

/// Atomic pointer to the broadcast handler
pub type LinearBroadcastHandlerPtr = Arc<LinearBroadcastHandler>;

/// Bespoke linear blockchain block broadcast handler.
///
/// This handler receives blocks from peers via P2P broadcast and applies
/// them to the local blockchain with full validation (PoW, merkle roots,
/// contract execution) through the dwowd LinearBlockchain wrapper.
pub struct LinearBroadcastHandler {
    /// Handler for BlockBroadcast messages
    handler: ProtocolGenericHandlerPtr<BlockBroadcast, BlockBroadcast>,
    /// dwowd LinearBlockchain with full WASM validation (not the base lib type)
    blockchain: Arc<CChainState>,
    /// Mempool for cleanup of confirmed transactions after block application
    mempool: Option<MempoolPtr>,
    /// P2P instance for block rebroadcast (relay forward)
    p2p: P2pPtr,
}

impl LinearBroadcastHandler {
    /// Initialize the broadcast handler with the full-validation blockchain.
    pub async fn init(
        p2p: &P2pPtr,
        blockchain: Arc<CChainState>,
        mempool: Option<MempoolPtr>,
    ) -> LinearBroadcastHandlerPtr {
        info!(
            target: "dwowd::proto::linear_broadcast::init",
            "Initializing linear broadcast handler"
        );

        let handler = ProtocolGenericHandler::new(p2p, "LinearBroadcast", SESSION_DEFAULT).await;
        let p2p_clone = p2p.clone();
        Arc::new(Self { handler, blockchain, mempool, p2p: p2p_clone })
    }

    /// Start the handler - spawns receive loop
    pub async fn start(&self, executor: &ExecutorPtr) -> Result<()> {
        info!(
            target: "dwowd::proto::linear_broadcast::start",
            "Starting linear broadcast handler"
        );

        let blockchain = self.blockchain.clone();
        let mempool = self.mempool.clone();
        let p2p = self.p2p.clone();
        self.handler.task.clone().start(
            handle_receive_block(self.handler.clone(), blockchain, mempool, p2p),
            |res| async move {
                match res {
                    Ok(()) | Err(dwow_core::Error::DetachedTaskStopped) => {}
                    Err(e) => {
                        tracing::error!(
                            target: "dwowd::proto::linear_broadcast",
                            "Failed starting LinearBroadcast handler: {e}"
                        )
                    }
                }
            },
            dwow_core::Error::DetachedTaskStopped,
            executor.clone(),
        );

        Ok(())
    }

    /// Stop the handler
    #[allow(dead_code)]
    pub async fn stop(&self) {
        info!(
            target: "dwowd::proto::linear_broadcast::stop",
            "Stopping linear broadcast handler"
        );
        self.handler.task.stop().await;
    }
}

// ============================================================================
// Broadcast Function
// ============================================================================

/// Fan-out block relay — structured gossip replacing flood broadcast.
///
/// Replaces O(N²) flood broadcast with fan-out = ⌈log₂(N)⌉ randomly
/// selected peers. This is the ρ-calculus `GossipStructured` process
/// from type-system.md §10.2. Propagation: O(log N) rounds, O(k·N)
/// total messages — optimal for epidemic dissemination.
///
/// Falls back to flood broadcast when ≤ 2 peers are connected.
pub async fn broadcast_block(p2p: &P2pPtr, block: dwow_chain::Block) {
    let msg = BlockBroadcast { block };
    let height = msg.block.header.height;

    let peers = p2p.hosts().peers();
    let n = peers.len();

    if n <= 2 {
        // Fallback: with ≤ 2 peers, fan-out = all peers = flood
        tracing::debug!(
            target: "dwowd::proto::linear_broadcast",
            "Broadcasting block at height {} to {} peers (flood — too few peers for fan-out)",
            height, n,
        );
        p2p.broadcast(&msg).await;
        return;
    }

    // Fan-out: k = ⌈log₂(N)⌉, minimum 2
    let k = ((n as f64).log2().ceil() as usize).max(2);
    // Scope the rng so it drops before the .await below — ThreadRng is !Send
    // and must not cross an async yield point (regression from 5644ae385c).
    let selected: Vec<_> = {
        let mut rng = rand::thread_rng();
        peers.choose_multiple(&mut rng, k).cloned().collect()
    };

    tracing::debug!(
        target: "dwowd::proto::linear_broadcast",
        "Fan-out block at height {}: {}/{} peers selected (k=⌈log₂({})⌉={})",
        height, selected.len(), n, n, k,
    );

    for peer in &selected {
        if let Err(e) = peer.send(&msg).await {
            tracing::warn!(
                target: "dwowd::proto::linear_broadcast",
                "Fan-out send to {} failed: {}",
                peer.address(), e,
            );
        }
    }
}

// ============================================================================
// Receive Loop
// ============================================================================

/// Max block size in bytes for P2P reception. Pinned to the single shared
/// source of truth (L1 barrier #7) so the wire decode cap and the miner's
/// template byte cap can never drift apart.
const MAX_BLOCK_SIZE: usize = dwow_chain::execution::MAX_BLOCK_SIZE;

/// Handle incoming block messages from peers
async fn handle_receive_block(
    handler: ProtocolGenericHandlerPtr<BlockBroadcast, BlockBroadcast>,
    blockchain: Arc<CChainState>,
    mempool: Option<MempoolPtr>,
    p2p: P2pPtr,
) -> Result<()> {
    tracing::info!(target: "dwowd::proto::linear_broadcast", "TRACE: handle_receive_block loop started");

    loop {
        let (channel, msg) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(
                    target: "dwowd::proto::linear_broadcast::handle_receive_block",
                    "recv fail: {e}"
                );
                continue
            }
        };

        tracing::info!(
            target: "dwowd::proto::linear_broadcast",
            "Received block at height {} from P2P",
            msg.block.header.height
        );

        // ── Early height-gap rejection (C4 fix) ──────────────────────
        // Check BEFORE RandomX VM creation or proof-of-token-balance.
        // Far-future blocks are silently skipped — the sync protocol
        // (GetBlocks/Blocks) will pull missing blocks in order.
        // This prevents CPU waste and HeightDiscontinuity noise from
        // peers broadcasting blocks beyond our current tip.
        let current_height = blockchain.get_height();
        if msg.block.header.height > current_height.succ() {
            tracing::debug!(
                target: "dwowd::proto::linear_broadcast",
                "Skipping far-future block at height {} (local height={})",
                msg.block.header.height, current_height
            );
            continue;
        }

        // Verify proof-of-token-balance: no hidden darkw minting beyond the coinbase.
        // C1 fix: log and skip bad block instead of killing the broadcast handler.
        if let Err(e) = dwow_chain::proof_of_token_balance::verify_proof_of_token_balance(&msg.block) {
            tracing::warn!(
                target: "dwowd::proto::linear_broadcast",
                "Block at height {} failed proof-of-token-balance: {}",
                msg.block.header.height, e
            );
            continue;
        }

        // Accept block — single unified path (block_acceptor::accept_block).
        // Use pooled RandomXCache — the 256 MB allocation is reused across
        // operations, only the 2 MB scratchpad is allocated fresh.
        let randomx_key = msg.block.header.randomx_key;
        let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = blockchain.get_cache(randomx_key);
        let vm = std::sync::Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .expect("Failed to create RandomX VM for P2P block execution"),
        );

        let height = blockchain.get_height();
        let target = msg.block.header.target;

        match crate::block_acceptor::accept_block(
            &blockchain, &msg.block, &[], &vm, height, target, None,
        ) {
            Ok(()) => {
                drop(vm);
                tracing::info!(
                    target: "dwowd::proto::linear_broadcast",
                    "Block at height {} applied from P2P",
                    msg.block.header.height
                );

                // Clean confirmed transactions from the mempool (batch)
                if let Some(ref mempool) = mempool {
                    let tx_hashes: Vec<blake3::Hash> = msg.block.transactions.iter()
                        .map(|tx| tx.hash()).collect();
                    mempool.mark_mined(&tx_hashes).await;
                }

                // H9 trigger: try chain reorganization from competing blocks.
                // If a peer has a longer chain, reorganize to it.
                // HAZID H-C1: reorganize_to() is gated behind reorg-enabled feature.
                // When disabled, competing blocks are stored for uncle rewards only.
                #[cfg(feature = "reorg-enabled")]
                match blockchain.try_reorg_from_competing() {
                    Ok(count) if count > 0 => {
                        tracing::info!(
                            target: "dwowd::proto::linear_broadcast",
                            "Reorganized {} blocks after receiving block at height {}",
                            count, msg.block.header.height
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "dwowd::proto::linear_broadcast",
                            "Reorg attempt failed: {e}"
                        );
                    }
                    _ => {} // No reorg needed
                }
                #[cfg(not(feature = "reorg-enabled"))]
                {
                    // HAZID H-C1: reorg is disabled — competing blocks are stored
                    // for uncle rewards only. No chain reorganization occurs.
                    let _ = blockchain; // suppress unused warning
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "dwowd::proto::linear_broadcast",
                    "Failed to apply block at height {} from P2P: {e}",
                    msg.block.header.height
                );
            }
        }

        // Relay block to all peers — relay nodes amplify network propagation.
        // Height-gap rejection in handle_receive_block (C4 fix) means peers
        // ahead of this block silently skip it. The sender is included in
        // the broadcast but will also skip (already has the block).
        p2p.broadcast(&msg).await;
        handler.send_action(channel, ProtocolGenericAction::Skip).await;
    }
}

// ============================================================================
// Extracted absorb_block (for use by dag_absorber)
// ============================================================================

/// Apply a received block to the local chain — the shared application logic
/// used by both flood-broadcast reception and DAG-substrate absorption.
/// Returns `Ok(true)` if the block was accepted, `Ok(false)` if skipped
/// (height-gap, proof failure, duplicate/reorg).
///
/// No relay occurs from this function — the DAG relay is done by the
/// EventGraph broadcast mechanism; the flood relay is done by the caller.
pub async fn absorb_block(
    blockchain: &Arc<CChainState>,
    _vm: &std::sync::Arc<randomx::RandomXVM>,
    mempool: &Option<MempoolPtr>,
    msg: &BlockBroadcast,
) {
    // Early height-gap rejection (C4 fix)
    let current_height = blockchain.get_height();
    if msg.block.header.height > current_height.succ() {
        tracing::debug!(
            target: "dwowd::proto::linear_broadcast::absorb_block",
            "Skipping far-future block at height {} (local height={})",
            msg.block.header.height, current_height,
        );
        return;
    }

    // Verify proof-of-token-balance
    if let Err(e) = dwow_chain::proof_of_token_balance::verify_proof_of_token_balance(
        &msg.block,
    ) {
        tracing::warn!(
            target: "dwowd::proto::linear_broadcast::absorb_block",
            "Block at height {} failed proof-of-token-balance: {}",
            msg.block.header.height, e,
        );
        return;
    }

    // Accept block
    let randomx_key = msg.block.header.randomx_key;
    let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    let rx_cache = blockchain.get_cache(randomx_key);
    let vm = std::sync::Arc::new(
        randomx::RandomXVM::new(flags, Some(rx_cache), None)
            .expect("Failed to create RandomX VM for DAG block execution"),
    );
    let height = blockchain.get_height();
    let target = msg.block.header.target;

    match crate::block_acceptor::accept_block(
        blockchain, &msg.block, &[], &vm, height, target, None,
    ) {
        Ok(()) => {
            drop(vm);
            tracing::info!(
                target: "dwowd::proto::linear_broadcast::absorb_block",
                "Block at height {} applied from DAG substrate",
                msg.block.header.height,
            );

            if let Some(ref mempool) = mempool {
                let tx_hashes: Vec<blake3::Hash> = msg.block.transactions.iter()
                    .map(|tx| tx.hash()).collect();
                mempool.mark_mined(&tx_hashes).await;
            }

            #[cfg(feature = "reorg-enabled")]
            match blockchain.try_reorg_from_competing() {
                Ok(count) if count > 0 => {
                    tracing::info!(
                        target: "dwowd::proto::linear_broadcast::absorb_block",
                        "Reorganized {} blocks after DAG-delivered block at height {}",
                        count, msg.block.header.height,
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "dwowd::proto::linear_broadcast::absorb_block",
                        "Reorg attempt failed: {e}",
                    );
                }
                _ => {}
            }
            #[cfg(not(feature = "reorg-enabled"))]
            { let _ = blockchain; }
        }
        Err(e) => {
            tracing::warn!(
                target: "dwowd::proto::linear_broadcast::absorb_block",
                "Failed to apply DAG-delivered block at height {}: {e}",
                msg.block.header.height,
            );
        }
    }
}

// ============================================================================
// DAG-substrate send side (§10.4)
// ============================================================================

/// Announce a block via the event-graph DAG substrate (dual-path: flood
/// + DAG). The DAG path wraps the block as a 0x42 blockchain event,
/// inserts into the event graph, and broadcasts EventPut.
///
/// This function is a no-op when the `event-graph` feature is absent.
/// Callers do not need their own `#[cfg]` gating — they call this
/// unconditionally and the function compiles to nothing outside the
/// DAG-substrate build.
#[cfg(feature = "event-graph")]
pub async fn dag_announce_block(
    event_graph: &dwow_core::event_graph::EventGraphPtr,
    block: &dwow_chain::Block,
) {
    use dwow_core::event_graph::events::Event;
    use dwow_core::net::P2pPtr;

    let msg = BlockBroadcast { block: block.clone() };
    let payload = serde_json::to_vec(&msg)
        .unwrap_or_else(|_| {
            tracing::error!("DAG announce: serde_json failed — dropping block");
            Vec::new()
        });
    if payload.is_empty() {
        return;
    }

    // [0x42][kind=0x01][serde_json payload]
    let mut content = Vec::with_capacity(1 + 1 + payload.len());
    content.push(crate::proto::dag_absorber::BLOCKCHAIN_EVENT_MARKER);
    content.push(crate::proto::dag_absorber::kind::BLOCK);
    content.extend_from_slice(&payload);

    let event = Event::new(content, event_graph);
    if let Err(e) = event_graph.dag_insert(&[event.clone()]).await {
        tracing::warn!("DAG insert failed for block at height {}: {e}", block.header.height);
        return;
    }
    // Broadcast EventPut to all peers — the transport layer handles relay.
    event_graph.p2p().broadcast(
        &dwow_core::event_graph::proto::EventPut(vec![event]),
    ).await;
}

/// No-op when the event-graph feature is absent — the caller compiles
/// unchanged.
#[cfg(not(feature = "event-graph"))]
pub async fn dag_announce_block(
    _event_graph: &(),
    _block: &dwow_chain::Block,
) {
}

/// Compile-time assertion that the DAG absorber's barb crossing is
/// in the allowed direction (§10.4). Called from dag_absorber.rs at
/// startup; unit-testable.
///
/// The barb vocabulary is unconditional (`dwow_core::barb`), so this
/// assertion is live in every dwowd build profile — not feature-gated
/// (a `#[cfg(feature = "event-graph")]` on dwowd would check dwowd's
/// own features (which dwowd doesn't define), making the check silently
/// dead in all profiles — HAZOP 3 deviation D1).
pub fn dag_absorber_barb_check() {
    struct BlockchainAbsorber;
    impl dwow_core::barb::ExhibitsBarb for BlockchainAbsorber {
        fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
            &[
                dwow_core::barb::BarbId::Verify,
                dwow_core::barb::BarbId::Commit,
            ]
        }
    }
    struct EventGraphBarbs;
    impl dwow_core::barb::ExhibitsBarb for EventGraphBarbs {
        fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
            &[
                dwow_core::barb::BarbId::DagParent,
                dwow_core::barb::BarbId::Broadcast,
                dwow_core::barb::BarbId::RateLimit,
                dwow_core::barb::BarbId::QuorumQuery,
            ]
        }
    }
    assert!(
        dwow_core::barb::bridge_safe::<EventGraphBarbs, BlockchainAbsorber>(),
        "§10.4 quarantine: event-graph → blockchain SHALL be the allowed direction",
    );
}