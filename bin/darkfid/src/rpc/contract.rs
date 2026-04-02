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

use tinyjson::JsonValue;
use tracing::{error, info};

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InvalidParams, InternalError},
        JsonError, JsonResponse, JsonResult,
    },
    util::encoding::base64,
};

use super::DarkfiNode;
use crate::contract_registry::ContractRegistry;

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
    pub async fn contract_invoke(&self, id: u16, params: JsonValue) -> JsonResult {
        // Extract fields from params object
        let params_obj = match params.get::<std::collections::HashMap<String, JsonValue>>() {
            Some(p) => p,
            None => {
                error!(target: "darkfid::rpc::contract", "Params must be an object");
                return JsonError::new(InvalidParams, Some("Params must be an object".to_string()), id).into();
            }
        };

        let contract_id = match params_obj.get("contract_id") {
            Some(v) => match v.get::<String>() {
                Some(s) => s.clone(),
                None => {
                    error!(target: "darkfid::rpc::contract", "contract_id must be a string");
                    return JsonError::new(InvalidParams, Some("contract_id must be a string".to_string()), id).into();
                }
            },
            None => {
                error!(target: "darkfid::rpc::contract", "Missing contract_id");
                return JsonError::new(InvalidParams, Some("Missing contract_id".to_string()), id).into();
            }
        };

        let function = match params_obj.get("function") {
            Some(v) => match v.get::<String>() {
                Some(s) => s.clone(),
                None => {
                    error!(target: "darkfid::rpc::contract", "function must be a string");
                    return JsonError::new(InvalidParams, Some("function must be a string".to_string()), id).into();
                }
            },
            None => {
                error!(target: "darkfid::rpc::contract", "Missing function");
                return JsonError::new(InvalidParams, Some("Missing function".to_string()), id).into();
            }
        };

        let default_params = JsonValue::Object(*Box::default());
        let function_params = params_obj.get("params").unwrap_or(&default_params);
        let dry_run = params_obj.get("dry_run").and_then(|v| v.get::<bool>()).unwrap_or(&false);

        info!(
            target: "darkfid::rpc::contract",
            "contract_invoke: contract={}, function={}, dry_run={}",
            contract_id,
            function,
            dry_run
        );

        // Get the contract registry
        let registry = ContractRegistry::new();

        // Get handler for this contract
        let handler = match registry.get(&contract_id) {
            Some(h) => h,
            None => {
                error!(
                    target: "darkfid::rpc::contract",
                    "Contract not found: {}",
                    contract_id
                );
                return JsonError::new(
                    InvalidParams,
                    Some(format!("Contract not found: {}", contract_id)),
                    id,
                )
                .into();
            }
        };

        // Get function selector
        let selector = match handler.function_selector(&function) {
            Some(s) => s,
            None => {
                error!(
                    target: "darkfid::rpc::contract",
                    "Function not found: {}",
                    function
                );
                return JsonError::new(
                    InvalidParams,
                    Some(format!(
                        "Function '{}' not found. Available: {:?}",
                        function,
                        handler.supported_functions()
                    )),
                    id,
                )
                .into();
            }
        };

        // Convert tinyjson::JsonValue to serde_json::Value for handler
        let params_str = match function_params.stringify() {
            Ok(s) => s,
            Err(e) => {
                error!(target: "darkfid::rpc::contract", "Failed to stringify params: {}", e);
                return JsonError::new(InvalidParams, Some(format!("Invalid params JSON: {}", e)), id).into();
            }
        };
        let params_value: serde_json::Value = match serde_json::from_str(&params_str) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::contract", "Failed to parse params: {}", e);
                return JsonError::new(InvalidParams, Some(format!("Invalid params: {}", e)), id).into();
            }
        };

        // Build calldata
        let calldata = match handler.build_params(&function, params_value) {
            Ok(data) => data,
            Err(e) => {
                error!(target: "darkfid::rpc::contract", "Failed to build params: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to build params: {}", e)),
                    id,
                )
                .into();
            }
        };

        let calldata_len = calldata.len();
        let calldata_b64 = base64::encode(&calldata);

        // For dry_run, we return the calldata without broadcasting
        let status = if *dry_run { "simulated" } else { "dry_run" };
        let message = if *dry_run {
            "Dry run complete - no transaction broadcast".to_string()
        } else {
            "Transaction building not yet implemented - ZK proof generation required".to_string()
        };

        // Build response as tinyjson::JsonValue
        let result_obj = JsonValue::from(std::collections::HashMap::from([
            ("selector".to_string(), JsonValue::Number(selector as f64)),
            ("calldata_len".to_string(), JsonValue::Number(calldata_len as f64)),
            ("calldata".to_string(), JsonValue::String(calldata_b64)),
            ("message".to_string(), JsonValue::String(message)),
        ]));

        let response_obj = JsonValue::from(std::collections::HashMap::from([
            ("contract_id".to_string(), JsonValue::String(contract_id)),
            ("function".to_string(), JsonValue::String(function)),
            ("result".to_string(), result_obj),
            ("transaction_id".to_string(), JsonValue::Null),
            ("status".to_string(), JsonValue::String(status.to_string())),
        ]));

        JsonResponse::new(response_obj, id).into()
    }
}
