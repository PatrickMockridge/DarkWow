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

/// Linear blockchain sync protocol (serve side) — shared dwow_chain::sync_handler.
pub use dwow_chain::sync_handler::{LinearSyncHandler, LinearSyncHandlerPtr};

/// Linear blockchain block broadcast protocol
pub mod linear_broadcast;
pub use linear_broadcast::{LinearBroadcastHandler, LinearBroadcastHandlerPtr};

/// Linear blockchain sync client protocol (requester side — net-node tier)
///
/// `LinearSyncHandler` serves GetTip/GetBlocks requests FROM peers.
/// This module provides the CLIENT side: it requests tips and blocks
/// FROM peers and returns typed results. Consensus code never touches
/// raw P2P primitives.
pub mod linear_sync_client;

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
        _sled_db: Option<sled::Db>,
    ) -> Result<DwowP2pHandlerPtr> {
        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::init",
            "Initializing a new Dwowd P2P handler..."
        );

        // Generate a new P2P instance
        let p2p = P2p::new(settings.clone(), executor.clone()).await?;

        // Tx sink for the unified sync server: route broadcast txs into the
        // mempool via the SAME admission path as the P2P ProtocolTx handler
        // (harmonization — one rail for sync and tx).
        let tx_sink: Option<dwow_chain::sync_connection::TxSink> = mempool.as_ref().map(|mp| {
            let mp = mp.clone();
            let relay = p2p.clone();
            let sink: dwow_chain::sync_connection::TxSink = Arc::new(move |core_tx| {
                let mp = mp.clone();
                let relay = relay.clone();
                smol::spawn(async move {
                    if let Err(e) = protocol_tx::admit_tx_to_mempool(core_tx, &mp, &relay).await {
                        tracing::warn!(target: "dwowd::proto::mod",
                            "sync-server tx admission failed: {e}");
                    }
                }).detach();
            });
            sink
        });

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

        // Unified sync connection server — serves GetTip/GetBlocks on a dedicated
        // sync port (inbound + SYNC_PORT_OFFSET), separate from the tx/broadcast P2P
        // port, so the wallet and node both use the same sync path (sync-hazop.md R3).
        // Spawned here (consumed) so the non-Sync PtListener never crosses threads.
        if let (Some(cs), Some(addr)) = (&chain_state, settings.inbound_addrs.first()) {
            let mut sync_addr = addr.clone();
            if let Some(port) = addr.port() {
                let _ = sync_addr.set_port(Some(port + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
            }
            let server = dwow_chain::sync_connection::SyncServer::listen(
                sync_addr.clone(),
                settings.magic_bytes.0,
                cs.clone(),
                tx_sink,
            ).await?;
            info!(target: "dwowd::proto::mod::DwowP2pHandler::init",
                "Unified sync server listening on {sync_addr}");
            smol::spawn(async move {
                if let Err(e) = server.run().await {
                    tracing::error!(target: "dwowd::proto::mod", "unified sync server stopped: {e}");
                }
            }).detach();
        }

        info!(
            target: "dwowd::proto::mod::DwowP2pHandler::init",
            "Dwowd P2P handler generated successfully!"
        );

        Ok(Arc::new(Self {
            p2p,
            txs,
            linear_sync,
            linear_broadcast,
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

        // Stop the linear blockchain protocol handlers
        if let Some(ref sync) = self.linear_sync {
            sync.stop().await;
        }
        if let Some(ref broadcast) = self.linear_broadcast {
            broadcast.stop().await;
        }

        // Stop the ProtocolTx messages handler
        self.txs.stop().await;

        // Stop the P2P instance (last — channels depend on P2P)
        self.p2p.stop().await;

        info!(target: "dwowd::proto::mod::DwowP2pHandler::stop", "Dwowd P2P handler terminated successfully!");
    }
}
