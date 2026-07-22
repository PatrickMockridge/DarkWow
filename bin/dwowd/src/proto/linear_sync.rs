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

use tracing::{debug, error, info};

use dwow_core::{
    net::{
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        P2pPtr,
    },
    concurrency::ExecutorPtr,
    Error, Result,
};
use dwow_chain::Block;
use dwow_chain::sync_types::{self, Blocks, GetBlock, BlockResponse, GetBlocks, GetTip, Tip};
use dwow_sdk::blockchain::BlockHeight;

/// Constant defining max blocks we send in a single response.
pub(crate) const LINEAR_SYNC_BATCH: usize = 20;

// ============================================================================
// Message Types — imported from dwow_chain::sync_types (G1: single definition)
// Codec + P2P registration — in dwow_chain::sync_types (G1: same crate as types)
// ============================================================================

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
    /// Chain state for reading blocks (single source of truth — no stale caches)
    chain_state: Arc<dwow_chain::CChainState>,
}

impl dwow_core::barb::ExhibitsBarb for LinearSyncHandler {
    fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
        // Pull-based block synchronization — verifies blocks, gates on
        // sync barrier (catch-up boundary), propagates network state.
        &[
            dwow_core::barb::BarbId::Verify,
            dwow_core::barb::BarbId::SyncBarrier,
            dwow_core::barb::BarbId::GossipForward,
        ]
    }
}

impl LinearSyncHandler {
    /// Initialize the linear sync protocol handlers
    pub async fn init(p2p: &P2pPtr, chain_state: Arc<dwow_chain::CChainState>) -> LinearSyncHandlerPtr {
        debug!(
            target: "dwowd::proto::linear_sync::init",
            "Adding linear sync protocols to the protocol registry"
        );

        let blocks_handler =
            ProtocolGenericHandler::new(p2p, "LinearSyncBlocks", SESSION_DEFAULT).await;
        let block_handler =
            ProtocolGenericHandler::new(p2p, "LinearSyncBlock", SESSION_DEFAULT).await;
        let tip_handler = ProtocolGenericHandler::new(p2p, "LinearSyncTip", SESSION_DEFAULT).await;

        Arc::new(Self { blocks_handler, block_handler, tip_handler, chain_state })
    }

    /// Start all linear sync background tasks
    pub async fn start(&self, executor: &ExecutorPtr) -> Result<()> {
        debug!(
            target: "dwowd::proto::linear_sync::start",
            "Starting linear sync protocol handlers..."
        );

        let chain_state = self.chain_state.clone();
        self.blocks_handler.task.clone().start(
            handle_get_blocks(self.blocks_handler.clone(), chain_state.clone()),
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

        let chain_state = self.chain_state.clone();
        self.block_handler.task.clone().start(
            handle_get_block(self.block_handler.clone(), chain_state.clone()),
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

        let chain_state = self.chain_state.clone();
        self.tip_handler.task.clone().start(
            handle_get_tip(self.tip_handler.clone(), chain_state.clone()),
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
    chain_state: Arc<dwow_chain::CChainState>,
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

        // Genesis is always served ALONE: it carries the 9 contract
        // deployments (multi-MB) — batching it with subsequent blocks
        // inflates the response for no benefit. The client sync loop
        // advances per accepted block, so a 1-block response is handled
        // with zero client changes; the next request starts at height 2.
        let count = if request.start_height == BlockHeight::GENESIS {
            1
        } else {
            std::cmp::min(request.count as usize, LINEAR_SYNC_BATCH)
        };
        let mut blocks = Vec::with_capacity(count);

        // G7: height advancement via succ(), not .get() + i
        let mut height = request.start_height;
        for _ in 0..count {
            match chain_state.get_block(height) {
                Ok(block) => blocks.push(block),
                Err(_) => break,
            }
            height = height.succ();
        }

        let response = Blocks { blocks };
        handler.send_action(channel, ProtocolGenericAction::Response(response)).await;
    }
}

/// Handle incoming GetBlock requests
async fn handle_get_block(
    handler: ProtocolGenericHandlerPtr<GetBlock, BlockResponse>,
    chain_state: Arc<dwow_chain::CChainState>,
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

        let block = match chain_state.get_block(request.height) {
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
    chain_state: Arc<dwow_chain::CChainState>,
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

        // CChainState.store is the single source of truth — no stale caches.
        // G7: Ord comparison, not .get() > 0
        let height = match chain_state.store.get_height() {
            Ok(h) => h,
            Err(e) => {
                error!(target: "dwowd::proto::linear_sync",
                    "Failed to read chain height from store: {e}");
                return Err(dwow_core::Error::Custom(format!("get_height failed: {e}")));
            }
        };
        let zero = BlockHeight::new(0);
        let hash = if height > zero {
            match chain_state.store.get_block(height) {
                Ok(tip_block) => {
                    format!("{}", chain_state.hash_block_with_cached_vm(&tip_block))
                }
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };

        // Include genesis hash so peers can detect incompatible chains
        // before downloading blocks (defense-in-depth, HAZID F7/F26).
        let genesis_hash = if height >= BlockHeight::GENESIS {
            match chain_state.store.get_block(BlockHeight::GENESIS) {
                Ok(genesis_block) => {
                    Some(format!("{}", chain_state.hash_block_with_cached_vm(&genesis_block)))
                }
                Err(_) => None,
            }
        } else {
            None
        };

        let response = Tip { height, hash, genesis_hash };
        handler.send_action(channel, ProtocolGenericAction::Response(response)).await;
    }
}

// ============================================================================
// Varint encoding — imported from dwow_chain::sync_types (G1: single definition)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_core::net::Message; // MAX_BYTES comes from Message trait impl in sync_types

    /// Verify MAX_BYTES values are sufficient for actual JSON-serialized payloads.
    /// These types use serde_json, so the wire size is much larger than binary
    /// field sizes. The inner varint prefix adds 1+ bytes. This test catches
    /// future encoding changes that would break the MAX_BYTES limits.
    #[test]
    fn max_bytes_sufficient_for_json_encoding() {
        // GetBlocks: {"start_height":18446744073709551615,"count":20} ≈ 67 bytes + varint
        let gb = GetBlocks { start_height: BlockHeight::new(u64::MAX), count: 20 };
        let json = serde_json::to_vec(&gb).unwrap();
        assert!(json.len() as u64 <= GetBlocks::MAX_BYTES,
            "GetBlocks MAX_BYTES={} but max-value JSON is {} bytes",
            GetBlocks::MAX_BYTES, json.len());

        // GetBlock: {"height":18446744073709551615} ≈ 36 bytes + varint
        let gb2 = GetBlock { height: BlockHeight::new(u64::MAX) };
        let json = serde_json::to_vec(&gb2).unwrap();
        assert!(json.len() as u64 <= GetBlock::MAX_BYTES,
            "GetBlock MAX_BYTES={} but max-value JSON is {} bytes",
            GetBlock::MAX_BYTES, json.len());

        // Tip: {"height":18446744073709551615,"hash":"<64 hex>"} ≈ 104 bytes + varint
        let tip = Tip { height: BlockHeight::new(u64::MAX), hash: "f".repeat(64), genesis_hash: None };
        let json = serde_json::to_vec(&tip).unwrap();
        assert!(json.len() as u64 <= Tip::MAX_BYTES,
            "Tip MAX_BYTES={} but max-value JSON is {} bytes",
            Tip::MAX_BYTES, json.len());

        // Block-carrying responses are uncapped on the wire (0 = no limit):
        // the genesis block carries the 9 contract deployments (~multi-MB
        // JSON) and MUST fit; non-genesis blocks are bounded by the
        // consensus-level MAX_BLOCK_SIZE rule in accept_block. A future
        // finite cap here MUST account for the real genesis block size.
        assert_eq!(Blocks::MAX_BYTES, 0, "Blocks wire cap must be no-limit");
        assert_eq!(BlockResponse::MAX_BYTES, 0, "BlockResponse wire cap must be no-limit");
    }
}