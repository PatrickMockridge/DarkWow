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

//! Contract invocation RPC methods.

use std::str::FromStr;

use dwow_sdk::crypto::ContractId;
use tinyjson::JsonValue;
use tracing::{error, info};

use dwow::{
    rpc::jsonrpc::{
        ErrorCode::{InvalidParams, InternalError},
        JsonError, JsonResponse, JsonResult,
    },
    util::encoding::base64,
};

use super::DwowNode;
use crate::contract_registry::ContractRegistry;

impl DwowNode {
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
                error!(target: "dwowd::rpc::contract", "Params must be an object");
                return JsonError::new(InvalidParams, Some("Params must be an object".to_string()), id).into();
            }
        };

        let contract_id = match params_obj.get("contract_id") {
            Some(v) => match v.get::<String>() {
                Some(s) => s.clone(),
                None => {
                    error!(target: "dwowd::rpc::contract", "contract_id must be a string");
                    return JsonError::new(InvalidParams, Some("contract_id must be a string".to_string()), id).into();
                }
            },
            None => {
                error!(target: "dwowd::rpc::contract", "Missing contract_id");
                return JsonError::new(InvalidParams, Some("Missing contract_id".to_string()), id).into();
            }
        };

        let function = match params_obj.get("function") {
            Some(v) => match v.get::<String>() {
                Some(s) => s.clone(),
                None => {
                    error!(target: "dwowd::rpc::contract", "function must be a string");
                    return JsonError::new(InvalidParams, Some("function must be a string".to_string()), id).into();
                }
            },
            None => {
                error!(target: "dwowd::rpc::contract", "Missing function");
                return JsonError::new(InvalidParams, Some("Missing function".to_string()), id).into();
            }
        };

        let default_params = JsonValue::Object(*Box::default());
        let function_params = params_obj.get("params").unwrap_or(&default_params);
        let dry_run = params_obj.get("dry_run").and_then(|v| v.get::<bool>()).unwrap_or(&false);

        info!(
            target: "dwowd::rpc::contract",
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
                    target: "dwowd::rpc::contract",
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
                    target: "dwowd::rpc::contract",
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
                error!(target: "dwowd::rpc::contract", "Failed to stringify params: {}", e);
                return JsonError::new(InvalidParams, Some(format!("Invalid params JSON: {}", e)), id).into();
            }
        };
        let params_value: serde_json::Value = match serde_json::from_str(&params_str) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd::rpc::contract", "Failed to parse params: {}", e);
                return JsonError::new(InvalidParams, Some(format!("Invalid params: {}", e)), id).into();
            }
        };

        // Build calldata
        let calldata = match handler.build_params(&function, params_value) {
            Ok(data) => data,
            Err(e) => {
                error!(target: "dwowd::rpc::contract", "Failed to build params: {}", e);
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

    // RPCAPI:
    // Deploy a WASM contract to the linear blockchain.
    // This endpoint is only available in darkwow-devnet mode.
    //
    // --> {"jsonrpc": "2.0", "method": "contract.deploy",
    //      "params": {
    //          "wasm": "base64_encoded_wasm_bytes",
    //          "contract_id": "base58_contract_id_string"  // required
    //      }, "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"contract_id": "...", "status": "deployed"}, "id": 1}
    pub async fn contract_deploy(&self, id: u16, params: JsonValue) -> JsonResult {
        let params_obj = match params.get::<std::collections::HashMap<String, JsonValue>>() {
            Some(p) => p,
            None => {
                error!(target: "dwowd::rpc::contract", "Params must be an object");
                return JsonError::new(InvalidParams, Some("Params must be an object".to_string()), id).into();
            }
        };

        // Get WASM bytes (base64 encoded)
        let wasm_b64 = match params_obj.get("wasm") {
            Some(v) => match v.get::<String>() {
                Some(s) => s.clone(),
                None => {
                    error!(target: "dwowd::rpc::contract", "wasm must be a base64 string");
                    return JsonError::new(InvalidParams, Some("wasm must be a string".to_string()), id).into();
                }
            },
            None => {
                error!(target: "dwowd::rpc::contract", "Missing wasm");
                return JsonError::new(InvalidParams, Some("Missing wasm".to_string()), id).into();
            }
        };

        // Get contract_id (base58 encoded)
        let contract_id_str = match params_obj.get("contract_id") {
            Some(v) => match v.get::<String>() {
                Some(s) => s.clone(),
                None => {
                    error!(target: "dwowd::rpc::contract", "contract_id must be a string");
                    return JsonError::new(InvalidParams, Some("contract_id must be a string".to_string()), id).into();
                }
            },
            None => {
                error!(target: "dwowd::rpc::contract", "Missing contract_id");
                return JsonError::new(InvalidParams, Some("Missing contract_id".to_string()), id).into();
            }
        };

        // Decode base64 WASM
        let wasm_bytes = match base64::decode(&wasm_b64) {
            Some(b) => b,
            None => {
                error!(target: "dwowd::rpc::contract", "Failed to decode wasm");
                return JsonError::new(InvalidParams, Some("Invalid base64 encoding".to_string()), id).into();
            }
        };

        // Parse contract_id from base58
        let contract_id = match ContractId::from_str(&contract_id_str) {
            Ok(cid) => cid,
            Err(e) => {
                error!(target: "dwowd::rpc::contract", "Invalid contract_id: {}", e);
                return JsonError::new(InvalidParams, Some(format!("Invalid contract_id: {}", e)), id).into();
            }
        };

        // Check we're in darkwow-devnet mode
        let linear_blockchain = match &self.linear_blockchain {
            Some(lb) => lb.clone(),
            None => {
                error!(target: "dwowd::rpc::contract", "contract.deploy is only available in darkwow-devnet mode");
                return JsonError::new(
                    InternalError,
                    Some("contract.deploy is only available in darkwow-devnet mode".to_string()),
                    id,
                )
                .into();
            }
        };

        info!(
            target: "dwowd::rpc::contract",
            "Deploying contract {} with {} bytes",
            contract_id,
            wasm_bytes.len()
        );

        // Deploy to linear blockchain
        match linear_blockchain.deploy_contract(&wasm_bytes, contract_id, &[]) {
            Ok(()) => {
                info!(target: "dwowd::rpc::contract", "Contract deployed successfully");
                let result = JsonValue::from(std::collections::HashMap::from([
                    ("contract_id".to_string(), JsonValue::String(contract_id_str)),
                    ("wasm_size".to_string(), JsonValue::Number(wasm_bytes.len() as f64)),
                    ("status".to_string(), JsonValue::String("deployed".to_string())),
                ]));
                JsonResponse::new(result, id).into()
            }
            Err(e) => {
                error!(target: "dwowd::rpc::contract", "Failed to deploy contract: {}", e);
                JsonError::new(InternalError, Some(format!("Failed to deploy: {}", e)), id).into()
            }
        }
    }
}
