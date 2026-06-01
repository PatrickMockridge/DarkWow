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

//! Linear blockchain P2P sync protocol
//!
//! This module provides a simple linear synchronization protocol
//! for the linear blockchain. Unlike the main DarkWow sync protocol,
//! linear sync requests blocks by height N, N+1, etc. with no forks.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

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
    Error, Result,
};
use dwow_chain::{Block, LinearBlockchain};
use dwow_serial::{AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, FutAsyncReadExt, FutAsyncWriteExt};

/// Constant defining max blocks we send in a single response.
pub(crate) const LINEAR_SYNC_BATCH: usize = 20;

/// Protocol metering configuration for linear sync
const LINEAR_SYNC_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 20,
    sleep_step: 500,
    expiry_time: NanoTimestamp::from_secs(5),
};

// ============================================================================
// Message Types - using serde for serialization via AsyncEncodable/AsyncDecodable
// ============================================================================

/// Request blocks starting from a given height
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlocks {
    pub start_height: u64,
    pub count: u64,
}

/// Response containing blocks
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blocks {
    pub blocks: Vec<Block>,
}

/// Request a single block by height
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlock {
    pub height: u64,
}

/// Response containing a single block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockResponse {
    pub block: Option<Block>,
}

/// Request to get the current chain tip
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetTip;

/// Response containing chain tip info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tip {
    pub height: u64,
    pub hash: String,
}

// ============================================================================
// Async Serialization for messages using serde_json
// ============================================================================
//
// CRITICAL: NEVER call serialize_async(self) inside an AsyncEncodable::encode_async
// impl of the SAME type. The dispatch chain is:
//   serialize_async<T> → T::encode_async() → serialize_async<T> → ...
// which produces infinite recursion and stack overflow on the smol executor.
// The smol thread pool names threads "<unknown>", making this hard to spot.
//
// Instead, use serde_json::to_vec(self) / serde_json::from_slice(&buf) directly.
// All message types below derive serde::Serialize + Deserialize, which uses a
// completely separate codegen path and cannot recurse back into AsyncEncodable.
//
// This is the same fix applied to BlockBroadcast in linear_broadcast.rs.
// The async_lib.rs serialize_async function also has a thread-local recursion
// depth guard that panics at depth 16 as a defense-in-depth measure.
// ============================================================================

#[async_trait]
impl AsyncEncodable for GetBlocks {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut len = 0;
        len += varint_encode(bytes.len(), s).await?;
        len += s.write(&bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for GetBlocks {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let len = varint_decode(d).await?;
        let mut buf = vec![0u8; len];
        d.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[async_trait]
impl AsyncEncodable for Blocks {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut len = 0;
        len += varint_encode(bytes.len(), s).await?;
        len += s.write(&bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for Blocks {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let len = varint_decode(d).await?;
        let mut buf = vec![0u8; len];
        d.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[async_trait]
impl AsyncEncodable for GetBlock {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut len = 0;
        len += varint_encode(bytes.len(), s).await?;
        len += s.write(&bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for GetBlock {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let len = varint_decode(d).await?;
        let mut buf = vec![0u8; len];
        d.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[async_trait]
impl AsyncEncodable for BlockResponse {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut len = 0;
        len += varint_encode(bytes.len(), s).await?;
        len += s.write(&bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for BlockResponse {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let len = varint_decode(d).await?;
        let mut buf = vec![0u8; len];
        d.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[async_trait]
impl AsyncEncodable for GetTip {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut len = 0;
        len += varint_encode(bytes.len(), s).await?;
        len += s.write(&bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for GetTip {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let len = varint_decode(d).await?;
        let mut buf = vec![0u8; len];
        d.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[async_trait]
impl AsyncEncodable for Tip {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut len = 0;
        len += varint_encode(bytes.len(), s).await?;
        len += s.write(&bytes).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for Tip {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
        let len = varint_decode(d).await?;
        let mut buf = vec![0u8; len];
        d.read_exact(&mut buf).await?;
        serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

// ============================================================================
// P2P Message trait registration
// ============================================================================

impl_p2p_message!(GetBlocks, "lineargetblocks", 16, 1, LINEAR_SYNC_METERING_CONFIGURATION);
/// Maximum size for a Blocks response: 4 MB (20 blocks @ ~100 KB each + overhead)
const MAX_BLOCKS_BYTES: u64 = 4 * 1024 * 1024;

impl_p2p_message!(Blocks, "linearblocks", MAX_BLOCKS_BYTES, 1, LINEAR_SYNC_METERING_CONFIGURATION);
impl_p2p_message!(GetBlock, "lineargetblock", 8, 1, LINEAR_SYNC_METERING_CONFIGURATION);
impl_p2p_message!(BlockResponse, "linearblockresponse", MAX_BLOCKS_BYTES, 1, LINEAR_SYNC_METERING_CONFIGURATION);
impl_p2p_message!(GetTip, "lineargettip", 0, 1, LINEAR_SYNC_METERING_CONFIGURATION);
impl_p2p_message!(Tip, "lineartip", 32, 1, LINEAR_SYNC_METERING_CONFIGURATION);

// ============================================================================
// Handler Implementation
// ============================================================================

/// Atomic pointer to the linear sync handler
pub type LinearSyncHandlerPtr = Arc<LinearSyncHandler>;

/// Handler managing linear blockchain sync protocol
pub struct LinearSyncHandler {
    /// Handler for GetBlocks/Blocks messages
    blocks_handler: ProtocolGenericHandlerPtr<GetBlocks, Blocks>,
    /// Handler for GetBlock/BlockResponse messages
    block_handler: ProtocolGenericHandlerPtr<GetBlock, BlockResponse>,
    /// Handler for GetTip/Tip messages
    tip_handler: ProtocolGenericHandlerPtr<GetTip, Tip>,
    /// Linear blockchain for accessing data
    blockchain: Arc<LinearBlockchain>,
}

impl LinearSyncHandler {
    /// Initialize the linear sync protocol handlers
    pub async fn init(p2p: &P2pPtr, blockchain: Arc<LinearBlockchain>) -> LinearSyncHandlerPtr {
        debug!(
            target: "dwowd::proto::linear_sync::init",
            "Adding linear sync protocols to the protocol registry"
        );

        let blocks_handler =
            ProtocolGenericHandler::new(p2p, "LinearSyncBlocks", SESSION_DEFAULT).await;
        let block_handler =
            ProtocolGenericHandler::new(p2p, "LinearSyncBlock", SESSION_DEFAULT).await;
        let tip_handler = ProtocolGenericHandler::new(p2p, "LinearSyncTip", SESSION_DEFAULT).await;

        Arc::new(Self { blocks_handler, block_handler, tip_handler, blockchain })
    }

    /// Start all linear sync background tasks
    pub async fn start(&self, executor: &ExecutorPtr) -> Result<()> {
        debug!(
            target: "dwowd::proto::linear_sync::start",
            "Starting linear sync protocol handlers..."
        );

        let blockchain = self.blockchain.clone();
        self.blocks_handler.task.clone().start(
            handle_get_blocks(self.blocks_handler.clone(), blockchain.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => {}
                    Err(e) => error!(
                        target: "dwowd::proto::linear_sync::start",
                        "Failed starting LinearSyncBlocks handler: {e}"
                    ),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        let blockchain = self.blockchain.clone();
        self.block_handler.task.clone().start(
            handle_get_block(self.block_handler.clone(), blockchain.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => {}
                    Err(e) => error!(
                        target: "dwowd::proto::linear_sync::start",
                        "Failed starting LinearSyncBlock handler: {e}"
                    ),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        let blockchain = self.blockchain.clone();
        self.tip_handler.task.clone().start(
            handle_get_tip(self.tip_handler.clone(), blockchain.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => {}
                    Err(e) => error!(
                        target: "dwowd::proto::linear_sync::start",
                        "Failed starting LinearSyncTip handler: {e}"
                    ),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        info!(
            target: "dwowd::proto::linear_sync::start",
            "Linear sync protocol handlers started"
        );
        Ok(())
    }
}

/// Handle incoming GetBlocks requests
async fn handle_get_blocks(
    handler: ProtocolGenericHandlerPtr<GetBlocks, Blocks>,
    blockchain: Arc<LinearBlockchain>,
) -> Result<()> {
    loop {
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "dwowd::proto::linear_sync::handle_get_blocks",
                    "recv fail: {e}"
                );
                continue;
            }
        };

        debug!(
            target: "dwowd::proto::linear_sync",
            "Received GetBlocks request for height {}, count {} from {:?}",
            request.start_height, request.count, channel
        );

        let count = std::cmp::min(request.count as usize, LINEAR_SYNC_BATCH);
        let mut blocks = Vec::with_capacity(count);

        for i in 0..count {
            let height = request.start_height + i as u64;
            match blockchain.get_block(height) {
                Ok(block) => blocks.push(block),
                Err(_) => break,
            }
        }

        let response = Blocks { blocks };
        handler.send_action(channel, ProtocolGenericAction::Response(response)).await;
    }
}

/// Handle incoming GetBlock requests
async fn handle_get_block(
    handler: ProtocolGenericHandlerPtr<GetBlock, BlockResponse>,
    blockchain: Arc<LinearBlockchain>,
) -> Result<()> {
    loop {
        let (channel, request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "dwowd::proto::linear_sync::handle_get_block",
                    "recv fail: {e}"
                );
                continue;
            }
        };

        debug!(
            target: "dwowd::proto::linear_sync",
            "Received GetBlock request for height {} from {:?}",
            request.height, channel
        );

        let block = match blockchain.get_block(request.height) {
            Ok(b) => Some(b),
            Err(_) => None,
        };

        let response = BlockResponse { block };
        handler.send_action(channel, ProtocolGenericAction::Response(response)).await;
    }
}

/// Handle incoming GetTip requests
async fn handle_get_tip(
    handler: ProtocolGenericHandlerPtr<GetTip, Tip>,
    blockchain: Arc<LinearBlockchain>,
) -> Result<()> {
    loop {
        let (channel, _request) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "dwowd::proto::linear_sync::handle_get_tip",
                    "recv fail: {e}"
                );
                continue;
            }
        };

        debug!(
            target: "dwowd::proto::linear_sync",
            "Received GetTip request from {:?}", channel
        );

        let height = blockchain.get_height();
        let hash = if height > 0 {
            match blockchain.get_tip_hash() {
                Ok(h) => format!("{}", h),
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };

        let response = Tip { height, hash };
        handler.send_action(channel, ProtocolGenericAction::Response(response)).await;
    }
}

// ============================================================================
// Variable-length integer encoding/decoding
// ============================================================================

async fn varint_encode<W: AsyncWrite + Unpin + Send>(mut value: usize, s: &mut W) -> std::io::Result<usize> {
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        len += FutAsyncWriteExt::write(s, &[byte]).await?;
        if value == 0 {
            break;
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
            break;
        }
        shift += 7;
    }
    Ok(result)
}