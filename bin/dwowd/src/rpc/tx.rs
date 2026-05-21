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

use tinyjson::JsonValue;
use tracing::{error, info};

use dwow::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams},
        JsonError, JsonResponse, JsonResult,
    },
    util::encoding::base64,
};

use crate::DwowNode;

impl DwowNode {
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
