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

//! Chain executor modules

pub mod eth;
pub mod xmr;
pub mod zec;
pub mod ltc;
pub mod azt;

use super::chain::{ChainExecutor, ExternalChain, DisabledExecutor};
use super::config::Config;
use super::error::{PendingWithdrawal, Result, TxHash};
use std::sync::Arc;

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
                Arc::new(eth::EthereumExecutor::new(&config.ethereum))
            } else {
                Arc::new(DisabledExecutor)
            },
            xmr: if config.is_monero_enabled() {
                Arc::new(xmr::MoneroExecutor::new(&config.monero))
            } else {
                Arc::new(DisabledExecutor)
            },
            zec: if config.is_zcash_enabled() {
                Arc::new(zec::ZcashExecutor::new(&config.zcash))
            } else {
                Arc::new(DisabledExecutor)
            },
            ltc: if config.is_litecoin_enabled() {
                Arc::new(ltc::LitecoinExecutor::new(&config.litecoin))
            } else {
                Arc::new(DisabledExecutor)
            },
            azt: if config.is_aztec_enabled() {
                Arc::new(azt::AztecExecutor::new(&config.aztec))
            } else {
                Arc::new(DisabledExecutor)
            },
        }
    }

    /// Get executor for a specific chain
    pub fn get_executor(&self, chain: ExternalChain) -> Arc<dyn ChainExecutor> {
        match chain {
            ExternalChain::Ethereum => self.eth.clone(),
            ExternalChain::Monero => self.xmr.clone(),
            ExternalChain::Zcash => self.zec.clone(),
            ExternalChain::Aztec => self.azt.clone(),
            ExternalChain::Litecoin => self.ltc.clone(),
        }
    }

    /// Get the Ethereum executor (for direct access)
    pub fn eth(&self) -> Arc<dyn ChainExecutor> {
        self.eth.clone()
    }

    /// Get the Monero executor (for direct access)
    pub fn xmr(&self) -> Arc<dyn ChainExecutor> {
        self.xmr.clone()
    }

    /// Get the Zcash executor (for direct access)
    pub fn zec(&self) -> Arc<dyn ChainExecutor> {
        self.zec.clone()
    }

    /// Get the Litecoin executor (for direct access)
    pub fn ltc(&self) -> Arc<dyn ChainExecutor> {
        self.ltc.clone()
    }

    /// Get the Aztec executor (for direct access)
    pub fn azt(&self) -> Arc<dyn ChainExecutor> {
        self.azt.clone()
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