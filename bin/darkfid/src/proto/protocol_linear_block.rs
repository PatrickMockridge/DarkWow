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

//! Linear blockchain block broadcast protocol
//!
//! This module provides a simple block broadcast protocol for the linear blockchain.
//! Unlike DarkFi's proposal protocol, linear blocks are broadcast directly without
//! the consensus layer.

use std::sync::Arc;

use tracing::{debug, info};

use darkfi::{
    net::P2pPtr,
    system::ExecutorPtr,
};
use darkfi_linear::LinearBlockchain;

/// Atomic pointer to the linear block handler
pub type LinearBlockHandlerPtr = Arc<LinearBlockHandler>;

/// Handler managing linear block broadcast protocol
pub struct LinearBlockHandler {
    /// Linear blockchain for applying received blocks
    blockchain: Arc<LinearBlockchain>,
}

impl LinearBlockHandler {
    /// Initialize the linear block protocol handler
    pub async fn init(_p2p: &P2pPtr, blockchain: Arc<LinearBlockchain>) -> LinearBlockHandlerPtr {
        debug!(
            target: "darkfid::proto::protocol_linear_block::init",
            "Adding linear block protocol to the protocol registry"
        );

        Arc::new(Self { blockchain })
    }

    /// Start the linear block background task
    pub async fn start(&self, _executor: &ExecutorPtr) -> darkfi::Result<()> {
        debug!(
            target: "darkfid::proto::protocol_linear_block::start",
            "Starting linear block protocol handler (stub - not fully implemented)..."
        );

        info!(
            target: "darkfid::proto::protocol_linear_block::start",
            "Linear block broadcast is a placeholder - full implementation deferred"
        );
        Ok(())
    }

    /// Stop the handler
    pub async fn stop(&self) {
        info!(
            target: "darkfid::proto::protocol_linear_block::stop",
            "Stopping linear block protocol handler..."
        );
    }
}