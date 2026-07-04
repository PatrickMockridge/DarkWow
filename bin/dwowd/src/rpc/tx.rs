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
    // Submit a transaction with contract calls to the darkwow-devnet mempool.
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

        // Check we're in darkwow-devnet mode
        let mempool = match &self.mempool {
            Some(mp) => mp.clone(),
            None => {
                error!(target: "dwowd::rpc::tx_submit_linear", "tx.submit_linear is only available in darkwow-devnet mode");
                return JsonError::new(
                    InternalError,
                    Some("tx.submit_linear is only available in darkwow-devnet mode".to_string()),
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

        let chain_tx: dwow_chain::Transaction = match serde_json::from_slice(&tx_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd::rpc::tx_submit_linear", "Failed deserializing bytes into Transaction: {}", e);
                return JsonError::new(InvalidParams, Some(format!("Invalid transaction format: {}", e)), id).into()
            }
        };

        // Reject coinbase transactions from the mempool — these are miner-only
        if chain_tx.coinbase.is_some() {
            error!(target: "dwowd::rpc::tx_submit_linear", "Rejecting coinbase transaction from mempool");
            return JsonError::new(InvalidParams, Some("Coinbase transactions cannot be submitted to mempool".to_string()), id).into()
        }

        // Reject transactions with no meaningful content
        if chain_tx.inputs.is_empty() && chain_tx.outputs.is_empty() && chain_tx.contract_calls.is_empty() {
            error!(target: "dwowd::rpc::tx_submit_linear", "Rejecting empty transaction");
            return JsonError::new(InvalidParams, Some("Transaction has no inputs, outputs, or contract calls".to_string()), id).into()
        }

        let tx_hash = format!("{}", chain_tx.hash());

        // Add to mempool
        if let Err(e) = mempool.add(chain_tx.clone()).await {
            error!(target: "dwowd::rpc::tx_submit_linear", "Failed to add transaction to mempool: {}", e);
            return JsonError::new(InternalError, Some(format!("Failed to add to mempool: {}", e)), id).into()
        };

        // Relay to all P2P peers — convert to core tx and broadcast.
        // Matches ProtocolTxHandler pattern in reverse.
        {
            use dwow_sdk::dark_tree::DarkLeaf;
            use dwow_sdk::tx::ContractCall;
            let core_tx = dwow_core::tx::Transaction {
                calls: chain_tx.contract_calls.iter().map(|c| {
                    DarkLeaf {
                        data: ContractCall {
                            // Chain tx was already validated — contract_id bytes are valid
                            contract_id: dwow_sdk::crypto::ContractId::from_bytes(c.contract_id)
                                .expect("valid contract_id from chain tx"),
                            data: c.data.clone(),
                        },
                        children_indexes: vec![],
                        parent_index: None,
                    }
                }).collect(),
                proofs: vec![],
                signatures: vec![],
                tx_commitment: [0u8; 32],
                nullifiers: chain_tx.nullifiers.clone(),
            };
            self.p2p_handler.p2p.broadcast(&core_tx).await;
        }

        info!(target: "dwowd::rpc::tx_submit_linear", "Transaction {} added to mempool and relayed", tx_hash);
        JsonResponse::new(JsonValue::String(tx_hash), id).into()
    }

    // RPCAPI:
    // Calculate the recommended fee based on recent block utilization.
    // Returns the fee in atomic DRKW units plus diagnostic info.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.calculate_fee", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"fee": 42000000, "utilization": 0.35, "blocks_sampled": 12}, "id": 1}
    pub async fn tx_calculate_fee(&self, id: u16, params: JsonValue) -> JsonResult {
        let _ = params;
        let fee = self.fee_estimator.estimate().await;
        let util = self.fee_estimator.utilization().await;
        let sampled = self.fee_estimator.blocks_sampled().await;

        let mut obj = std::collections::HashMap::new();
        obj.insert("fee".to_string(), JsonValue::Number(fee as f64));
        obj.insert("utilization".to_string(), JsonValue::Number(util));
        obj.insert("blocks_sampled".to_string(), JsonValue::Number(sampled as f64));
        JsonResponse::new(JsonValue::Object(obj), id).into()
    }

    // RPCAPI:
    // Simulate (dry-run) a transaction to check validity before broadcast.
    // Returns true if the transaction passes basic structural validation.
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

        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                return JsonError::new(InvalidParams, Some("Invalid base64 encoding".to_string()), id).into()
            }
        };

        let tx: dwow_chain::Transaction = match serde_json::from_slice(&tx_bytes) {
            Ok(v) => v,
            Err(e) => {
                return JsonError::new(InvalidParams, Some(format!("Invalid transaction format: {}", e)), id).into()
            }
        };

        // Reject coinbase transactions
        if tx.coinbase.is_some() {
            return JsonResponse::new(JsonValue::Boolean(false), id).into()
        }

        // Reject empty transactions
        if tx.inputs.is_empty() && tx.outputs.is_empty() && tx.contract_calls.is_empty() {
            return JsonResponse::new(JsonValue::Boolean(false), id).into()
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }
}
