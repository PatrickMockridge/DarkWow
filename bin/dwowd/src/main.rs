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

use std::sync::Arc;

use smol::{fs::read_to_string, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::{debug, error, info};

use dwow_core::{
    async_daemonize,
    cli_desc,
    net::settings::SettingsOpt,
    rpc::settings::{RpcSettings, RpcSettingsOpt},
    util::path::{expand_path, get_config_path},
    Error, Result,
};
use dwow_sdk::crypto::keypair::Network;

use dwowd::{task::ConsensusInitTaskConfig, Dwowd};

const CONFIG_FILE: &str = "dwowd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dwowd_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "dwowd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Path to keys.toml — declared mining keys (default: auto-generate on localnet)
    keys: Option<String>,

    #[structopt(short, long, default_value = "darkwow-devnet")]
    /// Blockchain network to use
    network: String,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(long)]
    /// Finality enforcement mode: "always" (default), "native", or "signaled".
    /// Overrides the [finality] TOML section if set.
    /// - always: Anchor every mined block to Arweave and enforce anchors on received blocks
    /// - native: Trust PoW only — ignore all anchors
    /// - signaled: Only enforce finality when a block signals it requires it
    finality_mode: Option<String>,

    #[structopt(long)]
    /// Disable Caribina Arweave anchoring entirely
    finality_disable_caribina: bool,

    #[structopt(long)]
    /// Enable Monero p2pool anchoring (default: disabled)
    finality_enable_monero: bool,

    #[structopt(long)]
    /// Monero minimum confirmations before finality (default: 3)
    monero_min_confirmations: Option<u32>,

    #[structopt(long)]
    /// monerod JSON-RPC URL for anchor verification (e.g. http://127.0.0.1:18081/json_rpc)
    monerod_rpc_url: Option<String>,

    #[structopt(long)]
    /// Export the default secret key as base58 and exit (for key backup or
    /// sharing with a wallet). Goes through AccountManager — the single key
    /// authority. Does not start the daemon.
    export_secret: bool,
}

#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
/// Defines a blockchain network configuration.
/// Default values correspond to a local network.
pub struct BlockchainNetwork {
    #[structopt(long, default_value = "~/.local/share/dwow/dwowd/darkwow-devnet")]
    /// Path to blockchain database
    database: String,

    #[structopt(long)]
    /// Skip syncing process and start node right away
    skip_sync: bool,

    #[structopt(long)]
    /// Create the genesis block. Only one node per network should have this set.
    /// Other nodes start at height 0 and sync genesis via P2P.
    create_genesis: bool,

    #[structopt(long)]
    /// Optional sync checkpoint height
    checkpoint_height: Option<u32>,

    #[structopt(long)]
    /// Optional sync checkpoint hash
    checkpoint: Option<String>,

    #[structopt(flatten)]
    /// P2P network settings
    net: SettingsOpt,

    #[structopt(flatten)]
    /// Main server JSON-RPC settings
    rpc: RpcSettingsOpt,

    #[structopt(skip)]
    /// Management server JSON-RPC settings (not used in darkwow-devnet)
    management_rpc: Option<RpcSettingsOpt>,

    #[structopt(skip)]
    /// Stratum server JSON-RPC settings (optional)
    stratum_rpc: Option<RpcSettingsOpt>,

    #[structopt(skip)]
    /// Merge mining server JSON-RPC settings (optional)
    mm_rpc: Option<RpcSettingsOpt>,

    #[structopt(skip)]
    /// Finality configuration (parsed from TOML, overridden by --finality-mode CLI flag)
    finality: Option<dwow_chain::FinalityConfig>,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "dwowd", "Initializing DarkWow node...");

    // Grab blockchain network configuration
    let (network, mut blockchain_config) = match args.network.as_str() {
        "darkwow-devnet" | "darkwow-testnet" => {
            parse_blockchain_config(args.config, args.network.as_str()).await?
        }
        _ => {
            error!("Unsupported chain `{}`", args.network);
            return Err(Error::UnsupportedChain)
        }
    };

    // Apply finality CLI overrides
    if args.finality_mode.is_some()
        || args.finality_disable_caribina
        || args.finality_enable_monero
        || args.monero_min_confirmations.is_some()
        || args.monerod_rpc_url.is_some()
    {
        let mut fc = blockchain_config.finality.unwrap_or_default();

        if let Some(ref mode_str) = args.finality_mode {
            fc.mode = match mode_str.as_str() {
                "native" => dwow_chain::FinalityMode::Native,
                "always" => dwow_chain::FinalityMode::Always,
                "signaled" => dwow_chain::FinalityMode::Signaled,
                other => {
                    error!(target: "dwowd", "Invalid finality mode: {other}. Must be one of: native, always, signaled");
                    return Err(Error::ParseFailed("Invalid finality mode"))
                }
            };
            info!(target: "dwowd", "Finality mode set via CLI: {mode_str}");
        }

        if args.finality_disable_caribina {
            fc.caribina_enabled = false;
            info!(target: "dwowd", "Caribina anchoring disabled via CLI");
        }

        if args.finality_enable_monero {
            fc.monero_enabled = true;
            info!(target: "dwowd", "Monero anchoring enabled via CLI");
        }

        if let Some(confirmations) = args.monero_min_confirmations {
            fc.monero_min_confirmations = confirmations;
            info!(target: "dwowd", "Monero min confirmations set via CLI: {confirmations}");
        }

        if let Some(ref url) = args.monerod_rpc_url {
            fc.monerod_url = Some(url.clone());
            info!(target: "dwowd", "Monero RPC URL set via CLI: {url}");
        }

        blockchain_config.finality = Some(fc);
    }

    info!(target: "dwowd", "Starting DarkWow node...");

    // Initialize or open sled database
    let db_path = expand_path(&blockchain_config.database)?;
    let sled_db = sled::Config::new()
        .path(&db_path)
        .cache_capacity(256 * 1024 * 1024) // 256MB (default 1GB was excessive)
        .open()?;

    // Resolve keys.toml path from --keys CLI flag
    let keys_path: Option<std::path::PathBuf> = args.keys.as_ref().map(|p| {
        let path = std::path::PathBuf::from(p);
        if path.is_relative() {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join(path)
        } else {
            path
        }
    });

    // --export-secret: print the default secret key as base58 and exit.
    // Goes through AccountManager — the single key authority. Reads from sled
    // cache on restart, falls back to keys.toml or auto-gen on first use.
    if args.export_secret {
        let accounts_tree = sled_db.open_tree("accounts")
            .expect("sled open_tree accounts");
        let cached_json = accounts_tree.get("accounts_json")
            .ok().flatten()
            .map(|v| String::from_utf8(v.to_vec()).expect("utf8"))
            .unwrap_or_default();
        let cached = if cached_json.is_empty() { None } else { Some(cached_json.as_str()) };
        let mgr = match dwow_accounts::AccountManager::open(
            cached,
            true, // localnet — allows auto-gen fallback if no keys.toml
            keys_path.as_deref(),
            network,
            None, // section_name=None → uses NODE_NAME env var
        ) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("export-secret: AccountManager::open failed: {e}");
                std::process::exit(1);
            }
        };
        let idx = mgr.default_index();
        let b58 = match mgr.export_base58(idx) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("export-secret: {e}");
                std::process::exit(1);
            }
        };
        // Loud diagnostic on stderr — key identity for verification.
        // The base58 secret key on stdout is the pipe-able output.
        let pk_hex = match mgr.default_public_key() {
            Ok(pk) => hex::encode(pk.to_bytes()),
            Err(_) => "unknown".to_string(),
        };
        eprintln!("export-secret: account[{}] secrets={} public={}",
            idx, mgr.secrets().len(), pk_hex);
        println!("{b58}");
        std::process::exit(0);
    }

    // Setup P2P settings
    let p2p_settings: dwow_core::net::Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), blockchain_config.net).try_into()?;

    // Initialize the daemon using LinearBlockchain
    let mining_enabled = std::env::var("MINING_ENABLED")
        .map(|v| v.to_lowercase() != "false")
        .unwrap_or(true);  // default: mining ON

    // Guard: observer nodes (MINING_ENABLED=false) MUST NOT create genesis.
    // Only the designated genesis authority creates block 1. Observers are
    // sync-only — they download genesis from peers. Misconfiguration here
    // would create a permanent chain split.
    if !mining_enabled && blockchain_config.create_genesis {
        panic!(
            "FATAL: Observer node (MINING_ENABLED=false) must not have CREATE_GENESIS=true. \
             Observers sync from peers. Set CREATE_GENESIS=false in dwowd_config.toml."
        );
    }

    let daemon = Dwowd::init_linear(
        network,
        &sled_db,
        &db_path,
        &p2p_settings,
        &ex,
        blockchain_config.finality,
        blockchain_config.create_genesis,
        keys_path.as_deref(),
        mining_enabled,
    )
    .await?;

    // Start the daemon with consensus config
    let config = ConsensusInitTaskConfig {
        skip_sync: blockchain_config.skip_sync,
        checkpoint_height: blockchain_config.checkpoint_height,
        checkpoint: blockchain_config.checkpoint,
    };
    daemon
        .start(
            &ex,
            &blockchain_config.rpc.into(),
            &RpcSettings::default(),
            &blockchain_config.stratum_rpc.map(|stratum_rpc_opts| stratum_rpc_opts.into()),
            &blockchain_config.mm_rpc.map(|mm_rpc_opts| mm_rpc_opts.into()),
            &config,
        )
        .await?;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "dwowd", "Caught termination signal, cleaning up and exiting...");

    daemon.stop().await?;

    info!(target: "dwowd", "Shut down successfully");

    Ok(())
}

/// Auxiliary function to parse dwowd configuration file and extract requested
/// blockchain network config.
pub async fn parse_blockchain_config(
    config: Option<String>,
    network: &str,
) -> Result<(Network, BlockchainNetwork)> {
    // Grab network prefix
    let used_net = match network {
        "darkwow-devnet" | "darkwow-testnet" => Network::Testnet,
        _ => return Err(Error::ParseFailed("Invalid blockchain network")),
    };

    // Grab config path
    let config_path = get_config_path(config, CONFIG_FILE)?;
    debug!(target: "dwowd", "Parsing configuration file: {config_path:?}");

    // Parse TOML file contents
    let contents = read_to_string(&config_path).await?;
    let contents: toml::Value = match toml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "dwowd", "Failed parsing TOML config: {e}");
            return Err(Error::ParseFailed("Failed parsing TOML config"))
        }
    };

    // Grab requested network config
    let Some(table) = contents.as_table() else { return Err(Error::ParseFailed("TOML not a map")) };
    let Some(network_configs) = table.get("network_config") else {
        return Err(Error::ParseFailed("TOML does not contain network configurations"))
    };
    let Some(network_configs) = network_configs.as_table() else {
        return Err(Error::ParseFailed("`network_config` not a map"))
    };
    let Some(network_config) = network_configs.get(network) else {
        return Err(Error::ParseFailed("TOML does not contain requested network configuration"))
    };
    // Parse optional [finality] subsection from the network config
    let finality_config: Option<dwow_chain::FinalityConfig> =
        if let Some(finality_section) = network_config.get("finality") {
            match finality_section.clone().try_into() {
                Ok(fc) => Some(fc),
                Err(e) => {
                    error!(target: "dwowd", "Failed parsing finality config: {e}");
                    return Err(Error::ParseFailed("Failed parsing finality config"))
                }
            }
        } else {
            None
        };

    let network_config_str = toml::to_string(&network_config).unwrap();
    let mut network_config =
        match BlockchainNetwork::from_iter_with_toml::<Vec<String>>(&network_config_str, vec![]) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "dwowd", "Failed parsing requested network configuration: {e}");
                return Err(Error::ParseFailed("Failed parsing requested network configuration"))
            }
        };
    network_config.finality = finality_config;
    debug!(target: "dwowd", "Parsed network configuration: {network_config:?}");

    Ok((used_net, network_config))
}
