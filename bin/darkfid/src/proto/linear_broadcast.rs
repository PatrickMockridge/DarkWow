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
//! - LinearBlockchain now has interior mutability (Arc<LinearBlockchain> pattern)

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::info;

use dwow::{
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
use dwow_linear::caribina::verify_anchor;
use dwow_linear::LinearBlockchain;
use dwow_serial::{
    deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite,
    FutAsyncReadExt, FutAsyncWriteExt,
};

// ============================================================================
// Message Type
// ============================================================================

/// Message for broadcasting blocks across the P2P network.
/// Simple one-way broadcast - sender broadcasts to all peers,
/// receivers insert blocks locally without rebroadcast.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockBroadcast {
    pub block: dwow_linear::Block,
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
        let bytes = serialize_async(self).await;
        let mut len = 0;
        len += FutAsyncWriteExt::write(s, &bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for BlockBroadcast {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let mut buf = Vec::new();
        FutAsyncReadExt::read_to_end(d, &mut buf).await?;
        deserialize_async(&buf)
            .await
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
/// This handler receives blocks from peers via P2P broadcast and inserts
/// them into the local blockchain.
pub struct LinearBroadcastHandler {
    /// Handler for BlockBroadcast messages
    handler: ProtocolGenericHandlerPtr<BlockBroadcast, BlockBroadcast>,
    /// Linear blockchain - Arc allows shared access with interior mutability
    blockchain: Arc<LinearBlockchain>,
}

impl LinearBroadcastHandler {
    /// Initialize the broadcast handler
    pub async fn init(p2p: &P2pPtr, blockchain: Arc<LinearBlockchain>) -> LinearBroadcastHandlerPtr {
        info!(
            target: "darkfid::proto::linear_broadcast::init",
            "Initializing linear broadcast handler"
        );

        let handler = ProtocolGenericHandler::new(p2p, "LinearBroadcast", SESSION_DEFAULT).await;
        Arc::new(Self { handler, blockchain })
    }

    /// Start the handler - spawns receive loop
    pub async fn start(&self, executor: &ExecutorPtr) -> Result<()> {
        info!(
            target: "darkfid::proto::linear_broadcast::start",
            "Starting linear broadcast handler"
        );

        let blockchain = self.blockchain.clone();
        self.handler.task.clone().start(
            handle_receive_block(self.handler.clone(), blockchain),
            |res| async move {
                match res {
                    Ok(()) | Err(dwow::Error::DetachedTaskStopped) => {}
                    Err(e) => {
                        tracing::error!(
                            target: "darkfid::proto::linear_broadcast",
                            "Failed starting LinearBroadcast handler: {e}"
                        )
                    }
                }
            },
            dwow::Error::DetachedTaskStopped,
            executor.clone(),
        );

        Ok(())
    }

    /// Stop the handler
    #[allow(dead_code)]
    pub async fn stop(&self) {
        info!(
            target: "darkfid::proto::linear_broadcast::stop",
            "Stopping linear broadcast handler"
        );
        self.handler.task.stop().await;
    }
}

// ============================================================================
// Broadcast Function
// ============================================================================

/// Broadcast a block to all connected peers
pub async fn broadcast_block(p2p: &P2pPtr, block: dwow_linear::Block) {
    let msg = BlockBroadcast { block };
    tracing::debug!(
        target: "darkfid::proto::linear_broadcast",
        "Broadcasting block at height {} to peers",
        msg.block.header.height
    );
    p2p.broadcast(&msg).await
}

// ============================================================================
// Receive Loop
// ============================================================================

/// Handle incoming block messages from peers
async fn handle_receive_block(
    handler: ProtocolGenericHandlerPtr<BlockBroadcast, BlockBroadcast>,
    blockchain: Arc<LinearBlockchain>,
) -> Result<()> {
    loop {
        let (channel, msg) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(
                    target: "darkfid::proto::linear_broadcast::handle_receive_block",
                    "recv fail: {e}"
                );
                continue
            }
        };

        tracing::info!(
            target: "darkfid::proto::linear_broadcast",
            "Received block at height {} from P2P",
            msg.block.header.height
        );

        // Verify PoW before inserting the block
        let block_height = msg.block.header.height;
        let randomx_key = msg.block.header.randomx_key;
        let vm = blockchain.get_vm(randomx_key);
        let block_hash = msg.block.hash(&vm);

        let pow_valid = {
            let consensus = &blockchain.consensus;
            match consensus.verify_proof(&msg.block, &vm) {
                Ok(true) => true,
                Ok(false) => {
                    tracing::warn!(
                        target: "darkfid::proto::linear_broadcast",
                        "Block at height {} failed PoW verification",
                        block_height
                    );
                    false
                }
                Err(e) => {
                    tracing::warn!(
                        target: "darkfid::proto::linear_broadcast",
                        "Block at height {} PoW error: {e}",
                        block_height
                    );
                    false
                }
            }
        };

        if !pow_valid {
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue;
        }

        // Verify Caribina anchor (if present)
        let anchor = &msg.block.header.anchor_tx_id;
        if *anchor != [0u8; 32] {
            let mut hash_bytes = [0u8; 32];
            hash_bytes.copy_from_slice(block_hash.as_bytes());
            match verify_anchor(
                anchor,
                &hash_bytes,
                msg.block.header.timestamp,
                msg.block.header.height,
            ) {
                Ok(()) => {
                    tracing::info!(
                        target: "darkfid::proto::linear_broadcast",
                        "Anchor verified for block {} at height {}",
                        block_hash, block_height
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        target: "darkfid::proto::linear_broadcast",
                        "Anchor verification failed for block {} at height {}: {}",
                        block_hash, block_height, e
                    );
                    handler.send_action(channel, ProtocolGenericAction::Skip).await;
                    continue;
                }
            }
        }

        // Insert block into blockchain
        match blockchain.insert_block(&msg.block) {
            Ok(()) => {
                tracing::info!(
                    target: "darkfid::proto::linear_broadcast",
                    "Block {} at height {} inserted from P2P",
                    block_hash, block_height
                );
            }
            Err(e) => {
                tracing::debug!(
                    target: "darkfid::proto::linear_broadcast",
                    "Failed to insert block: {e}"
                );
            }
        }

        // Skip rebroadcast - sender already broadcast to all peers
        handler.send_action(channel, ProtocolGenericAction::Skip).await;
    }
}