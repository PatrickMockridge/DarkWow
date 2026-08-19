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
    impl_p2p_message, impl_boundary_codec,
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
    &[
        dwow_core::net::barb_trait::BarbId::Commit,
        dwow_core::net::barb_trait::BarbId::Verify,
        dwow_core::net::barb_trait::BarbId::Broadcast,
        dwow_core::net::barb_trait::BarbId::GossipForward,
    ]
);

// JSON-based sync Encodable/Decodable — BoundaryCodec requires these
// supertraits (§10.5). Wire format matches the async codec above.
// MAX_BYTES=4 MiB (MAX_BLOCK_SIZE), METERING_SCORE=5 (blocks are
// expensive to validate).
impl dwow_serial::Encodable for BlockBroadcast {
    fn encode<W: std::io::Write>(&self, e: &mut W) -> std::io::Result<usize> {
        let json = serde_json::to_vec(&self.block)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        e.write(&json)
    }
}
impl dwow_serial::Decodable for BlockBroadcast {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let mut buf = Vec::new();
        d.read_to_end(&mut buf)?;
        let block = serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self { block })
    }
}
impl_boundary_codec!(BlockBroadcast, 4 * 1024 * 1024, 5,
    &[
        dwow_core::net::barb_trait::BarbId::Commit,
        dwow_core::net::barb_trait::BarbId::Verify,
        dwow_core::net::barb_trait::BarbId::Broadcast,
        dwow_core::net::barb_trait::BarbId::GossipForward,
    ]
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
        // Genesis blocks carry contract deployments (multi-MB WASM as
        // serde_json) and cannot use the MAX_BLOCK_SIZE cap. Apply a
        // generous upper bound (100 MB — 6x the expected ~15 MB genesis)
        // to prevent a malicious peer from causing OOM by sending a
        // multi-GB payload claiming to be genesis.
        const MAX_GENESIS_SIZE: u64 = 100 * 1024 * 1024; // 100 MB
        let mut buf = Vec::new();
        {
            let mut capped = FutAsyncReadExt::take(d, MAX_GENESIS_SIZE);
            FutAsyncReadExt::read_to_end(&mut capped, &mut buf).await?;
        }
        if buf.len() as u64 >= MAX_GENESIS_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("genesis block exceeds MAX_GENESIS_SIZE ({} bytes)", MAX_GENESIS_SIZE),
            ));
        }
        let msg: Self = serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if msg.block.header.height != dwow_sdk::blockchain::BlockHeight::GENESIS
            && buf.len() > MAX_BLOCK_SIZE
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "non-genesis block at height {} is {} bytes — exceeds MAX_BLOCK_SIZE {}",
                    msg.block.header.height, buf.len(), MAX_BLOCK_SIZE
                ),
            ));
        }
        Ok(msg)
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

impl dwow_core::barb::ExhibitsBarb for LinearBroadcastHandler {
    fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
        // Receives, validates, applies, and relays blocks. Does NOT mine
        // — mining occurs in miner_task. Per type-system.md §10.4.
        &[
            dwow_core::barb::BarbId::Commit,
            dwow_core::barb::BarbId::Verify,
            dwow_core::barb::BarbId::Broadcast,
            dwow_core::barb::BarbId::GossipForward,
        ]
    }
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
    fan_out_block(p2p, &msg).await;
}

/// Relay a block via log-fan-out (k = ⌈log₂(N)⌉, min 2), falling back to flood
/// at ≤ 2 peers. C1: the receive-side relay MUST use the same fan-out as the
/// miner — a flood relay is O(N²) on dense networks.
async fn fan_out_block(p2p: &P2pPtr, msg: &BlockBroadcast) {
    let height = msg.block.header.height;
    let peers = p2p.hosts().peers();
    let n = peers.len();

    if n <= 2 {
        // Fallback: with ≤ 2 peers, fan-out = all peers = flood
        tracing::debug!(
            target: "dwowd::proto::linear_broadcast",
            "Relaying block at height {} to {} peers (flood — too few peers for fan-out)",
            height, n,
        );
        p2p.broadcast(msg).await;
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
        if let Err(e) = peer.send(msg).await {
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
        let rx_cache = blockchain.get_cache(randomx_key)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "RandomX cache: {}", e
            )))?;
        let vm = std::sync::Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .expect("Failed to create RandomX VM for P2P block execution"),
        );

        let height = blockchain.get_height();
        let target = msg.block.header.target;

        match crate::block_acceptor::accept_block(
            &blockchain, &msg.block, &[], &vm, height, target, None,
        ) {
            Ok(outcome) => {
                drop(vm);
                let is_canonical = matches!(outcome, dwow_chain::BlockConnectOutcome::CanonicalExtension { .. });
                tracing::info!(
                    target: "dwowd::proto::linear_broadcast",
                    "Block at height {} {:?} from P2P",
                    msg.block.header.height, outcome,
                );

                // Clean confirmed transactions from the mempool — ONLY for
                // canonical blocks. Competing/Uncle blocks do NOT advance the
                // chain; their transactions remain pending in the mempool.
                if is_canonical {
                    if let Some(ref mempool) = mempool {
                        let tx_hashes: Vec<blake3::Hash> = msg.block.transactions.iter()
                            .map(|tx| tx.hash()).collect();
                        mempool.mark_mined(&tx_hashes).await;
                    }
                }

                // Reorganization removed — linear blockchain resolves forks
                // via uncle rewards, not reorg. Competing blocks are stored
                // for uncle rewards only. No chain reorganization occurs.
                // C1/C2: relay ONLY canonical blocks, via fan-out (not flood).
                // Competing/Uncle blocks are stored but MUST NOT be amplified
                // network-wide — they did not advance the chain. A false-negative
                // (rejecting a valid block) self-corrects because other honest
                // peers relay the block independently.
                if is_canonical {
                    fan_out_block(&p2p, &msg).await;
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "dwowd::proto::linear_broadcast",
                    "Failed to apply block at height {} from P2P: {e} — NOT relaying",
                    msg.block.header.height
                );
            }
        }

        handler.send_action(channel, ProtocolGenericAction::Skip).await;
    }
}
