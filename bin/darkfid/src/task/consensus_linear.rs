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

//! Linear-testnet consensus initialization task
//!
//! This module provides a simplified consensus init for linear-testnet mode,
//! which bypasses the overlay/diff system used by the standard consensus.

use std::sync::Arc;

use smol::Executor;
use tracing::info;

use crate::{task::consensus::ConsensusInitTaskConfig, DarkfiNodePtr, Result};

/// Async task to initialize consensus for linear-testnet mode.
///
/// Unlike the standard consensus_init_task, this function:
/// - Skips overlay-based genesis verification
/// - Marks the node as synced immediately
/// - Does not run the full consensus protocol (linear uses simple PoW mining via RPC)
pub async fn consensus_linear_init_task(
    node: DarkfiNodePtr,
    _config: ConsensusInitTaskConfig,
    _ex: Arc<Executor<'static>>,
) -> Result<()> {
    // Mark the node as synced immediately since linear-testnet doesn't need sync
    node.validator.write().await.synced = true;

    info!(target: "darkfid::task::consensus_linear_init_task", "Linear-testnet consensus initialized (synced=true)");

    // For linear-testnet, we don't need the full consensus task since
    // mining is done via the miner.mine_linear RPC endpoint.
    // We just wait forever (the stop() will terminate us)
    std::future::pending().await
}