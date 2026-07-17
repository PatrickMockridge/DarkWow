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

use dwow_core::{
    net::{P2p, P2pPtr, Settings},
    concurrency::ExecutorPtr,
    Result,
};
use tracing::info;

use dwow_chain::CChainState as DwowdBlockchain;
use dwow_mempool::MempoolPtr;
use crate::DwowNodePtr;

/// Transaction broadcast protocol
mod protocol_tx;
pub use protocol_tx::{ProtocolTxHandler, ProtocolTxHandlerPtr};

/// Linear blockchain sync protocol
pub(crate) mod linear_sync;
pub use linear_sync::{LinearSyncHandler, LinearSyncHandlerPtr};

/// Linear blockchain block broadcast protocol
pub mod linear_broadcast;
pub use linear_broadcast::{LinearBroadcastHandler, LinearBroadcastHandlerPtr};

/// DAG-substrate block absorber (type-system.md §10.4-§10.5).
/// Subscribes to the event-graph event_pub, validates 0x42 blockchain
/// events, and routes them into the blockchain accept path.
#[cfg(feature = "event-graph")]
mod dag_absorber;

/// Atomic pointer to the Dwowd P2P protocols handler.
pub type DwowP2pHandlerPtr = Arc<DwowP2pHandler>;

/// Dwowd P2P protocols handler.
pub struct DwowP2pHandler {
    /// P2P network pointer
    pub p2p: P2pPtr,
    /// `ProtocolTx` messages handler
    txs: ProtocolTxHandlerPtr,
    /// `LinearSync` messages handler (for darkwow-devnet mode)
    linear_sync: Option<LinearSyncHandlerPtr>,
    /// `LinearBroadcast` messages handler (for darkwow-devnet mode)
    linear_broadcast: Option<LinearBroadcastHandlerPtr>,
    /// Event-graph DAG for §10.4 blockchain-event substrate dissemination.
    /// Feature-gated: present only when the `event-graph` feature is
    /// enabled (always for the dwowd binary under this wiring).
    #[cfg(feature = "event-graph")]
    pub event_graph: Option<dwow_core::event_graph::EventGraphPtr>,
}

impl DwowP2pHandler {
    /// Initialize a Dwowd P2P protocols handler.
    ///
    /// A new P2P instance is generated using provided settings and all
    /// corresponding protocols are registered.
    ///
    /// `chain_state` is used by the sync handler to serve block requests.
    /// `dwowd_blockchain` is the dwowd wrapper with WASM validation, used
    /// by the broadcast handler to apply received blocks.
    /// `sled_db` is the same physical sled::Db opened by the daemon — the
    /// EventGraph opens its own named tree inside it (tree-level
    /// quarantine per §10.4).
    pub async fn init(
        settings: &Settings,
        executor: &ExecutorPtr,
        chain_state: Option<Arc<dwow_chain::CChainState>>,
        dwowd_blockchain: Option<Arc<DwowdBlockchain>>,
        mempool: Option<MempoolPtr>,
        sled_db: Option<sled::Db>,
    ) -> Result<DwowP2pHandlerPtr> {
        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::init",
            "Initializing a new Dwowd P2P handler..."
        );

        // Generate a new P2P instance
        let p2p = P2p::new(settings.clone(), executor.clone()).await?;

        // Generate a new `ProtocolTx` messages handler
        let txs = ProtocolTxHandler::init(&p2p, mempool.clone()).await;

        // Generate linear handlers if linear blockchain is enabled
        let linear_sync = if let Some(ref cs) = chain_state {
            Some(LinearSyncHandler::init(&p2p, cs.clone()).await)
        } else {
            None
        };

        let linear_broadcast = if let Some(ref blockchain) = dwowd_blockchain {
            Some(LinearBroadcastHandler::init(&p2p, blockchain.clone(), mempool).await)
        } else {
            None
        };

        // ── Event-graph DAG for §10.4 blockchain-event substrate ──
        // Port-means-copy from darkirc main.rs:339-384 (EventGraph::new +
        // ProtocolEventGraph registration). The event graph opens its own
        // named sled tree ("dwowd_dag") inside the same physical sled::Db —
        // tree-level quarantine, not binary-level.
        #[cfg(feature = "event-graph")]
        let event_graph = if let Some(sled_db) = sled_db {
            info!(
                target: "dwowd::proto::mod::DwowP2pHandler::init",
                "Instantiating EventGraph DAG for blockchain-event substrate..."
            );
            let eg = dwow_core::event_graph::EventGraph::new(
                p2p.clone(),
                sled_db,
                String::new(), // no replay datastore
                false,         // replay_mode off
                "dwowd_dag",
                1, // days_rotation
                executor.clone(),
            )
            .await
            .map_err(|e| dwow_core::Error::Custom(format!("EventGraph::new: {e}")))?;

            info!(
                target: "dwowd::proto::mod::DwowP2pHandler::init",
                "Registering ProtocolEventGraph on SESSION_DEFAULT..."
            );
            let eg_ = eg.clone();
            p2p.protocol_registry()
                .register(
                    dwow_core::net::session::SESSION_DEFAULT,
                    move |channel, _| {
                        let eg = eg_.clone();
                        async move {
                            dwow_core::event_graph::proto::ProtocolEventGraph::init(eg, channel)
                                .await
                                .unwrap()
                        }
                    },
                )
                .await;
            Some(eg)
        } else {
            None
        };

        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::init",
            "Dwowd P2P handler generated successfully!"
        );

        Ok(Arc::new(Self {
            p2p,
            txs,
            linear_sync,
            linear_broadcast,
            #[cfg(feature = "event-graph")]
            event_graph,
        }))
    }

    /// Start the Dwowd P2P protocols handler for provided node.
    pub async fn start(&self, executor: &ExecutorPtr, _node: &DwowNodePtr) -> Result<()> {
        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::start",
            "Starting the Dwowd P2P handler..."
        );

        // Start ProtocolTx — P2P transaction relay.
        // All full nodes receive, validate, and forward txs.
        // Miners profit from broader propagation (more fees).
        self.txs.start(executor).await?;

        // Start the `LinearSync` messages handler (darkwow-devnet mode)
        if let Some(ref linear_sync) = self.linear_sync {
            linear_sync.start(executor).await?;
        }

        // Start the `LinearBroadcast` messages handler (darkwow-devnet mode)
        if let Some(ref linear_broadcast) = self.linear_broadcast {
            linear_broadcast.start(executor).await?;
        }

        // Start the P2P instance
        self.p2p.clone().start().await?;

        #[cfg(feature = "event-graph")]
        if let Some(ref eg) = self.event_graph {
            let p2p_ = self.p2p.clone();
            let eg_ = eg.clone();
            // Copy darkirc main.rs:555-603 verbatim — sync_task loop +
            // sync_and_monitor. If dag_sync fails, the node silently drops
            // all EventPut (proto.rs:243-249); the re-sync loop here is the
            // liveness guarantee.
            let sync_task = dwow_core::concurrency::StoppableTask::new();
            sync_task.clone().start(async move {
                sync_and_monitor(p2p_, eg_, false).await.unwrap_or_else(|e| {
                    tracing::error!("DAG sync_and_monitor exited: {e}");
                });
            });
        }

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

/// Async task to endlessly try to sync DAG, returns Ok if done.
/// Copy from darkirc main.rs:555-574 — the exact retry loop.
#[cfg(feature = "event-graph")]
async fn sync_task(
    p2p: &dwow_core::net::P2pPtr,
    event_graph: &dwow_core::event_graph::EventGraphPtr,
    skip_dag_sync: bool,
) -> dwow_core::Result<()> {
    let comms_timeout = p2p.settings().read_arc().await.outbound_connect_timeout_max();
    loop {
        if p2p.is_connected() {
            if !skip_dag_sync {
                match event_graph.dag_sync().await {
                    Ok(()) => break,
                    Err(e) => {
                        tracing::error!(
                            "Failed syncing DAG ({e}), retrying in {comms_timeout}s..."
                        );
                        dwow_core::concurrency::sleep(comms_timeout).await;
                    }
                }
            } else {
                *event_graph.synced.write().await = true;
                break;
            }
        } else {
            tracing::info!("Waiting for some P2P connections...");
            dwow_core::concurrency::sleep(comms_timeout).await;
        }
    }
    Ok(())
}

/// Async task to monitor the network and force resync on disconnections.
/// Copy from darkirc main.rs:578-603 — the exact re-sync loop.
#[cfg(feature = "event-graph")]
async fn sync_and_monitor(
    p2p: dwow_core::net::P2pPtr,
    event_graph: dwow_core::event_graph::EventGraphPtr,
    skip_dag_sync: bool,
) -> dwow_core::Result<()> {
    loop {
        let net_subscription = p2p.hosts().subscribe_disconnect().await;
        let result = monitor_network(&net_subscription).await;
        net_subscription.unsubscribe().await;

        match result {
            Ok(_) => return Ok(()),
            Err(dwow_core::Error::NetworkNotConnected) => {
                tracing::info!("Network disconnection detected, resyncing DAG...");
                *event_graph.synced.write().await = false;
                sync_task(&p2p, &event_graph, skip_dag_sync).await?;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Async task to monitor network disconnections.
/// Copy from darkirc main.rs:550-552.
#[cfg(feature = "event-graph")]
async fn monitor_network(
    subscription: &dwow_core::net::Subscription<dwow_core::Error>,
) -> dwow_core::Result<()> {
    Err(subscription.receive().await)
}
