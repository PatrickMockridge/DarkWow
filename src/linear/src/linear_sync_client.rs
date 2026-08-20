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

//! Shared linear-sync client — the wire-level GetTip/GetBlocks pull flow.
//!
//! Used by BOTH the mining node (`bin/dwowd`) and the wallet (`bin/dww`) so the
//! two rails are provably wire-identical. Each side supplies its own block sink
//! (node → `accept_block`, wallet → `insert_synced_block`); this module owns only
//! the request/response wire flow over a `ChannelPtr`, over the shared
//! [`crate::sync_types`] messages.

use dwow_core::net::channel::ChannelPtr;
use dwow_core::Result;
use dwow_sdk::blockchain::BlockHeight;

use crate::sync_types::{Blocks, GetBlocks, GetTip, Tip};

/// Tip request timeout (seconds). Mirrors the node's LinearSyncClient.
pub const TIP_TIMEOUT: u64 = 5;

/// Block request timeout (seconds). Mirrors the node's LinearSyncClient.
pub const BLOCKS_TIMEOUT: u64 = 30;

/// Request the chain tip from a peer.
///
/// Idempotently registers the GetTip/Tip dispatchers first — the wallet has no
/// server-side handler to do this; the node's `LinearSyncHandler` already has,
/// and re-adding is a no-op.
pub async fn request_tip(channel: &ChannelPtr) -> Result<Tip> {
    channel.add_dispatch::<GetTip>().await;
    channel.add_dispatch::<Tip>().await;

    let tip_sub = channel.subscribe_msg::<Tip>().await.map_err(|e| {
        dwow_core::Error::Custom(format!(
            "subscribe Tip on {}: {e}",
            channel.address().as_str(),
        ))
    })?;
    channel.send(&GetTip).await.map_err(|e| {
        dwow_core::Error::Custom(format!(
            "send GetTip to {}: {e}",
            channel.address().as_str(),
        ))
    })?;
    let tip = tip_sub.receive_with_timeout(TIP_TIMEOUT).await.map_err(|_| {
        dwow_core::Error::Custom(format!(
            "GetTip timed out after {TIP_TIMEOUT}s for {}",
            channel.address().as_str(),
        ))
    })?;
    // receive_with_timeout returns Arc<Tip>; the caller wants an owned Tip.
    Ok((*tip).clone())
}

/// Request a batch of blocks from a peer, starting at `start_height`.
pub async fn request_blocks(
    channel: &ChannelPtr,
    start_height: BlockHeight,
    count: u64,
) -> Result<Vec<crate::Block>> {
    channel.add_dispatch::<GetBlocks>().await;
    channel.add_dispatch::<Blocks>().await;

    let blocks_sub = channel.subscribe_msg::<Blocks>().await.map_err(|e| {
        dwow_core::Error::Custom(format!(
            "subscribe Blocks on {}: {e}",
            channel.address().as_str(),
        ))
    })?;
    let request = GetBlocks { start_height, count };
    channel.send(&request).await.map_err(|e| {
        dwow_core::Error::Custom(format!(
            "send GetBlocks to {}: {e}",
            channel.address().as_str(),
        ))
    })?;
    let blocks_msg = blocks_sub.receive_with_timeout(BLOCKS_TIMEOUT).await.map_err(|_| {
        dwow_core::Error::Custom(format!(
            "GetBlocks timed out after {BLOCKS_TIMEOUT}s for {}",
            channel.address().as_str(),
        ))
    })?;
    // receive_with_timeout returns Arc<Blocks>; clone the block vector out.
    Ok(blocks_msg.blocks.clone())
}
