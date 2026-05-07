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

//! RPC client for communicating with darkfid
//!
//! This module provides the client-side interface to darkfid's JSON-RPC API,
//! enabling tau_pallas to broadcast transactions and interact with the
//! DarkWow blockchain.

use std::sync::Arc;

use darkfi::{
    rpc::client::RpcClient,
    rpc::jsonrpc::JsonRequest,
    tx::Transaction,
};
use tinyjson::JsonValue;
use darkfi_serial::Encodable;
use darkfi_sdk::tx::TransactionHash;
use smol::Executor;
use url::Url;

use crate::error::{TauPallasError, TauPallasResult};

/// Client for darkfid RPC operations
#[derive(Clone)]
pub struct DarkfidClient {
    rpc: Arc<RpcClient>,
}

impl DarkfidClient {
    /// Create a new DarkfidClient connected to the specified URL
    pub async fn new(url: &str, ex: Arc<Executor<'static>>) -> TauPallasResult<Self> {
        let url = Url::parse(url)
            .map_err(|e| TauPallasError::RpcError(format!("Failed to parse URL: {}", e)))?;

        let rpc = RpcClient::new(url, ex)
            .await
            .map_err(|e| TauPallasError::RpcError(format!("Failed to create RPC client: {}", e)))?;

        Ok(Self { rpc: Arc::new(rpc) })
    }

    /// Broadcast a transaction to the DarkWow network via darkfid
    ///
    /// This serializes the transaction to base64 and sends it to darkfid's
    /// `tx.broadcast` endpoint, which validates, adds to mempool, and
    /// broadcasts to the P2P network.
    pub async fn broadcast_tx(&self, tx: &Transaction) -> TauPallasResult<TransactionHash> {
        // Serialize transaction to base64
        let mut bytes = Vec::new();
        tx.encode(&mut bytes)
            .map_err(|e| TauPallasError::TransactionError(format!("Failed to serialize tx: {}", e)))?;

        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);

        // Call tx.broadcast
        let params = JsonValue::Array(vec![JsonValue::String(encoded)]);
        let rep = self
            .rpc
            .request(JsonRequest::new("tx.broadcast", params))
            .await
            .map_err(|e| TauPallasError::RpcError(format!("RPC call failed: {}", e)))?;

        // Parse response as transaction hash string
        let tx_hash_str = rep
            .get::<String>()
            .ok_or_else(|| TauPallasError::RpcError("Invalid response format".to_string()))?;

        let tx_hash = tx_hash_str
            .parse::<TransactionHash>()
            .map_err(|e| TauPallasError::RpcError(format!("Failed to parse tx hash: {}", e)))?;

        Ok(tx_hash)
    }
}
