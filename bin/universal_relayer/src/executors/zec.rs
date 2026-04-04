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

//! Zcash executor for ZEC withdrawals

use async_trait::async_trait;
use super::super::chain::{ChainExecutor, ExternalChain};
use super::super::config::ZcashConfig;
use super::super::error::{PendingWithdrawal, Result, TxHash};

/// Zcash executor implementation
pub struct ZcashExecutor {
    config: ZcashConfig,
}

impl ZcashExecutor {
    /// Create a new Zcash executor
    pub fn new(config: &ZcashConfig) -> Self {
        Self { config: config.clone() }
    }

    /// Derive Zcash address from recipient_hash
    fn derive_address(&self, _recipient_hash: &[u8; 32]) -> String {
        // In production: decode recipient_hash to get ZEC address (taddr or zaddr)
        // If shielded_pool is true, derive a z-address
        // Otherwise, derive a transparent address
        if self.config.shielded_pool {
            "zs1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
        } else {
            "t1xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
        }
    }
}

#[async_trait]
impl ChainExecutor for ZcashExecutor {
    async fn execute(&self, withdrawal: &PendingWithdrawal) -> Result<TxHash> {
        tracing::info!(
            "Executing ZEC withdrawal: {} to {} (shielded: {})",
            withdrawal.amount,
            self.derive_address(&withdrawal.recipient_hash),
            self.config.shielded_pool
        );

        // In production:
        // 1. Connect to zcashd via RPC
        // 2. For shielded: use z_sendmany to zaddr
        // 3. For transparent: use sendtoaddress
        // 4. Wait for 10 confirmations

        let tx_hash = blake3::hash(&withdrawal.recipient_hash);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(tx_hash.as_bytes());

        tracing::info!("ZEC withdrawal submitted: {}", hex::encode(hash));
        Ok(TxHash { chain: ExternalChain::Zcash.as_u8(), hash })
    }

    fn chain(&self) -> ExternalChain {
        ExternalChain::Zcash
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled && !self.config.node_rpc_url.is_empty()
    }

    async fn estimate_fee(&self, _withdrawal: &PendingWithdrawal) -> Result<u64> {
        // ZEC uses a default relay fee of 0.00001 ZEC
        Ok(1000) // 0.00001 ZEC in zatoshis
    }

    async fn verify_confirmation(&self, tx_hash: &TxHash) -> Result<bool> {
        tracing::debug!("Verifying ZEC tx confirmation: {}", hex::encode(tx_hash.hash));
        // In production: query zcashd RPC for confirmations
        Ok(true)
    }
}