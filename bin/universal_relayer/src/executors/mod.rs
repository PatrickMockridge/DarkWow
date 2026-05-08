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

//! Chain executor modules
//!
//! Each executor implements both:
//! - `ChainExecutor`: For internal relayer operations
//! - `BridgeChainHandler`: For unified interface with bridge contract

pub mod eth;
pub mod xmr;
pub mod zec;
pub mod ltc;
pub mod azt;

use async_trait::async_trait;
use dwow_bridge_contract::chain_handler::{
    ChainHandler as BridgeChainHandlerTrait, ChainId, ExternalDeposit, HtlcDeposit,
    TxHash as BridgeTxHash, VerifiedWithdrawal, WithdrawalRequest,
};
use dwow_sdk::{error::ContractResult, pasta::pallas};
use super::chain::{ChainExecutor, ExternalChain, DisabledExecutor};
use super::config::Config;
use super::error::{PendingWithdrawal, Result, TxHash};
use std::sync::Arc;

/// Wrapper that adapts `Arc<dyn ChainExecutor>` to implement `BridgeChainHandler`
struct HandlerAdapter {
    inner: Arc<dyn ChainExecutor>,
}

impl HandlerAdapter {
    fn new(executor: Arc<dyn ChainExecutor>) -> Self {
        Self { inner: executor }
    }

    fn address_to_hash(address: &[u8]) -> [u8; 32] {
        let mut hash = [0u8; 32];
        let len = address.len().min(32);
        hash[..len].copy_from_slice(&address[..len]);
        hash
    }
}

#[async_trait]
impl BridgeChainHandlerTrait for HandlerAdapter {
    fn chain_id(&self) -> ChainId {
        match self.inner.chain() {
            ExternalChain::Ethereum => ChainId::Ethereum,
            ExternalChain::Monero => ChainId::Monero,
            ExternalChain::Zcash => ChainId::Zcash,
            ExternalChain::Aztec => ChainId::Aztec,
            ExternalChain::Litecoin => ChainId::Litecoin,
        }
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
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
        let pending = PendingWithdrawal {
            withdrawal_id: verified.nullifier,
            recipient_hash: Self::address_to_hash(&verified.recipient_address),
            amount: verified.amount,
            chain: verified.chain.as_u8(),
            request_height: 0,
            timeout_height: u64::MAX,
            relayer_fee: verified.fee,
            feed_mode: 0,
            guarantee_premium: 0,
        };

        self.inner.execute(&pending).await.map_err(|_| dwow_sdk::error::ContractError::Custom(3))?;
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

        let _fee = self.inner.estimate_fee(&pending).await.map_err(|_| dwow_sdk::error::ContractError::Custom(3))?;
        Ok(())
    }

    async fn verify_confirmation(&self, tx_hash: &BridgeTxHash) -> ContractResult {
        let tx = TxHash { chain: tx_hash.chain.as_u8(), hash: tx_hash.hash };
        self.inner.verify_confirmation(&tx).await.map_err(|_| dwow_sdk::error::ContractError::Custom(3))?;
        Ok(())
    }

    async fn verify_htlc_deposit(&self, htlc_deposit: &HtlcDeposit) -> ContractResult {
        tracing::info!(
            "HandlerAdapter verifying HTLC deposit for swap_id: {}",
            hex::encode(htlc_deposit.swap_id)
        );
        // HTLC deposit verification is chain-specific, delegate to the executor
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_claim(
        &self,
        swap_id: &[u8; 32],
        secret: pallas::Base,
        recipient: &[u8],
    ) -> ContractResult {
        tracing::info!(
            "HandlerAdapter executing HTLC claim for swap_id: {}, secret: {:?}",
            hex::encode(swap_id),
            secret
        );
        // HTLC claim execution is chain-specific, delegate to the executor
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_refund(&self, swap_id: &[u8; 32], sender: &[u8]) -> ContractResult {
        tracing::info!(
            "HandlerAdapter executing HTLC refund for swap_id: {}, sender: {}",
            hex::encode(swap_id),
            hex::encode(sender)
        );
        // HTLC refund execution is chain-specific, delegate to the executor
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn get_htlc_status(&self, swap_id: &[u8; 32]) -> ContractResult {
        tracing::debug!(
            "HandlerAdapter getting HTLC status for swap_id: {}",
            hex::encode(swap_id)
        );
        Err(dwow_sdk::error::ContractError::Custom(2))
    }
}

/// Executor registry - holds all chain executors
pub struct ExecutorRegistry {
    eth: Arc<dyn ChainExecutor>,
    xmr: Arc<dyn ChainExecutor>,
    zec: Arc<dyn ChainExecutor>,
    ltc: Arc<dyn ChainExecutor>,
    azt: Arc<dyn ChainExecutor>,
}

impl ExecutorRegistry {
    /// Create a new executor registry from config
    pub fn new(config: &Config) -> Self {
        Self {
            eth: if config.is_ethereum_enabled() {
                Arc::new(eth::EthereumExecutor::new(&config.ethereum)) as Arc<dyn ChainExecutor>
            } else {
                Arc::new(DisabledExecutor::new()) as Arc<dyn ChainExecutor>
            },
            xmr: if config.is_monero_enabled() {
                Arc::new(xmr::MoneroExecutor::new(&config.monero)) as Arc<dyn ChainExecutor>
            } else {
                Arc::new(DisabledExecutor::new()) as Arc<dyn ChainExecutor>
            },
            zec: if config.is_zcash_enabled() {
                Arc::new(zec::ZcashExecutor::new(&config.zcash)) as Arc<dyn ChainExecutor>
            } else {
                Arc::new(DisabledExecutor::new()) as Arc<dyn ChainExecutor>
            },
            ltc: if config.is_litecoin_enabled() {
                Arc::new(ltc::LitecoinExecutor::new(&config.litecoin)) as Arc<dyn ChainExecutor>
            } else {
                Arc::new(DisabledExecutor::new()) as Arc<dyn ChainExecutor>
            },
            azt: if config.is_aztec_enabled() {
                Arc::new(azt::AztecExecutor::new(&config.aztec)) as Arc<dyn ChainExecutor>
            } else {
                Arc::new(DisabledExecutor::new()) as Arc<dyn ChainExecutor>
            },
        }
    }

    /// Get executor for a specific chain (as ChainExecutor trait)
    pub fn get_executor(&self, chain: ExternalChain) -> Arc<dyn ChainExecutor> {
        match chain {
            ExternalChain::Ethereum => self.eth.clone(),
            ExternalChain::Monero => self.xmr.clone(),
            ExternalChain::Zcash => self.zec.clone(),
            ExternalChain::Aztec => self.azt.clone(),
            ExternalChain::Litecoin => self.ltc.clone(),
        }
    }

    /// Get ChainHandler for a specific chain (for bridge integration)
    pub fn get_handler(&self, chain: ExternalChain) -> Option<Arc<dyn BridgeChainHandlerTrait + 'static>> {
        let executor = match chain {
            ExternalChain::Ethereum => self.eth.clone(),
            ExternalChain::Monero => self.xmr.clone(),
            ExternalChain::Zcash => self.zec.clone(),
            ExternalChain::Aztec => self.azt.clone(),
            ExternalChain::Litecoin => self.ltc.clone(),
        };

        if executor.is_enabled() {
            Some(Arc::new(HandlerAdapter::new(executor)) as Arc<dyn BridgeChainHandlerTrait>)
        } else {
            None
        }
    }

    /// Get all handlers as a registry-compatible structure
    pub fn handlers(&self) -> Vec<(ExternalChain, Arc<dyn BridgeChainHandlerTrait + 'static>)> {
        let mut result = Vec::new();
        if let Some(h) = self.get_handler(ExternalChain::Ethereum) {
            result.push((ExternalChain::Ethereum, h));
        }
        if let Some(h) = self.get_handler(ExternalChain::Monero) {
            result.push((ExternalChain::Monero, h));
        }
        if let Some(h) = self.get_handler(ExternalChain::Zcash) {
            result.push((ExternalChain::Zcash, h));
        }
        if let Some(h) = self.get_handler(ExternalChain::Litecoin) {
            result.push((ExternalChain::Litecoin, h));
        }
        if let Some(h) = self.get_handler(ExternalChain::Aztec) {
            result.push((ExternalChain::Aztec, h));
        }
        result
    }

    /// Get all enabled chains
    pub fn enabled_chains(&self) -> Vec<ExternalChain> {
        let mut chains = Vec::new();
        if self.eth.is_enabled() {
            chains.push(ExternalChain::Ethereum);
        }
        if self.xmr.is_enabled() {
            chains.push(ExternalChain::Monero);
        }
        if self.zec.is_enabled() {
            chains.push(ExternalChain::Zcash);
        }
        if self.ltc.is_enabled() {
            chains.push(ExternalChain::Litecoin);
        }
        if self.azt.is_enabled() {
            chains.push(ExternalChain::Aztec);
        }
        chains
    }
}
