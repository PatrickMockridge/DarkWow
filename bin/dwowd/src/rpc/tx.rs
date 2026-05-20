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

// QUARANTINED: DAG-era code. Not used in linear-testnet mode. Pending deletion.
//
use dwow_serial::deserialize_async;
use tinyjson::JsonValue;
use tracing::{error, info, warn};

use dwow::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams},
        JsonError, JsonResponse, JsonResult,
    },
    tx::Transaction,
    util::encoding::base64,
};

use super::DwowNode;
use crate::{server_error, RpcError};

impl DwowNode {
    // RPCAPI:
    // Simulate a network state transition with the given transaction.
    // Returns `true` if the transaction is valid, otherwise, a corresponding
    // error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.simulate", "params": ["base64encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_simulate(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let mut validator = self.validator.as_ref().unwrap().write().await;
        if !validator.synced {
            error!(target: "dwowd::rpc::tx_simulate", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                error!(target: "dwowd::rpc::tx_simulate", "Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize_async(&tx_bytes).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd::rpc::tx_simulate", "Failed deserializing bytes into Transaction: {e}");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // Simulate state transition
        if let Err(e) = validator.append_tx(&tx, false).await {
            error!(target: "dwowd::rpc::tx_simulate", "Failed to validate state transition: {e}");
            return server_error(RpcError::TxSimulationFail, id, None)
        };

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Append a given transaction to the mempool and broadcast it to
    // the P2P network. The function will first simulate the state
    // transition in order to see if the transaction is actually valid,
    // and in turn it will return an error if this is the case.
    // Otherwise, a transaction ID will be returned.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.broadcast", "params": ["base64encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID...", "id": 1}
    pub async fn tx_broadcast(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let mut validator = self.validator.as_ref().unwrap().write().await;
        if !validator.synced {
            error!(target: "dwowd::rpc::tx_broadcast", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                error!(target: "dwowd::rpc::tx_broadcast", "Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize_async(&tx_bytes).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd::rpc::tx_broadcast", "Failed deserializing bytes into Transaction: {e}");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // We'll perform the state transition check here.
        if let Err(e) = validator.append_tx(&tx, true).await {
            error!(target: "dwowd::rpc::tx_broadcast", "Failed to append transaction to mempool: {e}");
            return server_error(RpcError::TxSimulationFail, id, None)
        };

        self.p2p_handler.p2p.broadcast(&tx).await;
        if !self.p2p_handler.p2p.is_connected() {
            warn!(target: "dwowd::rpc::tx_broadcast", "No connected channels to broadcast tx");
        }

        let tx_hash = tx.hash().to_string();
        JsonResponse::new(JsonValue::String(tx_hash), id).into()
    }

    // RPCAPI:
    // Queries the node pending transactions store to retrieve all transactions.
    // Returns a vector of hex-encoded transaction hashes.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.pending", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["TxHash" , "..."], "id": 1}
    pub async fn tx_pending(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let validator = self.validator.as_ref().unwrap().read().await;
        if !validator.synced {
            error!(target: "dwowd::rpc::tx_pending", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        let pending_txs = match validator.blockchain.get_pending_txs() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd::rpc::tx_pending", "Failed fetching pending txs: {e}");
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let pending_txs: Vec<JsonValue> =
            pending_txs.iter().map(|x| JsonValue::String(x.hash().to_string())).collect();

        JsonResponse::new(JsonValue::Array(pending_txs), id).into()
    }

    // RPCAPI:
    // Queries the node pending transactions store to reset all
    // transactions. Unproposed transactions are removed.
    // Returns `true` if the operation was successful, otherwise, a
    // corresponding error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.clean_pending", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_clean_pending(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let mut validator = self.validator.as_ref().unwrap().write().await;
        if !validator.synced {
            error!(target: "dwowd::rpc::tx_clean_pending", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Retrieve registry transactions
        let registry_txs = self.registry.state.read().await.proposed_transactions();

        // Purge all unproposed pending transactions from the database
        if let Err(e) = validator.consensus.purge_unproposed_pending_txs(registry_txs).await {
            error!(target: "dwowd::rpc::tx_clean_pending", "Failed removing pending txs: {e}");
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Compute provided transaction's total gas, against current best fork.
    // Returns the gas value if the transaction is valid, otherwise, a corresponding
    // error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.calculate_fee", "params": ["base64encodedTX", "include_fee"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_calculate_fee(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 2 || !params[0].is_string() || !params[1].is_bool() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let validator = self.validator.as_ref().unwrap().read().await;
        if !validator.synced {
            error!(target: "dwowd::rpc::tx_calculate_fee", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                error!(target: "dwowd::rpc::tx_calculate_fee", "Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize_async(&tx_bytes).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd::rpc::tx_calculate_fee", "Failed deserializing bytes into Transaction: {e}");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // Parse the include fee flag
        let include_fee = params[1].get::<bool>().unwrap();

        // Simulate state transition
        let result = validator.calculate_fee(&tx, *include_fee).await;
        if result.is_err() {
            error!(
                target: "dwowd::rpc::tx_calculate_fee", "Failed to validate state transition: {}",
                result.err().unwrap()
            );
            return server_error(RpcError::TxGasCalculationFail, id, None)
        };

        JsonResponse::new(JsonValue::Number(result.unwrap() as f64), id).into()
    }

    // RPCAPI:
    // Submit a transaction with contract calls to the linear-testnet mempool.
    // Returns the transaction hash on success.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.submit_linear", "params": ["base64encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txHash...", "id": 1}
    pub async fn tx_submit_linear(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Check we're in linear-testnet mode
        let mempool = match &self.mempool {
            Some(mp) => mp.clone(),
            None => {
                error!(target: "dwowd::rpc::tx_submit_linear", "tx.submit_linear is only available in linear-testnet mode");
                return JsonError::new(
                    InternalError,
                    Some("tx.submit_linear is only available in linear-testnet mode".to_string()),
                    id,
                )
                .into()
            }
        };

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                error!(target: "dwowd::rpc::tx_submit_linear", "Failed decoding base64 transaction");
                return JsonError::new(InvalidParams, Some("Invalid base64 encoding".to_string()), id).into()
            }
        };

        let tx: dwow_linear::Transaction = match serde_json::from_slice(&tx_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd::rpc::tx_submit_linear", "Failed deserializing bytes into Transaction: {}", e);
                return JsonError::new(InvalidParams, Some(format!("Invalid transaction format: {}", e)), id).into()
            }
        };

        let tx_hash = format!("{}", tx.hash());

        // Add to mempool
        if let Err(e) = mempool.add(tx).await {
            error!(target: "dwowd::rpc::tx_submit_linear", "Failed to add transaction to mempool: {}", e);
            return JsonError::new(InternalError, Some(format!("Failed to add to mempool: {}", e)), id).into()
        };

        info!(target: "dwowd::rpc::tx_submit_linear", "Transaction {} added to mempool", tx_hash);
        JsonResponse::new(JsonValue::String(tx_hash), id).into()
    }
}
