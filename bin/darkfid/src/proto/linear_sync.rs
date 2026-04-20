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

//! Linear blockchain P2P sync protocol
//!
//! This module provides a simple linear synchronization protocol
//! for the linear blockchain. Unlike the main DarkFi sync protocol,
//! linear sync requests blocks by height N, N+1, etc. with no forks.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use darkfi::{
    net::{
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
use darkfi_linear::{Block, LinearStore};

/// Constant defining max blocks we send in a single response.
const LINEAR_SYNC_BATCH: usize = 20;

/// Protocol metering configuration for linear sync
const LINEAR_SYNC_METERING_CONFIGURATION: darkfi::net::metering::MeteringConfiguration =
    darkfi::net::metering::MeteringConfiguration {
        threshold: 20,
        sleep_step: 500,
        expiry_time: NanoTimestamp::from_secs(5),
    };

/// Request blocks starting from a given height
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlocks {
    /// Starting block height
    pub start_height: u64,
    /// Number of blocks to fetch
    pub count: u64,
}

/// Response containing blocks
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blocks {
    /// Blocks returned
    pub blocks: Vec<Block>,
}

/// Request a single block by height
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlock {
    /// Block height
    pub height: u64,
}

/// Response containing a single block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockResponse {
    /// Block if found
    pub block: Option<Block>,
}

/// Request to get the current chain tip
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetTip;

/// Response containing chain tip info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tip {
    /// Current chain height
    pub height: u64,
    /// Block hash at current tip
    pub hash: String,
}

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
    /// Linear store for accessing blockchain data
    store: Arc<LinearStore>,
}

impl LinearSyncHandler {
    /// Initialize the linear sync protocol handlers
    pub async fn init(p2p: &P2pPtr, store: Arc<LinearStore>) -> LinearSyncHandlerPtr {
        debug!(
            target: "darkfid::proto::linear_sync::init",
            "Adding linear sync protocols to the protocol registry"
        );

        let blocks_handler =
            ProtocolGenericHandler::new(p2p, "LinearSyncBlocks", SESSION_DEFAULT).await;
        let block_handler =
            ProtocolGenericHandler::new(p2p, "LinearSyncBlock", SESSION_DEFAULT).await;
        let tip_handler = ProtocolGenericHandler::new(p2p, "LinearSyncTip", SESSION_DEFAULT).await;

        Arc::new(Self { blocks_handler, block_handler, tip_handler, store })
    }

    /// Start all linear sync background tasks
    pub async fn start(&self, executor: &ExecutorPtr) {
        debug!(
            target: "darkfid::proto::linear_sync::start",
            "Starting linear sync protocol handlers..."
        );

        let store = self.store.clone();
        self.blocks_handler.task.clone().start(
            handle_get_blocks(self.blocks_handler.clone(), store.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => {}
                    Err(e) => error!(
                        target: "darkfid::proto::linear_sync::start",
                        "Failed starting LinearSyncBlocks handler: {e}"
                    ),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        let store = self.store.clone();
        self.block_handler.task.clone().start(
            handle_get_block(self.block_handler.clone(), store.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => {}
                    Err(e) => error!(
                        target: "darkfid::proto::linear_sync::start",
                        "Failed starting LinearSyncBlock handler: {e}"
                    ),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        let store = self.store.clone();
        self.tip_handler.task.clone().start(
            handle_get_tip(self.tip_handler.clone(), store.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => {}
                    Err(e) => error!(
                        target: "darkfid::proto::linear_sync::start",
                        "Failed starting LinearSyncTip handler: {e}"
                    ),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        info!(
            target: "darkfid::proto::linear_sync::start",
            "Linear sync protocol handlers started"
        );
    }
}

/// Handle incoming GetBlocks requests
async fn handle_get_blocks(
    handler: ProtocolGenericHandlerPtr<GetBlocks, Blocks>,
    store: Arc<LinearStore>,
) -> Result<()> {
    loop {
        let (action, who) = handler.task.clone().recv().await?;
        match action {
            ProtocolGenericAction::Request(msg) => {
                debug!(
                    target: "darkfid::proto::linear_sync",
                    "Received GetBlocks request for height {}, count {} from {}",
                    msg.start_height, msg.count, who
                );

                let count = std::cmp::min(msg.count as usize, LINEAR_SYNC_BATCH);
                let mut blocks = Vec::with_capacity(count);

                for i in 0..count {
                    let height = msg.start_height + i as u64;
                    match store.get_block(height) {
                        Ok(block) => blocks.push(block),
                        Err(_) => break,
                    }
                }

                let response = Blocks { blocks };
                handler.task.clone().send(response, who).await?;
            }
            ProtocolGenericAction::Response(_) => {
                error!(
                    target: "darkfid::proto::linear_sync",
                    "Received unexpected response in GetBlocks handler"
                );
            }
        }
    }
}

/// Handle incoming GetBlock requests
async fn handle_get_block(
    handler: ProtocolGenericHandlerPtr<GetBlock, BlockResponse>,
    store: Arc<LinearStore>,
) -> Result<()> {
    loop {
        let (action, who) = handler.task.clone().recv().await?;
        match action {
            ProtocolGenericAction::Request(msg) => {
                debug!(
                    target: "darkfid::proto::linear_sync",
                    "Received GetBlock request for height {} from {}",
                    msg.height, who
                );

                let block = match store.get_block(msg.height) {
                    Ok(b) => Some(b),
                    Err(_) => None,
                };

                let response = BlockResponse { block };
                handler.task.clone().send(response, who).await?;
            }
            ProtocolGenericAction::Response(_) => {
                error!(
                    target: "darkfid::proto::linear_sync",
                    "Received unexpected response in GetBlock handler"
                );
            }
        }
    }
}

/// Handle incoming GetTip requests
async fn handle_get_tip(
    handler: ProtocolGenericHandlerPtr<GetTip, Tip>,
    store: Arc<LinearStore>,
) -> Result<()> {
    loop {
        let (action, who) = handler.task.clone().recv().await?;
        match action {
            ProtocolGenericAction::Request(_msg) => {
                debug!(
                    target: "darkfid::proto::linear_sync",
                    "Received GetTip request from {}", who
                );

                let height = store.get_height().unwrap_or(0);
                let hash = if height > 0 {
                    match store.get_block(height) {
                        Ok(block) => format!("{:x}", block.hash()),
                        Err(_) => String::new(),
                    }
                } else {
                    String::new()
                };

                let response = Tip { height, hash };
                handler.task.clone().send(response, who).await?;
            }
            ProtocolGenericAction::Response(_) => {
                error!(
                    target: "darkfid::proto::linear_sync",
                    "Received unexpected response in GetTip handler"
                );
            }
        }
    }
}