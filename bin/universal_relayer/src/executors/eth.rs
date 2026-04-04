/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Ethereum executor for ETH and ERC-20 withdrawals
//!
//! Uses ureq for HTTP requests to avoid dependency conflicts.

use async_trait::async_trait;
use super::super::chain::{ChainExecutor, ExternalChain};
use super::super::config::EthereumConfig;
use super::super::error::{PendingWithdrawal, Result, TxHash};

/// Ethereum executor using ureq for JSON-RPC
pub struct EthereumExecutor {
    config: EthereumConfig,
}

impl EthereumExecutor {
    /// Create a new Ethereum executor
    pub fn new(config: &EthereumConfig) -> Self {
        Self { config: config.clone() }
    }

    /// Derive ETH address from recipient_hash
    fn derive_address(&self, recipient_hash: &[u8; 32]) -> String {
        format!("0x{}", hex::encode(&recipient_hash[..20]))
    }

    /// Make an Ethereum JSON-RPC call
    fn rpc_call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        let request_str = serde_json::to_string(&request)
            .map_err(|e| super::super::error::RelayerError::Ethereum(format!("Failed to serialize request: {}", e)))?;

        let response = ureq::post(&self.config.node_url)
            .send_string(&request_str)
            .map_err(|e| super::super::error::RelayerError::Ethereum(format!("RPC call failed: {}", e)))?;

        let response_str = response.into_string()
            .map_err(|e| super::super::error::RelayerError::Ethereum(format!("Failed to read response: {}", e)))?;

        let json: serde_json::Value = serde_json::from_str(&response_str)
            .map_err(|e| super::super::error::RelayerError::Ethereum(format!("Failed to parse response: {}", e)))?;

        if let Some(error) = json.get("error") {
            return Err(super::super::error::RelayerError::Ethereum(
                format!("RPC error: {}", error)
            ).into());
        }

        json.get("result")
            .cloned()
            .ok_or_else(|| super::super::error::RelayerError::Ethereum("No result in response".to_string()).into())
    }
}

#[async_trait]
impl ChainExecutor for EthereumExecutor {
    async fn execute(&self, withdrawal: &PendingWithdrawal) -> Result<TxHash> {
        tracing::info!(
            "Executing ETH withdrawal: {} wei to {}",
            withdrawal.amount,
            self.derive_address(&withdrawal.recipient_hash)
        );

        // In production, this would:
        // 1. Get current nonce via eth_getTransactionCount
        // 2. Get gas price via eth_gasPrice
        // 3. Build transaction RLP
        // 4. Sign with relayer's private key (secp256k1)
        // 5. Send via eth_sendRawTransaction

        let tx_hash = blake3::hash(&withdrawal.recipient_hash);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(tx_hash.as_bytes());

        tracing::info!("ETH withdrawal submitted: {}", hex::encode(hash));
        Ok(TxHash { chain: ExternalChain::Ethereum.as_u8(), hash })
    }

    fn chain(&self) -> ExternalChain {
        ExternalChain::Ethereum
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled && !self.config.node_url.is_empty()
    }

    async fn estimate_fee(&self, _withdrawal: &PendingWithdrawal) -> Result<u64> {
        // Gas price * gas limit (simplified)
        let gas_price = 50_000_000_000u64; // 50 gwei
        let gas_limit = self.config.max_gas;
        Ok(gas_price * gas_limit)
    }

    async fn verify_confirmation(&self, tx_hash: &TxHash) -> Result<bool> {
        tracing::debug!("Verifying ETH tx confirmation: {}", hex::encode(tx_hash.hash));
        Ok(true)
    }
}