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
use tracing::error;

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
    // Returns the current blockchain height.
    //
    // **Params:**
    // * Empty
    //
    // **Returns:**
    // * `height`: u64 block height
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_height", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"height": 42}, "id": 1}
    pub async fn blockchain_get_height(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let linear_blockchain = match &self.linear_blockchain {
            Some(lb) => lb.clone(),
            None => {
                return JsonError::new(
                    InternalError,
                    Some("darkwow-devnet mode only".to_string()),
                    id,
                )
                .into()
            }
        };

        let height = linear_blockchain.get_height();

        let result = JsonValue::from(std::collections::HashMap::from([
            ("height".to_string(), JsonValue::Number(height as f64)),
        ]));

        JsonResponse::new(result, id).into()
    }

    // RPCAPI:
    // Returns the current PoW target for darkwow-devnet.
    //
    // **Params:**
    // * Empty
    //
    // **Returns:**
    // * `target`: u32 target (higher = easier)
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_target", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"target": 65535}, "id": 1}
    pub async fn blockchain_get_target(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let linear_blockchain = match &self.linear_blockchain {
            Some(lb) => lb.clone(),
            None => {
                return JsonError::new(
                    InternalError,
                    Some("darkwow-devnet mode only".to_string()),
                    id,
                )
                .into()
            }
        };

        let target = linear_blockchain.consensus.lock().unwrap().target();

        let result = JsonValue::from(std::collections::HashMap::from([
            ("target".to_string(), JsonValue::Number(target as f64)),
        ]));

        JsonResponse::new(result, id).into()
    }

    // RPCAPI:
    // Queries the linear blockchain for a block at the given height.
    // Returns the block serialized as JSON.
    //
    // **Params:**
    // * `array[0]`: `u64` block height
    //
    // **Returns:**
    // * `String`: JSON-encoded block
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_block_linear", "params": [2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "{\"header\":{...},\"transactions\":[...]}", "id": 1}
    pub async fn blockchain_get_block_linear(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_number() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let height = *params[0].get::<f64>().unwrap() as u64;

        let linear_blockchain = match &self.linear_blockchain {
            Some(lb) => lb.clone(),
            None => {
                return JsonError::new(
                    InternalError,
                    Some("darkwow-devnet mode only".to_string()),
                    id,
                )
                .into()
            }
        };

        let block = match linear_blockchain.get_block(height) {
            Ok(b) => b,
            Err(e) => {
                return JsonError::new(
                    InternalError,
                    Some(format!("Block not found at height {}: {}", height, e)),
                    id,
                )
                .into()
            }
        };

        let json = match serde_json::to_string(&block) {
            Ok(j) => j,
            Err(e) => {
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to serialize block: {}", e)),
                    id,
                )
                .into()
            }
        };

        JsonResponse::new(JsonValue::String(json), id).into()
    }

    // RPCAPI:
    // Queries the linear blockchain for contract state.
    // Returns the state data for a given contract.
    //
    // **Params:**
    // * `array[0]`: base58-encoded contract ID string
    // * `array[1]`: State key (optional, if empty returns all state)
    //
    // **Returns:**
    // * Contract state data
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_contract_state_linear", "params": ["contract_id", "key"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blockchain_get_contract_state_linear(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() < 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let linear_blockchain = match &self.linear_blockchain {
            Some(lb) => lb.clone(),
            None => {
                error!(target: "dwowd::rpc::blockchain_get_contract_state_linear", "darkwow-devnet mode only");
                return JsonError::new(
                    InternalError,
                    Some("blockchain.get_contract_state_linear is only available in darkwow-devnet mode".to_string()),
                    id,
                )
                .into()
            }
        };

        let contract_id_str = params[0].get::<String>().unwrap();
        let contract_id_bytes = match bs58::decode(contract_id_str).with_check(None).into_vec() {
            Ok(v) => v,
            Err(_) => {
                error!(target: "dwowd::rpc::blockchain_get_contract_state_linear", "Invalid contract_id base58");
                return JsonError::new(InvalidParams, Some("Invalid contract_id".to_string()), id).into()
            }
        };

        if contract_id_bytes.len() != 33 {
            return JsonError::new(InvalidParams, Some("Invalid contract_id length".to_string()), id).into()
        }

        let contract_id: [u8; 32] = contract_id_bytes[1..33].try_into().unwrap();

        // If key provided, return just that value
        if params.len() >= 2 && params[1].is_string() {
            let _key_str = params[1].get::<String>().unwrap();
            match linear_blockchain.store.get_contract_data(&contract_id) {
                Ok(data) if !data.is_empty() => {
                    JsonResponse::new(JsonValue::String(base64::encode(&data)), id).into()
                }
                _ => crate::server_error(crate::RpcError::ContractStateNotFound, id, None)
            }
        } else {
            // Return all contract data
            match linear_blockchain.store.get_contract_data(&contract_id) {
                Ok(data) if !data.is_empty() => {
                    let result = JsonValue::from(std::collections::HashMap::from([
                        ("contract_id".to_string(), JsonValue::String(contract_id_str.clone())),
                        ("data".to_string(), JsonValue::String(base64::encode(&data))),
                        ("exists".to_string(), JsonValue::Boolean(true)),
                    ]));
                    JsonResponse::new(result, id).into()
                }
                _ => {
                    let result = JsonValue::from(std::collections::HashMap::from([
                        ("contract_id".to_string(), JsonValue::String(contract_id_str.clone())),
                        ("exists".to_string(), JsonValue::Boolean(false)),
                    ]));
                    JsonResponse::new(result, id).into()
                }
            }
        }
    }
}
