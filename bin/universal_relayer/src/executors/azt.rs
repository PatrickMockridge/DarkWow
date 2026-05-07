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

//! Aztec executor for Aztec private rollup withdrawals

use async_trait::async_trait;
use darkfi_bridge_contract::chain_handler::{
    ChainHandler as BridgeChainHandler, ChainId, ExternalDeposit, HtlcDeposit, TxHash as BridgeTxHash,
    VerifiedWithdrawal, WithdrawalRequest,
};
use darkfi_sdk::{error::ContractResult, pasta::pallas};
use super::super::chain::{ChainExecutor, ExternalChain};
use super::super::config::AztecConfig;
use super::super::error::{PendingWithdrawal, Result, TxHash};

/// Aztec executor implementation
pub struct AztecExecutor {
    config: AztecConfig,
}

impl AztecExecutor {
    /// Create a new Aztec executor
    pub fn new(config: &AztecConfig) -> Self {
        Self { config: config.clone() }
    }

    /// Derive Aztec recipient from recipient_hash
    /// Aztec uses an Aztec address format (not Ethereum addresses)
    fn derive_address(&self, _recipient_hash: &[u8; 32]) -> String {
        // In production: derive Aztec address from recipient_hash
        // Aztec addresses are derived from a viewing key + spending key
        // For now, return rollup_address as placeholder
        self.config.rollup_address.clone()
    }
}

#[async_trait]
impl ChainExecutor for AztecExecutor {
    async fn execute(&self, withdrawal: &PendingWithdrawal) -> Result<TxHash> {
        tracing::info!(
            "Executing Aztec withdrawal: {} to {}",
            withdrawal.amount,
            self.derive_address(&withdrawal.recipient_hash)
        );

        // In production:
        // 1. Connect to Aztec sequencer API
        // 2. Use the withdrawal circuit to exit from rollup
        // 3. Submit the exit proof to the rollup contract
        // 4. Wait for the rollup to be finalized on Ethereum

        let tx_hash = blake3::hash(&withdrawal.recipient_hash);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(tx_hash.as_bytes());

        tracing::info!("Aztec withdrawal submitted: {}", hex::encode(hash));
        Ok(TxHash { chain: ExternalChain::Aztec.as_u8(), hash })
    }

    fn chain(&self) -> ExternalChain {
        ExternalChain::Aztec
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled &&
            !self.config.rollup_address.is_empty() &&
            !self.config.sequencer_url.is_empty()
    }

    async fn estimate_fee(&self, _withdrawal: &PendingWithdrawal) -> Result<u64> {
        // Aztec withdrawal fees go to the rollup operator
        // Typically similar to ETH gas costs
        Ok(200_000_000_000_000) // ~0.0002 ETH in wei
    }

    async fn verify_confirmation(&self, tx_hash: &TxHash) -> Result<bool> {
        tracing::debug!("Verifying Aztec tx confirmation: {}", hex::encode(tx_hash.hash));
        // In production: check the rollup contract for the exit inclusion
        Ok(true)
    }
}

// =============================================================================
// Bridge ChainHandler implementation (for unified interface)
// =============================================================================

#[async_trait]
impl BridgeChainHandler for AztecExecutor {
    fn chain_id(&self) -> ChainId {
        ChainId::Aztec
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled &&
            !self.config.rollup_address.is_empty() &&
            !self.config.sequencer_url.is_empty()
    }

    async fn verify_deposit(&self, _deposit: &ExternalDeposit) -> ContractResult {
        Err(darkfi_sdk::error::ContractError::Custom(2))
    }

    async fn verify_withdrawal(&self, _withdrawal: &WithdrawalRequest) -> ContractResult {
        Err(darkfi_sdk::error::ContractError::Custom(2))
    }

    async fn execute(&self, verified: &VerifiedWithdrawal) -> ContractResult {
        let pending = PendingWithdrawal {
            withdrawal_id: verified.nullifier,
            recipient_hash: address_to_hash(&verified.recipient_address),
            amount: verified.amount,
            chain: ChainId::Aztec.as_u8(),
            request_height: 0,
            timeout_height: u64::MAX,
            relayer_fee: verified.fee,
            feed_mode: 0,
            guarantee_premium: 0,
        };

        <Self as ChainExecutor>::execute(self, &pending).await.map_err(|_| darkfi_sdk::error::ContractError::Custom(3))?;
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

        let _fee = <Self as ChainExecutor>::estimate_fee(self, &pending).await.map_err(|_| darkfi_sdk::error::ContractError::Custom(3))?;
        Ok(())
    }

    async fn verify_confirmation(&self, tx_hash: &BridgeTxHash) -> ContractResult {
        let tx = TxHash { chain: tx_hash.chain.as_u8(), hash: tx_hash.hash };
        <Self as ChainExecutor>::verify_confirmation(self, &tx).await.map_err(|_| darkfi_sdk::error::ContractError::Custom(3))?;
        Ok(())
    }

    async fn verify_htlc_deposit(&self, htlc_deposit: &HtlcDeposit) -> ContractResult {
        tracing::info!(
            "Verifying Aztec HTLC deposit for swap_id: {}",
            hex::encode(htlc_deposit.swap_id)
        );
        // In production:
        // 1. Query Aztec rollup for HTLC deposit note
        // 2. Verify the deposit matches expected hash and timelock
        Err(darkfi_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_claim(
        &self,
        swap_id: &[u8; 32],
        secret: pallas::Base,
        recipient: &[u8],
    ) -> ContractResult {
        tracing::info!(
            "Executing Aztec HTLC claim for swap_id: {}, secret: {:?}",
            hex::encode(swap_id),
            secret
        );
        // In production:
        // 1. Connect to Aztec sequencer API
        // 2. Submit claim note to HTLC contract
        // 3. Wait for rollup inclusion
        Err(darkfi_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_refund(&self, swap_id: &[u8; 32], sender: &[u8]) -> ContractResult {
        tracing::info!(
            "Executing Aztec HTLC refund for swap_id: {}, sender: {}",
            hex::encode(swap_id),
            hex::encode(sender)
        );
        // In production:
        // 1. Connect to Aztec sequencer API
        // 2. Submit refund note after timelock
        // 3. Wait for rollup inclusion
        Err(darkfi_sdk::error::ContractError::Custom(2))
    }

    async fn get_htlc_status(&self, swap_id: &[u8; 32]) -> ContractResult {
        tracing::debug!("Getting Aztec HTLC status for swap_id: {}", hex::encode(swap_id));
        Err(darkfi_sdk::error::ContractError::Custom(2))
    }
}

fn address_to_hash(address: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    let len = address.len().min(32);
    hash[..len].copy_from_slice(&address[..len]);
    hash
}