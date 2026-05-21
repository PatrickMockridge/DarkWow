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

use std::{
    collections::HashSet,
    sync::Arc,
};

use smol::lock::{Mutex, RwLock};
use tracing::{error, info};

use dwow::{
    rpc::{
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use dwow_sdk::crypto::keypair::Network;

use crate::{
    rpc::stratum::StratumRpcHandler,
    DwowNode, DwowNodePtr,
};

/// Block related structures
pub mod model;
use model::LinearPowRewardZk;

/// Atomic pointer to the DarkWow node miners registry state.
pub type DwowMinersRegistryStatePtr = Arc<RwLock<DwowMinersRegistryState>>;

/// DarkWow node miners registry state.
pub struct DwowMinersRegistryState {
    /// Linear PoW reward ZK data (None for linear-testnet mode)
    pub powrewardv1_zk: Option<LinearPowRewardZk>,
    /// Linear blockchain state (only set in linear-testnet mode)
    pub linear_blockchain: Option<Arc<crate::blockchain::LinearBlockchain>>,
}

impl DwowMinersRegistryState {
    /// Create a new registry state for linear-testnet mode
    pub async fn new_linear(
        linear_blockchain: Arc<crate::blockchain::LinearBlockchain>,
    ) -> Result<DwowMinersRegistryStatePtr> {
        Ok(Arc::new(RwLock::new(Self {
            powrewardv1_zk: None,
            linear_blockchain: Some(linear_blockchain),
        })))
    }
}

/// Atomic pointer to the DarkWow node miners registry.
pub type DwowMinersRegistryPtr = Arc<DwowMinersRegistry>;

/// DarkWow node miners registry.
pub struct DwowMinersRegistry {
    /// Blockchain network
    pub network: Network,
    /// Registry state
    pub state: DwowMinersRegistryStatePtr,
    /// Stratum JSON-RPC background task
    stratum_rpc_task: StoppableTaskPtr,
    /// Stratum JSON-RPC connection tracker
    pub stratum_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// HTTP JSON-RPC background task
    mm_rpc_task: StoppableTaskPtr,
    /// HTTP JSON-RPC connection tracker
    pub mm_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl DwowMinersRegistry {
    /// Initialize a DarkWow node miners registry for linear-testnet mode.
    pub async fn init_linear(
        network: Network,
        linear_blockchain: Arc<crate::blockchain::LinearBlockchain>,
    ) -> Result<DwowMinersRegistryPtr> {
        info!(
            target: "dwowd::registry::mod::DwowMinersRegistry::init_linear",
            "Initializing a new DarkWow node miners registry for linear-testnet..."
        );

        let state = DwowMinersRegistryState::new_linear(linear_blockchain).await?;

        let stratum_rpc_task = StoppableTask::new();
        let stratum_rpc_connections = Mutex::new(HashSet::new());

        let mm_rpc_task = StoppableTask::new();
        let mm_rpc_connections = Mutex::new(HashSet::new());

        info!(
            target: "dwowd::registry::mod::DwowMinersRegistry::init_linear",
            "DarkWow node miners registry for linear-testnet generated successfully!"
        );

        Ok(Arc::new(Self {
            network,
            state,
            stratum_rpc_task,
            stratum_rpc_connections,
            mm_rpc_task,
            mm_rpc_connections,
        }))
    }

    /// Start the DarkWow node miners registry for provided DarkWow node
    /// instance.
    pub fn start(
        &self,
        executor: &ExecutorPtr,
        node: &DwowNodePtr,
        stratum_rpc_settings: &Option<RpcSettings>,
        mm_rpc_settings: &Option<RpcSettings>,
    ) -> Result<()> {
        info!(
            target: "dwowd::registry::mod::DwowMinersRegistry::start",
            "Starting the DarkWow node miners registry..."
        );

        if let Some(ref stratum_rpc) = stratum_rpc_settings {
            if !stratum_rpc.is_localhost() && !stratum_rpc.is_wildcard() {
                error!(
                    target: "dwowd::registry::mod::DwowMinersRegistry::start",
                    "Stratum RPC is configured to bind to '{}', which is not a local address. \
                     Stratum is a local coordination channel. Bind to 127.0.0.1, ::1, or 0.0.0.0.",
                    stratum_rpc.listen,
                );
                return Err(Error::ParseFailed(
                    "Stratum RPC must bind to localhost or 0.0.0.0",
                ));
            }
            if stratum_rpc.is_wildcard() {
                info!(
                    target: "dwowd::registry::mod::DwowMinersRegistry::start",
                    "Stratum RPC binding to 0.0.0.0 (all-interfaces — Docker devnet mode)",
                );
            }
        }
        if let Some(ref mm_rpc) = mm_rpc_settings {
            if !mm_rpc.is_localhost() && !mm_rpc.is_wildcard() {
                error!(
                    target: "dwowd::registry::mod::DwowMinersRegistry::start",
                    "mm_rpc is configured to bind to '{}', which is not a local address. \
                     mm_rpc is a local coordination channel. Bind to 127.0.0.1, ::1, or 0.0.0.0.",
                    mm_rpc.listen,
                );
                return Err(Error::ParseFailed(
                    "mm_rpc must bind to localhost or 0.0.0.0",
                ));
            }
            if mm_rpc.is_wildcard() {
                info!(
                    target: "dwowd::registry::mod::DwowMinersRegistry::start",
                    "mm_rpc binding to 0.0.0.0 (all-interfaces — Docker devnet mode)",
                );
            }
        }

        // Start the stratum server JSON-RPC task
        if let Some(stratum_rpc) = stratum_rpc_settings {
            info!(target: "dwowd::registry::mod::DwowMinersRegistry::start", "Starting Stratum JSON-RPC server");
            let node_ = node.clone();
            self.stratum_rpc_task.clone().start(
                listen_and_serve::<StratumRpcHandler>(stratum_rpc.clone(), node.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => <DwowNode as RequestHandler<StratumRpcHandler>>::stop_connections(&node_).await,
                        Err(e) => error!(target: "dwowd::registry::mod::DwowMinersRegistry::start", "Failed starting Stratum JSON-RPC server: {e}"),
                    }
                },
                Error::RpcServerStopped,
                executor.clone(),
            );
        } else {
            self.stratum_rpc_task.clone().start(
                async { Ok(()) },
                |_| async { /* Do nothing */ },
                Error::RpcServerStopped,
                executor.clone(),
            );
        }

        // Start the merge mining JSON-RPC task (placeholder — mm_rpc deleted)
        self.mm_rpc_task.clone().start(
            async { Ok(()) },
            |_| async { /* Do nothing */ },
            Error::RpcServerStopped,
            executor.clone(),
        );

        info!(
            target: "dwowd::registry::mod::DwowMinersRegistry::start",
            "DarkWow node miners registry started successfully!"
        );

        Ok(())
    }

    /// Stop the DarkWow node miners registry.
    pub async fn stop(&self) {
        info!(target: "dwowd::registry::mod::DwowMinersRegistry::stop", "Terminating DarkWow node miners registry...");

        info!(target: "dwowd::registry::mod::DwowMinersRegistry::stop", "Stopping Stratum JSON-RPC server...");
        self.stratum_rpc_task.stop().await;

        info!(target: "dwowd::registry::mod::DwowMinersRegistry::stop", "Stopping merge mining JSON-RPC server...");
        self.mm_rpc_task.stop().await;

        info!(target: "dwowd::registry::mod::DwowMinersRegistry::stop", "DarkWow node miners registry terminated successfully!");
    }
}
