/* This file is part of DarkFi (https://dark.fi)
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

//! Linear blockchain block broadcast message
//!
//! This module defines the LinearBlockMessage type used for P2P block
//! broadcasting in the linear blockchain. The message wraps a Block
//! and uses async serialization via serde JSON (same pattern as linear_sync).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use darkfi::{
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
use darkfi_linear::LinearBlockchain;
use darkfi_serial::{
    deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite,
    FutAsyncReadExt, FutAsyncWriteExt,
};

/// Protocol metering configuration for linear block broadcast
const LINEAR_BLOCK_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 20,
    sleep_step: 500,
    expiry_time: NanoTimestamp::from_secs(5),
};

// ============================================================================
// Message Type
// ============================================================================

/// Message for broadcasting blocks across the P2P network.
/// This is a one-way broadcast message - the sender broadcasts to all peers,
/// and receivers insert the block locally without further rebroadcast.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinearBlockMessage {
    pub block: darkfi_linear::Block,
}

// ============================================================================
// Async Serialization using serde_json
// ============================================================================

#[async_trait]
impl AsyncEncodable for LinearBlockMessage {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        let bytes = serialize_async(self).await;
        let mut len = 0;
        len += varint_encode(bytes.len(), s).await?;
        len += s.write(&bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for LinearBlockMessage {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let len = varint_decode(d).await?;
        let mut buf = vec![0u8; len];
        FutAsyncReadExt::read_exact(d, &mut buf).await?;
        deserialize_async(&buf)
            .await
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

// ============================================================================
// P2P Message trait registration
// ============================================================================

impl_p2p_message!(
    LinearBlockMessage,
    "linearblock",
    0,
    1,
    LINEAR_BLOCK_METERING_CONFIGURATION
);

// ============================================================================
// Handler Implementation
// ============================================================================

/// Atomic pointer to the linear block handler
pub type LinearBlockHandlerPtr = Arc<LinearBlockHandler>;

/// Handler managing linear block broadcast protocol.
/// This handler receives blocks from peers via P2P broadcast and inserts
/// them into the local blockchain.
pub struct LinearBlockHandler {
    /// Handler for LinearBlockMessage
    handler: ProtocolGenericHandlerPtr<LinearBlockMessage, LinearBlockMessage>,
    /// Linear blockchain for applying received blocks.
    /// Arc<Mutex<Arc<...>>> allows shared ownership (Arc) and interior mutability (Mutex)
    blockchain: Arc<Mutex<Arc<LinearBlockchain>>>,
}

impl LinearBlockHandler {
    /// Initialize the linear block broadcast handler
    pub async fn init(p2p: &P2pPtr, blockchain: Arc<LinearBlockchain>) -> LinearBlockHandlerPtr {
        debug!(
            target: "darkfid::proto::linear_block_message::init",
            "Adding linear block protocol to the protocol registry"
        );

        let handler = ProtocolGenericHandler::new(p2p, "LinearBlock", SESSION_DEFAULT).await;
        Arc::new(Self { handler, blockchain: Arc::new(Mutex::new(blockchain)) })
    }

    /// Start the linear block background task
    pub async fn start(&self, executor: &ExecutorPtr) -> Result<()> {
        debug!(
            target: "darkfid::proto::linear_block_message::start",
            "Starting linear block protocol handler..."
        );

        let blockchain = self.blockchain.clone();
        self.handler.task.clone().start(
            handle_receive_block(self.handler.clone(), blockchain),
            |res| async move {
                match res {
                    Ok(()) | Err(darkfi::Error::DetachedTaskStopped) => {}
                    Err(e) => {
                        tracing::error!(
                            target: "darkfid::proto::linear_block_message",
                            "Failed starting LinearBlock handler: {e}"
                        )
                    }
                }
            },
            darkfi::Error::DetachedTaskStopped,
            executor.clone(),
        );

        Ok(())
    }

    /// Stop the handler
    pub async fn stop(&self) {
        tracing::info!(
            target: "darkfid::proto::linear_block_message::stop",
            "Stopping linear block protocol handler..."
        );
        self.handler.task.stop().await;
    }
}

/// Handle incoming block messages from peers
async fn handle_receive_block(
    handler: ProtocolGenericHandlerPtr<LinearBlockMessage, LinearBlockMessage>,
    blockchain: Arc<Mutex<Arc<LinearBlockchain>>>,
) -> Result<()> {
    loop {
        let (channel, msg) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::linear_block_message::handle_receive_block",
                    "recv fail: {e}"
                );
                continue
            }
        };

        tracing::info!(
            target: "darkfid::proto::linear_block_message",
            "Received block at height {} from P2P",
            msg.block.header.height
        );

        // Insert block into blockchain
        // SAFETY: We hold exclusive access through the Arc pointer, safe within this scope
        // Use a block scope to ensure bc_locked is dropped before the await point
        let insert_result = {
            let bc = Arc::clone(&blockchain);
            let mut bc_locked = bc.lock().unwrap();
            let bc_mut: &mut LinearBlockchain = unsafe {
                let arc_ref: *mut Arc<LinearBlockchain> = &mut *bc_locked;
                let inner_ref: *mut LinearBlockchain = arc_ref as *mut LinearBlockchain;
                &mut *inner_ref
            };
            bc_mut.insert_block(&msg.block)
        };
        match insert_result {
            Ok(_) => {
                tracing::info!(
                    target: "darkfid::proto::linear_block_message",
                    "Block at height {} inserted from P2P",
                    msg.block.header.height
                );
            }
            Err(e) => {
                tracing::debug!(
                    target: "darkfid::proto::linear_block_message",
                    "Failed to insert block: {e}"
                );
            }
        };

        // Skip re-broadcast - sender already broadcast to all peers
        // We use Skip to avoid duplicate transmissions
        handler.send_action(channel, ProtocolGenericAction::Skip).await;
    }
}

// ============================================================================
// Variable-length integer encoding/decoding (same as linear_sync)
// ============================================================================

async fn varint_encode<W: AsyncWrite + Unpin + Send>(
    mut value: usize,
    s: &mut W,
) -> std::io::Result<usize> {
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        len += FutAsyncWriteExt::write(s, &[byte]).await?;
        if value == 0 {
            break
        }
    }
    Ok(len)
}

async fn varint_decode<R: AsyncRead + Unpin + Send>(d: &mut R) -> std::io::Result<usize> {
    let mut result = 0;
    let mut shift = 0;
    loop {
        let mut buf = [0u8; 1];
        FutAsyncReadExt::read_exact(d, &mut buf).await?;
        let byte = buf[0];
        result |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            break
        }
        shift += 7;
    }
    Ok(result)
}