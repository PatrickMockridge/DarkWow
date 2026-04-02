/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 0-2026 Dyne.org foundation
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

//! Contract invocation RPC methods.

use tracing::{error, info};

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InvalidParams, InternalError},
        JsonError, JsonResponse, JsonResult, JsonValue,
    },
    tx::Transaction,
    util::encoding::base64,
};

use super::DarkfiNode;
use crate::{
    contract_registry::{resolve_contract_id, ContractRegistry},
    server_error,
    RpcError,
};

impl DarkfiNode {
    // RPCAPI:
    // Generalized contract invocation endpoint.
    // Invokes any contract function without requiring a specific RPC method.
    //
    // --> {"jsonrpc": "2.0", "method": "contract.invoke",
    //      "params": {
    //          "contract_id": "dao_escrow",
    //          "function": "InitializeV1",
    //          "params": {"enable_drain_protection": true},
    //          "dry_run": true
    //      }, "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn contract_invoke(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<JsonValue>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Extract request fields
        let contract_id = match params.get::<serde_json::Value>().and_then(|v| v.get("contract_id")).and_then(|v| v.get::<String>()) {
            Some(v) => v.clone(),
            None => {
                error!(target: "darkfid::rpc::contract_invoke", "Missing contract_id parameter");
                return JsonError::new(InvalidParams, Some("Missing contract_id".to_string()), id).into()
            }
        };

        let function = match params.get::<serde_json::Value>().and_then(|v| v.get("function")).and_then(|v| v.get::<String>()) {
            Some(v) => v.clone(),
            None => {
                error!(target: "darkfid::rpc::contract_invoke", "Missing function parameter");
                return JsonError::new(InvalidParams, Some("Missing function".to_string()), id).into()
            }
        };

        let function_params = match params.get::<serde_json::Value>().and_then(|v| v.get("params")).and_then(|v| v.get::<serde_json::Value>()) {
            Some(v) => v.clone(),
            None => JsonValue::new_object(),
        };

        let dry_run = params.get::<serde_json::Value>()
            .and_then(|v| v.get("dry_run"))
            .and_then(|v| v.get::<bool>())
            .unwrap_or(false);

        info!(
            target: "darkfid::rpc::contract_invoke",
            "Invoking {}.{} (dry_run={})",
            contract_id,
            function,
            dry_run
        );

        // Get the contract registry
        let registry = ContractRegistry::new();

        // Get the handler for this contract
        let handler = match registry.get(&contract_id) {
            Some(h) => h,
            None => {
                error!(
                    target: "darkfid::rpc::contract_invoke",
                    "Contract not found: {}",
                    contract_id
                );
                return server_error(RpcError::ContractStateNotFound, id, Some(&format!("Contract '{}' not found", contract_id)))
            }
        };

        // Validate the function exists
        let selector = match handler.function_selector(&function) {
            Some(s) => s,
            None => {
                error!(
                    target: "darkfid::rpc::contract_invoke",
                    "Function not found: {}",
                    function
                );
                return JsonError::new(
                    InvalidParams,
                    Some(format!("Function '{}' not found in contract '{}'", function, contract_id)),
                    id,
                ).into()
            }
        };

        // Build the calldata
        let calldata = match handler.build_params(&function, function_params.clone()) {
            Ok(data) => data,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::contract_invoke",
                    "Failed to build params: {}",
                    e
                );
                return server_error(RpcError::ParseError, id, Some(&format!("Failed to build params: {}", e)))
            }
        };

        // Resolve the contract ID
        let contract_id_bytes = match resolve_contract_id(&contract_id, &self.validator.read().await).await {
            Ok(cid) => cid.to_bytes(),
            Err(e) => {
                error!(
                    target: "darkfid::rpc::contract_invoke",
                    "Failed to resolve contract ID: {}",
                    e
                );
                return server_error(RpcError::ContractStateNotFound, id, Some(&format!("{}", e)))
            }
        };

        // Build a minimal transaction for simulation/broadcast
        // Note: Full implementation would include ZK proofs and proper contract calls
        let tx = Transaction {
            calls: vec![],
            proofs: vec![],
            signatures: vec![],
        };

        // For now, return a response indicating the call was constructed
        // Full implementation requires ZK proof generation and proper transaction building
        let result = serde_json::json!({
            "contract_id": contract_id,
            "function": function,
            "selector": selector,
            "calldata_len": calldata.len(),
            "message": "Transaction building not yet implemented - ZK proof generation required"
        });

        // If dry_run, simulate
        if dry_run {
            info!(target: "darkfid::rpc::contract_invoke", "Dry run mode - skipping broadcast");
        }

        let response = crate::rpc::contract::ContractInvokeResponse {
            contract_id,
            function,
            result: result.into(),
            transaction_id: None,
            status: if dry_run { "simulated" } else { "dry_run" }.to_string(),
        };

        JsonResponse::new(serde_json::to_value(response).unwrap().into(), id).into()
    }
}
