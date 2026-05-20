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

use std::sync::Arc;

use dwow::{
    net::{P2p, P2pPtr, Settings},
    system::ExecutorPtr,
    Result,
};
use tracing::info;

use crate::DwowNodePtr;

/// Transaction broadcast protocol
mod protocol_tx;
pub use protocol_tx::{ProtocolTxHandler, ProtocolTxHandlerPtr};

/// Linear blockchain sync protocol
mod linear_sync;
pub use linear_sync::{LinearSyncHandler, LinearSyncHandlerPtr};

/// Linear blockchain block broadcast protocol
pub mod linear_broadcast;
pub use linear_broadcast::{LinearBroadcastHandler, LinearBroadcastHandlerPtr};

/// Atomic pointer to the Dwowd P2P protocols handler.
pub type DwowP2pHandlerPtr = Arc<DwowP2pHandler>;

/// Dwowd P2P protocols handler.
pub struct DwowP2pHandler {
    /// P2P network pointer
    pub p2p: P2pPtr,
    /// `ProtocolTx` messages handler
    txs: ProtocolTxHandlerPtr,
    /// `LinearSync` messages handler (for linear-testnet mode)
    linear_sync: Option<LinearSyncHandlerPtr>,
    /// `LinearBroadcast` messages handler (for linear-testnet mode)
    linear_broadcast: Option<LinearBroadcastHandlerPtr>,
}

impl DwowP2pHandler {
    /// Initialize a Dwowd P2P protocols handler.
    ///
    /// A new P2P instance is generated using provided settings and all
    /// corresponding protocols are registered.
    pub async fn init(
        settings: &Settings,
        executor: &ExecutorPtr,
        linear_blockchain: Option<Arc<dwow_linear::LinearBlockchain>>,
    ) -> Result<DwowP2pHandlerPtr> {
        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::init",
            "Initializing a new Dwowd P2P handler..."
        );

        // Generate a new P2P instance
        let p2p = P2p::new(settings.clone(), executor.clone()).await?;

        // Generate a new `ProtocolTx` messages handler
        let txs = ProtocolTxHandler::init(&p2p).await;

        // Generate linear handlers if linear blockchain is enabled
        let linear_sync = if let Some(ref blockchain) = linear_blockchain {
            Some(LinearSyncHandler::init(&p2p, blockchain.clone()).await)
        } else {
            None
        };

        let linear_broadcast = if let Some(blockchain) = linear_blockchain {
            Some(LinearBroadcastHandler::init(&p2p, blockchain).await)
        } else {
            None
        };

        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::init",
            "Dwowd P2P handler generated successfully!"
        );

        Ok(Arc::new(Self { p2p, txs, linear_sync, linear_broadcast }))
    }

    /// Start the Dwowd P2P protocols handler for provided node.
    pub async fn start(&self, executor: &ExecutorPtr, node: &DwowNodePtr) -> Result<()> {
        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::start",
            "Starting the Dwowd P2P handler..."
        );

        // Start the `ProtocolTx` messages handler (DAG mode only)
        if let Some(ref validator) = node.validator {
            let subscriber = node.subscribers.get("txs").unwrap().clone();
            self.txs.start(executor, validator, subscriber).await?;
        }

        // Start the `LinearSync` messages handler (linear-testnet mode)
        if let Some(ref linear_sync) = self.linear_sync {
            linear_sync.start(executor).await?;
        }

        // Start the `LinearBroadcast` messages handler (linear-testnet mode)
        if let Some(ref linear_broadcast) = self.linear_broadcast {
            linear_broadcast.start(executor).await?;
        }

        // Start the P2P instance
        self.p2p.clone().start().await?;

        // Seed the P2P network to discover peers from seed nodes
        self.p2p.clone().seed().await;

        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::start",
            "Dwowd P2P handler started successfully!"
        );

        Ok(())
    }

    /// Stop the Dwowd P2P protocols handler.
    pub async fn stop(&self) {
        info!(target: "dwowd::proto::mod::DwowP2pHandler::stop", "Terminating Dwowd P2P handler...");

        // Stop the P2P instance
        self.p2p.stop().await;

        // Start the `ProtocolTx` messages handler
        self.txs.stop().await;

        info!(target: "dwowd::proto::mod::DwowP2pHandler::stop", "Dwowd P2P handler terminated successfully!");
    }
}
