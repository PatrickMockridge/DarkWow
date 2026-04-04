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

//! Unified configuration for the universal relayer

use serde::Deserialize;
use std::path::PathBuf;

/// DarkFi connection settings
#[derive(Debug, Clone, Deserialize)]
pub struct DarkFiConfig {
    /// URL for darkfid RPC (default: http://127.0.0.1:8543)
    pub darkfid_url: String,

    /// Polling interval in seconds for checking withdrawals
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,

    /// Maximum concurrent withdrawals to process
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_withdrawals: usize,
}

fn default_poll_interval() -> u64 { 10 }
fn default_max_concurrent() -> usize { 10 }

/// Ethereum chain configuration
#[derive(Debug, Clone, Deserialize)]
pub struct EthereumConfig {
    /// Enable Ethereum withdrawals
    #[serde(default)]
    pub enabled: bool,

    /// Ethereum node URL (Infura/Alchemy/self-hosted)
    pub node_url: String,

    /// Relayer private key (hex encoded, 0x prefixed)
    pub relayer_private_key: String,

    /// Maximum gas price in gwei
    #[serde(default = "default_max_gas_gwei")]
    pub max_gas_gwei: u64,

    /// Maximum gas to spend on a transaction
    #[serde(default = "default_max_gas")]
    pub max_gas: u64,
}

fn default_max_gas_gwei() -> u64 { 50 }
fn default_max_gas() -> u64 { 21000 }

impl EthereumConfig {
    /// Check if the config is valid
    pub fn is_valid(&self) -> bool {
        self.enabled && !self.node_url.is_empty() && !self.relayer_private_key.is_empty()
    }
}

impl Default for EthereumConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_url: String::new(),
            relayer_private_key: String::new(),
            max_gas_gwei: default_max_gas_gwei(),
            max_gas: default_max_gas(),
        }
    }
}

/// Monero chain configuration
#[derive(Debug, Clone, Deserialize)]
pub struct MoneroConfig {
    /// Enable Monero withdrawals
    #[serde(default)]
    pub enabled: bool,

    /// Monero wallet RPC URL
    pub wallet_rpc_url: String,

    /// Monero full node RPC URL
    pub node_rpc_url: String,

    /// View key for observing deposits (cannot spend)
    pub view_key: String,

    /// Minimum confirmations before processing
    #[serde(default = "default_xmr_confirmations")]
    pub min_confirmations: u64,

    /// Relayer fee address (one-time address)
    pub fee_address: String,
}

fn default_xmr_confirmations() -> u64 { 10 }

impl MoneroConfig {
    pub fn is_valid(&self) -> bool {
        self.enabled &&
            !self.wallet_rpc_url.is_empty() &&
            !self.node_rpc_url.is_empty() &&
            !self.fee_address.is_empty()
    }
}

impl Default for MoneroConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            wallet_rpc_url: String::new(),
            node_rpc_url: String::new(),
            view_key: String::new(),
            min_confirmations: default_xmr_confirmations(),
            fee_address: String::new(),
        }
    }
}

/// Zcash chain configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ZcashConfig {
    /// Enable Zcash withdrawals
    #[serde(default)]
    pub enabled: bool,

    /// Zcash node RPC URL
    pub node_rpc_url: String,

    /// Transparent or shielded pool
    #[serde(default)]
    pub shielded_pool: bool,

    /// Minimum confirmations
    #[serde(default = "default_zec_confirmations")]
    pub min_confirmations: u64,
}

fn default_zec_confirmations() -> u64 { 10 }

impl ZcashConfig {
    pub fn is_valid(&self) -> bool {
        self.enabled && !self.node_rpc_url.is_empty()
    }
}

impl Default for ZcashConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_rpc_url: String::new(),
            shielded_pool: false,
            min_confirmations: default_zec_confirmations(),
        }
    }
}

/// Litecoin chain configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LitecoinConfig {
    /// Enable Litecoin withdrawals
    #[serde(default)]
    pub enabled: bool,

    /// Litecoin node RPC URL
    pub node_rpc_url: String,

    /// RPC username
    pub rpc_user: String,

    /// RPC password
    pub rpc_pass: String,

    /// Minimum confirmations
    #[serde(default = "default_ltc_confirmations")]
    pub min_confirmations: u64,
}

fn default_ltc_confirmations() -> u64 { 6 }

impl LitecoinConfig {
    pub fn is_valid(&self) -> bool {
        self.enabled && !self.node_rpc_url.is_empty() && !self.rpc_user.is_empty()
    }
}

impl Default for LitecoinConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            node_rpc_url: String::new(),
            rpc_user: String::new(),
            rpc_pass: String::new(),
            min_confirmations: default_ltc_confirmations(),
        }
    }
}

/// Aztec chain configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AztecConfig {
    /// Enable Aztec withdrawals
    #[serde(default)]
    pub enabled: bool,

    /// Aztec rollup contract address
    pub rollup_address: String,

    /// Aztec sequencer URL
    pub sequencer_url: String,

    /// Minimum confirmations after rollup
    #[serde(default = "default_azt_confirmations")]
    pub min_confirmations: u64,
}

fn default_azt_confirmations() -> u64 { 5 }

impl AztecConfig {
    pub fn is_valid(&self) -> bool {
        self.enabled && !self.rollup_address.is_empty() && !self.sequencer_url.is_empty()
    }
}

impl Default for AztecConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rollup_address: String::new(),
            sequencer_url: String::new(),
            min_confirmations: default_azt_confirmations(),
        }
    }
}

/// Relayer global settings
#[derive(Debug, Clone, Deserialize)]
pub struct RelayerSettings {
    /// Blocks before a withdrawal can be cancelled (timeout)
    #[serde(default = "default_timeout_blocks")]
    pub timeout_blocks: u64,

    /// Relayer fee percentage (1 = 1%)
    #[serde(default = "default_fee_percentage")]
    pub fee_percentage: u64,

    /// Log file path
    #[serde(default)]
    pub log_file: Option<String>,
}

fn default_timeout_blocks() -> u64 { 100 }
fn default_fee_percentage() -> u64 { 1 }

impl Default for RelayerSettings {
    fn default() -> Self {
        Self {
            timeout_blocks: 100,
            fee_percentage: 1,
            log_file: None,
        }
    }
}

/// Master configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub darkfi: DarkFiConfig,

    #[serde(default)]
    pub ethereum: EthereumConfig,

    #[serde(default)]
    pub monero: MoneroConfig,

    #[serde(default)]
    pub zcash: ZcashConfig,

    #[serde(default)]
    pub litecoin: LitecoinConfig,

    #[serde(default)]
    pub aztec: AztecConfig,

    #[serde(default)]
    pub relayer: RelayerSettings,
}

impl Default for DarkFiConfig {
    fn default() -> Self {
        Self {
            darkfid_url: "http://127.0.0.1:8543".to_string(),
            poll_interval_secs: default_poll_interval(),
            max_concurrent_withdrawals: default_max_concurrent(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &PathBuf) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // At least one chain must be enabled
        let any_enabled = self.ethereum.is_valid() ||
            self.monero.is_valid() ||
            self.zcash.is_valid() ||
            self.litecoin.is_valid() ||
            self.aztec.is_valid();

        if !any_enabled {
            errors.push("At least one chain must be enabled".to_string());
        }

        errors
    }

    /// Check if Ethereum is enabled and valid
    pub fn is_ethereum_enabled(&self) -> bool {
        self.ethereum.is_valid()
    }

    /// Check if Monero is enabled and valid
    pub fn is_monero_enabled(&self) -> bool {
        self.monero.is_valid()
    }

    /// Check if Zcash is enabled and valid
    pub fn is_zcash_enabled(&self) -> bool {
        self.zcash.is_valid()
    }

    /// Check if Litecoin is enabled and valid
    pub fn is_litecoin_enabled(&self) -> bool {
        self.litecoin.is_valid()
    }

    /// Check if Aztec is enabled and valid
    pub fn is_aztec_enabled(&self) -> bool {
        self.aztec.is_valid()
    }
}