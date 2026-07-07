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
    system::ExecutorPtr,
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
    LINEAR_BROADCAST_METERING_CONFIGURATION
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

/// Broadcast a block to all connected peers
pub async fn broadcast_block(p2p: &P2pPtr, block: dwow_chain::Block) {
    let msg = BlockBroadcast { block };
    tracing::debug!(
        target: "dwowd::proto::linear_broadcast",
        "Broadcasting block at height {} to peers",
        msg.block.header.height
    );
    p2p.broadcast(&msg).await
}

// ============================================================================
// Receive Loop
// ============================================================================

/// Max block size in bytes for P2P reception (4 MB).
const MAX_BLOCK_SIZE: usize = 4 * 1024 * 1024;

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
        if msg.block.header.height > current_height + 1 {
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
            &blockchain, &msg.block, &[], &vm, height, target,
        ) {
            Ok(()) => {
                drop(vm);
                tracing::info!(
                    target: "dwowd::proto::linear_broadcast",
                    "Block at height {} applied from P2P",
                    msg.block.header.height
                );

                // Clean confirmed transactions from the mempool
                if let Some(ref mempool) = mempool {
                    for tx in &msg.block.transactions {
                        mempool.remove(tx.hash().as_bytes()).await;
                    }
                }

                // H9 trigger: try chain reorganization from competing blocks.
                // If a peer has a longer chain, reorganize to it.
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