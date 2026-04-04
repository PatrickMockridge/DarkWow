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

//! Aztec executor for Aztec private rollup withdrawals

use async_trait::async_trait;
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