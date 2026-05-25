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

use tracing::{debug, error};

use dwow::{
    net::{
        protocol::protocol_generic::{
            ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        P2pPtr,
    },
    system::ExecutorPtr,
    tx::Transaction,
    Error, Result,
};

/// Atomic pointer to the `ProtocolTx` handler.
pub type ProtocolTxHandlerPtr = Arc<ProtocolTxHandler>;

/// Handler managing [`Transaction`] messages, over a generic P2P protocol.
pub struct ProtocolTxHandler {
    /// The generic handler for [`Transaction`] messages.
    handler: ProtocolGenericHandlerPtr<Transaction, Transaction>,
}

impl ProtocolTxHandler {
    /// Initialize a generic prototocol handler for [`Transaction`] messages
    /// and registers it to the provided P2P network, using the default session flag.
    pub async fn init(p2p: &P2pPtr) -> ProtocolTxHandlerPtr {
        debug!(
            target: "dwowd::proto::protocol_tx::init",
            "Adding ProtocolTx to the protocol registry"
        );

        let handler = ProtocolGenericHandler::new(p2p, "ProtocolTx", SESSION_DEFAULT).await;

        Arc::new(Self { handler })
    }

    /// Start the `ProtocolTx` background task.
    /// In darkwow-devnet mode, this is a no-op (kept for forward compatibility).
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
    ) -> Result<()> {
        debug!(
            target: "dwowd::proto::protocol_tx::start",
            "ProtocolTx handler running in linear mode (no-op)..."
        );

        self.handler.task.clone().start(
            async { std::future::pending().await },
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "dwowd::proto::protocol_tx::start", "Failed starting ProtocolTx handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        debug!(
            target: "dwowd::proto::protocol_tx::start",
            "ProtocolTx handler task started!"
        );

        Ok(())
    }

    /// Stop the `ProtocolTx` background task.
    pub async fn stop(&self) {
        debug!(target: "dwowd::proto::protocol_tx::stop", "Terminating ProtocolTx handler task...");
        self.handler.task.stop().await;
        debug!(target: "dwowd::proto::protocol_tx::stop", "ProtocolTx handler task terminated!");
    }
}
