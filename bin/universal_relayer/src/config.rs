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

#![allow(dead_code)]

//! Unified configuration for the universal relayer

use serde::Deserialize;
use std::path::PathBuf;

/// DarkWow connection settings
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

/// Feed market modes for withdrawal pricing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum FeedMode {
    /// Standard fee - relayer executes or user cancels
    #[serde(rename = "standard")]
    Standard,
    /// Delivery guarantee - fee + premium, refund on failure
    #[serde(rename = "guaranteed")]
    Guaranteed { refund_premium_bp: u32 },
}

impl Default for FeedMode {
    fn default() -> Self {
        FeedMode::Standard
    }
}

/// Stake configuration for relayer coverage
#[derive(Debug, Clone, Deserialize)]
pub struct StakeConfig {
    /// Enable stake-based coverage
    #[serde(default)]
    pub enabled: bool,

    /// DAI amount staked
    #[serde(default)]
    pub dai_amount: u64,

    /// NETHER amount staked
    #[serde(default)]
    pub nether_amount: u64,

    /// DarkWow contract ID for DAI staking
    #[serde(default)]
    pub stake_contract_id: String,

    /// DarkWow contract ID for NETHER staking
    #[serde(default)]
    pub nether_contract_id: String,

    /// Minimum stake to be active
    #[serde(default = "default_min_stake")]
    pub min_stake: u64,

    /// Maximum withdrawal amount (coverage limit)
    #[serde(default = "default_max_withdrawal")]
    pub max_withdrawal: u64,
}

fn default_min_stake() -> u64 { 1000 }
fn default_max_withdrawal() -> u64 { 10000 }

impl Default for StakeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dai_amount: 0,
            nether_amount: 0,
            stake_contract_id: String::new(),
            nether_contract_id: String::new(),
            min_stake: default_min_stake(),
            max_withdrawal: default_max_withdrawal(),
        }
    }
}

/// Staking pool configuration for shared coverage
#[derive(Debug, Clone, Deserialize)]
pub struct PoolConfig {
    /// Enable pool participation
    #[serde(default)]
    pub enabled: bool,

    /// Pool ID if joining existing pool
    #[serde(default)]
    pub pool_id: Option<String>,

    /// Minimum pool members if creating new pool
    #[serde(default = "default_min_pool_members")]
    pub min_pool_members: usize,

    /// Maximum pool coverage shared among members
    #[serde(default = "default_max_pool_coverage")]
    pub max_pool_coverage: u64,
}

fn default_min_pool_members() -> usize { 1 }
fn default_max_pool_coverage() -> u64 { 100_000 }

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pool_id: None,
            min_pool_members: default_min_pool_members(),
            max_pool_coverage: default_max_pool_coverage(),
        }
    }
}

/// Capital deployer configuration for external backing
#[derive(Debug, Clone, Deserialize)]
pub struct CapitalDeployerConfig {
    /// Enable capital deployer functionality
    #[serde(default)]
    pub enabled: bool,

    /// Contract ID for the endowment/escrow
    #[serde(default)]
    pub endowment_contract_id: String,

    /// Minimum deployment amount
    #[serde(default = "default_min_deploy")]
    pub min_deploy: u64,

    /// Maximum deployment amount
    #[serde(default = "default_max_deploy")]
    pub max_deploy: u64,

    /// Cut percentage for capital provider (basis points, e.g., 1500 = 15%)
    #[serde(default = "default_deployer_cut")]
    pub deployer_cut_bp: u32,
}

fn default_min_deploy() -> u64 { 1000 }
fn default_max_deploy() -> u64 { 100_000 }
fn default_deployer_cut() -> u32 { 1500 }

impl Default for CapitalDeployerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endowment_contract_id: String::new(),
            min_deploy: default_min_deploy(),
            max_deploy: default_max_deploy(),
            deployer_cut_bp: default_deployer_cut(),
        }
    }
}

/// Betting configuration for market-based consensus
#[derive(Debug, Clone, Deserialize)]
pub struct BettingConfig {
    /// Enable betting functionality
    #[serde(default)]
    pub enabled: bool,

    /// Betting stake contract ID
    #[serde(default)]
    pub betting_contract_id: String,

    /// Whether relayer accepts bets on their performance
    #[serde(default = "default_accept_bets")]
    pub accept_bets: bool,

    /// Maximum bet amount the relayer will honor
    #[serde(default = "default_max_bet")]
    pub max_bet_amount: u64,
}

fn default_accept_bets() -> bool { true }
fn default_max_bet() -> u64 { 1000 }

impl Default for BettingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            betting_contract_id: String::new(),
            accept_bets: default_accept_bets(),
            max_bet_amount: default_max_bet(),
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

    #[serde(default)]
    pub stake: StakeConfig,

    #[serde(default)]
    pub feed: FeedMode,

    #[serde(default)]
    pub pool: PoolConfig,

    #[serde(default)]
    pub capital_deployer: CapitalDeployerConfig,

    #[serde(default)]
    pub betting: BettingConfig,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- EthereumConfig ----------
    #[test]
    fn test_ethereum_config_is_valid() {
        let cfg = EthereumConfig {
            enabled: true,
            node_url: "https://infura.io".to_string(),
            relayer_private_key: "0xABCD".to_string(),
            ..Default::default()
        };
        assert!(cfg.is_valid());
    }

    #[test]
    fn test_ethereum_config_is_valid_disabled() {
        let cfg = EthereumConfig { enabled: false, ..Default::default() };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_ethereum_config_is_valid_empty_url() {
        let cfg = EthereumConfig {
            enabled: true,
            node_url: "".to_string(),
            relayer_private_key: "0xABCD".to_string(),
            ..Default::default()
        };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_ethereum_config_is_valid_empty_key() {
        let cfg = EthereumConfig {
            enabled: true,
            node_url: "https://infura.io".to_string(),
            relayer_private_key: "".to_string(),
            ..Default::default()
        };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_ethereum_config_default() {
        let cfg = EthereumConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.max_gas_gwei, 50);
        assert_eq!(cfg.max_gas, 21000);
    }

    // ---------- MoneroConfig ----------
    #[test]
    fn test_monero_config_is_valid() {
        let cfg = MoneroConfig {
            enabled: true,
            wallet_rpc_url: "http://127.0.0.1:18083".to_string(),
            node_rpc_url: "http://127.0.0.1:18081".to_string(),
            fee_address: "addr".to_string(),
            ..Default::default()
        };
        assert!(cfg.is_valid());
    }

    #[test]
    fn test_monero_config_is_valid_disabled() {
        let cfg = MoneroConfig { enabled: false, ..Default::default() };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_monero_config_is_valid_missing_fields() {
        let cfg = MoneroConfig {
            enabled: true,
            wallet_rpc_url: "".to_string(),
            node_rpc_url: "http://127.0.0.1:18081".to_string(),
            fee_address: "addr".to_string(),
            ..Default::default()
        };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_monero_config_is_valid_no_fee_address() {
        let cfg = MoneroConfig {
            enabled: true,
            wallet_rpc_url: "http://url".to_string(),
            node_rpc_url: "http://url".to_string(),
            fee_address: "".to_string(),
            ..Default::default()
        };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_monero_config_default() {
        let cfg = MoneroConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.min_confirmations, 10);
    }

    // ---------- ZcashConfig ----------
    #[test]
    fn test_zcash_config_is_valid() {
        let cfg = ZcashConfig {
            enabled: true,
            node_rpc_url: "http://127.0.0.1:8232".to_string(),
            ..Default::default()
        };
        assert!(cfg.is_valid());
    }

    #[test]
    fn test_zcash_config_is_valid_disabled() {
        let cfg = ZcashConfig { enabled: false, ..Default::default() };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_zcash_config_is_valid_empty_url() {
        let cfg = ZcashConfig { enabled: true, node_rpc_url: "".to_string(), ..Default::default() };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_zcash_config_default() {
        let cfg = ZcashConfig::default();
        assert!(!cfg.enabled);
        assert!(!cfg.shielded_pool);
        assert_eq!(cfg.min_confirmations, 10);
    }

    // ---------- LitecoinConfig ----------
    #[test]
    fn test_litecoin_config_is_valid() {
        let cfg = LitecoinConfig {
            enabled: true,
            node_rpc_url: "http://url".to_string(),
            rpc_user: "user".to_string(),
            ..Default::default()
        };
        assert!(cfg.is_valid());
    }

    #[test]
    fn test_litecoin_config_is_valid_disabled() {
        let cfg = LitecoinConfig { enabled: false, ..Default::default() };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_litecoin_config_is_valid_empty_user() {
        let cfg = LitecoinConfig {
            enabled: true,
            node_rpc_url: "http://url".to_string(),
            rpc_user: "".to_string(),
            ..Default::default()
        };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_litecoin_config_default() {
        let cfg = LitecoinConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.min_confirmations, 6);
    }

    // ---------- AztecConfig ----------
    #[test]
    fn test_aztec_config_is_valid() {
        let cfg = AztecConfig {
            enabled: true,
            rollup_address: "0xRollup".to_string(),
            sequencer_url: "https://aztec.network".to_string(),
            ..Default::default()
        };
        assert!(cfg.is_valid());
    }

    #[test]
    fn test_aztec_config_is_valid_disabled() {
        let cfg = AztecConfig { enabled: false, ..Default::default() };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_aztec_config_is_valid_missing_fields() {
        let cfg = AztecConfig {
            enabled: true,
            rollup_address: "".to_string(),
            sequencer_url: "https://aztec.network".to_string(),
            ..Default::default()
        };
        assert!(!cfg.is_valid());
    }

    #[test]
    fn test_aztec_config_default() {
        let cfg = AztecConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.min_confirmations, 5);
    }

    // ---------- RelayerSettings ----------
    #[test]
    fn test_relayer_settings_default() {
        let cfg = RelayerSettings::default();
        assert_eq!(cfg.timeout_blocks, 100);
        assert_eq!(cfg.fee_percentage, 1);
        assert!(cfg.log_file.is_none());
    }

    // ---------- FeedMode ----------
    #[test]
    fn test_feed_mode_default() {
        assert_eq!(FeedMode::default(), FeedMode::Standard);
    }

    #[test]
    fn test_feed_mode_deserialize_standard() {
        let toml_str = "type = \"standard\"";
        let mode: FeedMode = toml::from_str(toml_str).unwrap();
        assert_eq!(mode, FeedMode::Standard);
    }

    #[test]
    fn test_feed_mode_deserialize_guaranteed() {
        let toml_str = "type = \"guaranteed\"\nvalue = { refund_premium_bp = 500 }";
        let mode: FeedMode = toml::from_str(toml_str).unwrap();
        assert_eq!(mode, FeedMode::Guaranteed { refund_premium_bp: 500 });
    }

    // ---------- StakeConfig ----------
    #[test]
    fn test_stake_config_default() {
        let cfg = StakeConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.dai_amount, 0);
        assert_eq!(cfg.nether_amount, 0);
        assert_eq!(cfg.min_stake, 1000);
        assert_eq!(cfg.max_withdrawal, 10000);
    }

    // ---------- PoolConfig ----------
    #[test]
    fn test_pool_config_default() {
        let cfg = PoolConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.pool_id.is_none());
        assert_eq!(cfg.min_pool_members, 1);
        assert_eq!(cfg.max_pool_coverage, 100_000);
    }

    // ---------- CapitalDeployerConfig ----------
    #[test]
    fn test_capital_deployer_config_default() {
        let cfg = CapitalDeployerConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.min_deploy, 1000);
        assert_eq!(cfg.max_deploy, 100_000);
        assert_eq!(cfg.deployer_cut_bp, 1500);
    }

    // ---------- BettingConfig ----------
    #[test]
    fn test_betting_config_default() {
        let cfg = BettingConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.accept_bets);
        assert_eq!(cfg.max_bet_amount, 1000);
    }

    // ---------- DarkFiConfig ----------
    #[test]
    fn test_darkfi_config_default() {
        let cfg = DarkFiConfig::default();
        assert_eq!(cfg.darkfid_url, "http://127.0.0.1:8543");
        assert_eq!(cfg.poll_interval_secs, 10);
        assert_eq!(cfg.max_concurrent_withdrawals, 10);
    }

    // ---------- Config validate ----------
    fn default_config() -> Config {
        Config {
            darkfi: DarkFiConfig::default(),
            ethereum: EthereumConfig::default(),
            monero: MoneroConfig::default(),
            zcash: ZcashConfig::default(),
            litecoin: LitecoinConfig::default(),
            aztec: AztecConfig::default(),
            relayer: RelayerSettings::default(),
            stake: StakeConfig::default(),
            feed: FeedMode::default(),
            pool: PoolConfig::default(),
            capital_deployer: CapitalDeployerConfig::default(),
            betting: BettingConfig::default(),
        }
    }

    #[test]
    fn test_config_validate_no_chains_enabled() {
        let cfg = default_config();
        let errors = cfg.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.to_lowercase().contains("at least one chain")));
    }

    #[test]
    fn test_config_validate_one_chain_enabled() {
        let mut cfg = default_config();
        cfg.ethereum = EthereumConfig {
            enabled: true,
            node_url: "https://infura.io".to_string(),
            relayer_private_key: "0xABCD".to_string(),
            ..Default::default()
        };
        let errors = cfg.validate();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_config_is_ethereum_enabled() {
        let mut cfg = default_config();
        assert!(!cfg.is_ethereum_enabled());

        cfg.ethereum.enabled = true;
        cfg.ethereum.node_url = "https://infura.io".to_string();
        cfg.ethereum.relayer_private_key = "0xABCD".to_string();
        assert!(cfg.is_ethereum_enabled());
    }

    #[test]
    fn test_config_is_monero_enabled() {
        let mut cfg = default_config();
        assert!(!cfg.is_monero_enabled());

        cfg.monero.enabled = true;
        cfg.monero.wallet_rpc_url = "http://url".to_string();
        cfg.monero.node_rpc_url = "http://url".to_string();
        cfg.monero.fee_address = "addr".to_string();
        assert!(cfg.is_monero_enabled());
    }

    #[test]
    fn test_config_is_zcash_enabled() {
        let mut cfg = default_config();
        assert!(!cfg.is_zcash_enabled());

        cfg.zcash.enabled = true;
        cfg.zcash.node_rpc_url = "http://url".to_string();
        assert!(cfg.is_zcash_enabled());
    }

    #[test]
    fn test_config_is_litecoin_enabled() {
        let mut cfg = default_config();
        assert!(!cfg.is_litecoin_enabled());

        cfg.litecoin.enabled = true;
        cfg.litecoin.node_rpc_url = "http://url".to_string();
        cfg.litecoin.rpc_user = "user".to_string();
        assert!(cfg.is_litecoin_enabled());
    }

    #[test]
    fn test_config_is_aztec_enabled() {
        let mut cfg = default_config();
        assert!(!cfg.is_aztec_enabled());

        cfg.aztec.enabled = true;
        cfg.aztec.rollup_address = "0xRollup".to_string();
        cfg.aztec.sequencer_url = "https://aztec".to_string();
        assert!(cfg.is_aztec_enabled());
    }
}