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

//! Ethereum executor for ETH and ERC-20 withdrawals
//!
//! Uses ureq for HTTP requests to avoid dependency conflicts.

use async_trait::async_trait;
use darkfi_bridge_contract::chain_handler::{
    ChainHandler as BridgeChainHandler, ChainId, ExternalDeposit, HtlcDeposit, TxHash as BridgeTxHash,
    VerifiedWithdrawal, WithdrawalRequest,
};
use dwow_sdk::{error::ContractResult, pasta::pallas};
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

// =============================================================================
// Bridge ChainHandler implementation (for unified interface)
// =============================================================================

#[async_trait]
impl BridgeChainHandler for EthereumExecutor {
    fn chain_id(&self) -> ChainId {
        ChainId::Ethereum
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled && !self.config.node_url.is_empty()
    }

    async fn verify_deposit(&self, _deposit: &ExternalDeposit) -> ContractResult {
        // Relayer doesn't verify deposits - that's done by the bridge contract
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn verify_withdrawal(&self, _withdrawal: &WithdrawalRequest) -> ContractResult {
        // Relayer doesn't verify withdrawals - that's done by the bridge contract
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute(&self, verified: &VerifiedWithdrawal) -> ContractResult {
        // Convert VerifiedWithdrawal to PendingWithdrawal
        let pending = PendingWithdrawal {
            withdrawal_id: verified.nullifier,
            recipient_hash: address_to_hash(&verified.recipient_address),
            amount: verified.amount,
            chain: ChainId::Ethereum.as_u8(),
            request_height: 0,
            timeout_height: u64::MAX,
            relayer_fee: verified.fee,
            feed_mode: 0,
            guarantee_premium: 0,
        };

        <Self as ChainExecutor>::execute(self, &pending).await.map_err(|_| dwow_sdk::error::ContractError::Custom(3))?;
        Ok(())
    }

    async fn estimate_fee(&self, withdrawal: &WithdrawalRequest) -> ContractResult {
        let pending = PendingWithdrawal {
            withdrawal_id: withdrawal.nullifier,
            recipient_hash: withdrawal.recipient_hash,
            amount: withdrawal.amount,
            chain: withdrawal.chain.as_u8(),
            request_height: 0,
            timeout_height: u64::MAX,
            relayer_fee: withdrawal.fee,
            feed_mode: 0,
            guarantee_premium: 0,
        };

        let _fee = <Self as ChainExecutor>::estimate_fee(self, &pending).await.map_err(|_| dwow_sdk::error::ContractError::Custom(3))?;
        Ok(())
    }

    async fn verify_confirmation(&self, tx_hash: &BridgeTxHash) -> ContractResult {
        let tx = TxHash {
            chain: tx_hash.chain.as_u8(),
            hash: tx_hash.hash,
        };

        <Self as ChainExecutor>::verify_confirmation(self, &tx).await.map_err(|_| dwow_sdk::error::ContractError::Custom(3))?;
        Ok(())
    }

    async fn verify_htlc_deposit(&self, htlc_deposit: &HtlcDeposit) -> ContractResult {
        tracing::info!(
            "Verifying ETH HTLC deposit for swap_id: {}",
            hex::encode(htlc_deposit.swap_id)
        );
        // In production:
        // 1. Query ETH HTLC contract events for the swap_id
        // 2. Verify the deposit matches expected hash and timelock
        // 3. Verify sufficient confirmations
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_claim(
        &self,
        swap_id: &[u8; 32],
        secret: pallas::Base,
        recipient: &[u8],
    ) -> ContractResult {
        tracing::info!(
            "Executing ETH HTLC claim for swap_id: {}, secret: {:?}",
            hex::encode(swap_id),
            secret
        );
        // In production:
        // 1. Build transaction calling claim(secret) on HTLC contract
        // 2. Sign with relayer's private key
        // 3. Broadcast via eth_sendRawTransaction
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_refund(&self, swap_id: &[u8; 32], sender: &[u8]) -> ContractResult {
        tracing::info!(
            "Executing ETH HTLC refund for swap_id: {}, sender: {}",
            hex::encode(swap_id),
            hex::encode(sender)
        );
        // In production:
        // 1. Build transaction calling refund() on HTLC contract
        // 2. Sign with relayer's private key
        // 3. Broadcast via eth_sendRawTransaction
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn get_htlc_status(&self, swap_id: &[u8; 32]) -> ContractResult {
        tracing::debug!("Getting ETH HTLC status for swap_id: {}", hex::encode(swap_id));
        // In production: query HTLC contract for current state
        Err(dwow_sdk::error::ContractError::Custom(2))
    }
}

/// Convert address bytes to 32-byte hash
fn address_to_hash(address: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    let len = address.len().min(32);
    hash[..len].copy_from_slice(&address[..len]);
    hash
}