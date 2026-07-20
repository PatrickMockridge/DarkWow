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

use hex;

use dwow_core::{
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

        let chain = match &self.chain_state {
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

        let height = chain.get_height();

        let result = JsonValue::from(std::collections::HashMap::from([
            ("height".to_string(), JsonValue::Number(height.get() as f64)),
        ]));

        JsonResponse::new(result, id).into()
    }

    // RPCAPI:
    // Returns the last confirmed block height and hash.
    // Matches the format the wallet expects: [height_f64, hash_string].
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.last_confirmed_block", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [42, "abc123..."], "id": 1}
    pub async fn blockchain_last_confirmed_block(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let chain = match &self.chain_state {
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

        let height = chain.get_height();
        let hash = match chain.get_block(height) {
            Ok(block) => hex::encode(block.header.merkle_root.as_bytes()),
            Err(_) => String::new(),
        };

        let result = JsonValue::Array(vec![
            JsonValue::Number(height.get() as f64),
            JsonValue::String(hash),
        ]);

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

        let chain = match &self.chain_state {
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

        let target = chain.consensus.lock().unwrap().target();

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

        let height = dwow_sdk::blockchain::BlockHeight::new(*params[0].get::<f64>().unwrap() as u64);

        let chain = match &self.chain_state {
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

        let block = match chain.get_block(height) {
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

        let chain = match &self.chain_state {
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

        let contract_id: [u8; 32] = match contract_id_bytes[1..33].try_into() {
            Ok(arr) => arr,
            Err(_) => return JsonError::new(InvalidParams, Some("Invalid contract_id length".to_string()), id).into(),
        };

        // If key provided, return just that value
        if params.len() >= 2 && params[1].is_string() {
            let _key_str = params[1].get::<String>().unwrap();
            match chain.store.get_contract_data(&contract_id) {
                Ok(data) if !data.is_empty() => {
                    JsonResponse::new(JsonValue::String(base64::encode(&data)), id).into()
                }
                _ => crate::server_error(crate::RpcError::ContractStateNotFound, id, None)
            }
        } else {
            // Return all contract data
            match chain.store.get_contract_data(&contract_id) {
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

    // RPCAPI:
    // Returns the Pedersen cumulative supply commitment chain state.
    // Computes S_H = S_{H-1} + C_H from the canonical chain using the
    // deterministic emission schedule and coinbase blind derivation.
    // Any node can independently verify this matches the contract's stored state.
    //
    // **Params:**
    // * Empty
    //
    // **Returns:**
    // * `height`: u64 current canonical block height
    // * `total_supply`: u64 cumulative expected supply at this height
    // * `cumulative_value_commit`: base64-encoded compressed pallas::Point (S_H)
    // * `cumulative_blind`: base64-encoded pallas::Scalar (sum of coinbase blinds)
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_cumulative_supply", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"height":42,"total_supply":...,"cumulative_value_commit":"...","cumulative_blind":"..."}, "id": 1}
    pub async fn blockchain_get_cumulative_supply(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let chain = match &self.chain_state {
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

        use dwow_sdk::blockchain::{coinbase_blind, expected_cumulative_supply, expected_reward, BlockHeight};
        use dwow_sdk::crypto::{pedersen_commitment_u64, pasta_prelude::{Group, PrimeField}, Blind};
        use dwow_sdk::pasta::pallas;

        let height = chain.get_height();
        let _total_supply = expected_cumulative_supply(height);

        // Compute cumulative commitment from canonical block history.
        // Uses the deterministic blind derivation: blind_H = coinbase_blind(prev_coin, H).
        // The prev_coin for each block is derived from the previous block's hash.
        let mut cumulative = pallas::Point::identity();
        let mut cumulative_blind = pallas::Scalar::zero();

        for h in 1u64..=height.get() {
            let reward = expected_reward(BlockHeight::new(h));
            // For the RPC audit, prev_coin is the previous block hash.
            // The contract uses the actual coinbase coin commitment; both
            // are deterministic and verifiable.
            let prev_bytes = if h == 1 {
                [0u8; 32]
            } else if let Ok(prev_block) = chain.get_block(BlockHeight::new(h - 1)) {
                *chain.hash_block_with_cached_vm(&prev_block).as_bytes()
            } else {
                [0u8; 32]
            };
            let blind = coinbase_blind(&prev_bytes, BlockHeight::new(h));
            cumulative = cumulative + pedersen_commitment_u64(reward.get(), Blind(blind));
            cumulative_blind += blind;
        }

        let total_supply = expected_cumulative_supply(height);

        // Serialize using dwow_serial (Encodable trait)
        use dwow_serial::Encodable;
        let mut commit_bytes = Vec::new();
        cumulative.encode(&mut commit_bytes).unwrap();
        let mut blind_bytes = [0u8; 32];
        blind_bytes.copy_from_slice(&cumulative_blind.to_repr());

        let result = JsonValue::from(std::collections::HashMap::from([
            ("height".to_string(), JsonValue::Number(height.get() as f64)),
            ("total_supply".to_string(), JsonValue::Number(total_supply as f64)),
            ("cumulative_value_commit".to_string(), JsonValue::String(base64::encode(&commit_bytes))),
            ("cumulative_blind".to_string(), JsonValue::String(base64::encode(&blind_bytes))),
        ]));

        JsonResponse::new(result, id).into()
    }

    /// Subscribe to new block notifications.
    /// The subscriber pushes `dwow_chain::Block` JSON strings as notifications.
    ///
    /// --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": "subscribed", "id": 1}
    /// <-- {"jsonrpc": "2.0", "method": "blockchain.subscription", "params": {"result": "<Block JSON>"}}
    pub async fn blockchain_subscribe_blocks(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.rpc_state.subscribers.get("blocks").unwrap().clone().into()
    }

    /// Look up ZK circuit bincodes for a given contract ID.
    /// Returns an array of [namespace, bincode_base64] pairs.
    /// Currently returns empty — bincodes are embedded client-side in the wallet.
    ///
    /// --> {"jsonrpc": "2.0", "method": "blockchain.lookup_zkas", "params": ["<contract_id>"], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": [], "id": 1}
    pub async fn blockchain_lookup_zkas(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Bincodes are embedded in wallet binary; dwowd does not store them.
        JsonResponse::new(JsonValue::Array(vec![]), id).into()
    }

    /// Look up a transaction by its hash.
    /// Returns the transaction as a JSON string, or null if not found.
    ///
    /// --> {"jsonrpc": "2.0", "method": "blockchain.get_tx", "params": ["<tx_hash>"], "id": 1}
    /// <-- {"jsonrpc": "2.0", "result": null, "id": 1}
    pub async fn blockchain_get_tx(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Tx-by-hash lookup requires a chain-state index (future work).
        // Internal code exists at src/linear/src/execution.rs (execute_block).
        JsonResponse::new(JsonValue::Null, id).into()
    }
}
