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

use std::collections::HashSet;

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tracing::debug;

use dwow::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
};

use crate::DwowNode;

/// Blockchain related methods
mod blockchain;

/// Transactions related methods
mod tx;

/// Contract invocation methods
mod contract;

/// Stratum JSON-RPC related methods for native mining
pub mod stratum;

/// HTTP JSON-RPC related methods for merge mining
pub mod xmr;

/// Misc JSON-RPC methods
pub mod misc;

/// Node management JSON-RPC methods
pub mod management;

/// Miner methods for local development
pub mod miner;

/// Default JSON-RPC `RequestHandler`
pub struct DefaultRpcHandler;

#[async_trait]
#[rustfmt::skip]
impl RequestHandler<DefaultRpcHandler> for DwowNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "dwowd::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => <DwowNode as RequestHandler<DefaultRpcHandler>>::pong(self, req.id, req.params).await,
            "clock" => self.clock(req.id, req.params).await,

            // ==================
            // Blockchain methods
            // ==================
            "blockchain.get_block" => self.blockchain_get_block(req.id, req.params).await,
            "blockchain.get_tx" => self.blockchain_get_tx(req.id, req.params).await,
            "blockchain.get_difficulty" => self.blockchain_get_difficulty(req.id, req.params).await,
            "blockchain.get_difficulty_linear" => self.blockchain_get_difficulty_linear(req.id, req.params).await,
            "blockchain.get_block_linear" => self.blockchain_get_block_linear(req.id, req.params).await,
            "blockchain.last_confirmed_block" => self.blockchain_last_confirmed_block(req.id, req.params).await,
            "blockchain.block_target" => self.blockchain_block_target(req.id, req.params).await,
            "blockchain.lookup_wasm" => self.blockchain_lookup_wasm(req.id, req.params).await,
            "blockchain.lookup_zkas" => self.blockchain_lookup_zkas(req.id, req.params).await,
            "blockchain.contract_info" => self.blockchain_contract_info(req.id, req.params).await,
            "blockchain.get_contract_state" => self.blockchain_get_contract_state(req.id, req.params).await,
            "blockchain.get_contract_state_key" => self.blockchain_get_contract_state_key(req.id, req.params).await,
            "blockchain.get_contract_state_linear" => self.blockchain_get_contract_state_linear(req.id, req.params).await,
            "blockchain.subscribe_blocks" => self.blockchain_subscribe_blocks(req.id, req.params).await,
            "blockchain.subscribe_txs" =>  self.blockchain_subscribe_txs(req.id, req.params).await,
            "blockchain.subscribe_proposals" => self.blockchain_subscribe_proposals(req.id, req.params).await,

            // ===================
            // Transaction methods
            // ===================
            "tx.simulate" => self.tx_simulate(req.id, req.params).await,
            "tx.broadcast" => self.tx_broadcast(req.id, req.params).await,
            "tx.pending" => self.tx_pending(req.id, req.params).await,
            "tx.clean_pending" => self.tx_clean_pending(req.id, req.params).await,
            "tx.calculate_fee" => self.tx_calculate_fee(req.id, req.params).await,
            "tx.submit_linear" => self.tx_submit_linear(req.id, req.params).await,

            // =======================
            // Contract methods
            // =======================
            "contract.invoke" => self.contract_invoke(req.id, req.params).await,
            "contract.deploy" => self.contract_deploy(req.id, req.params).await,

            // ==============
            // Miner methods
            // ==============
            "miner.mine" => self.miner_mine(req.id, req.params).await,
            "miner.mine_linear" => self.miner_mine_linear(req.id, req.params).await,

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}
