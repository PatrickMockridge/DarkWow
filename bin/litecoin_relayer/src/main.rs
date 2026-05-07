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

//! Litecoin Relayer Service
//!
//! Litecoin bridge relayer that:
//! 1. Monitors Litecoin blockchain for deposits to bridge addresses
//! 2. Constructs ZK proofs for deposit verification
//! 3. Submits deposits to DarkWow bridge contract
//!
//! "The Monero trade pair - move in and out of privacy with LTC"

use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use smol::Timer;
use structopt::StructOpt;
use tracing::{error, info, warn};

mod litecoin_rpc;
mod proof;
mod withdrawal;

use litecoin_rpc::LitecoinRpcClient;

const CONFIG_FILE: &str = "litecoin_relayer_config.toml";

#[derive(Debug, Clone, Deserialize)]
struct Config {
    darkfid_url: String,
    litecoin_rpc_url: String,
    litecoin_rpc_user: String,
    litecoin_rpc_pass: String,
    min_deposit: u64,
    confirmations: u64,
    fee: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            darkfid_url: "http://127.0.0.1:8543".to_string(),
            litecoin_rpc_url: "http://127.0.0.1:9332".to_string(),
            litecoin_rpc_user: String::new(),
            litecoin_rpc_pass: String::new(),
            min_deposit: 100_000, // 0.001 LTC = 100,000 satoshis
            confirmations: 6,
            fee: 10_000, // 0.0001 LTC in satoshis
        }
    }
}

#[derive(Debug, StructOpt)]
struct Args {
    #[structopt(long)]
    config: Option<String>,
    #[structopt(long, default_value = "testnet")]
    network: String,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    Start,
    DeriveAddress {
        #[structopt(long)]
        pub_x: String,
        #[structopt(long)]
        pub_y: String,
        #[structopt(long)]
        nonce: u64,
    },
    Status,
}

fn main() {
    let args = Args::from_args();

    match args.command {
        Command::Start => {
            if let Err(e) = run_main(args) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Command::DeriveAddress { pub_x, pub_y, nonce } => {
            if let Err(e) = derive_address(pub_x, pub_y, nonce) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Command::Status => {
            if let Err(e) = status() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn run_main(args: Args) -> Result<()> {
    smol::block_on(async_main(args))
}

async fn async_main(args: Args) -> Result<()> {
    let config_path = match args.config {
        Some(p) => p,
        None => expand_path(CONFIG_FILE)?,
    };

    let config: Config = if std::path::Path::new(&config_path).exists() {
        let contents = std::fs::read_to_string(&config_path)?;
        toml::from_str(&contents)?
    } else {
        Config::default()
    };

    info!(target: "ltc_relayer", "Starting Litecoin Relayer Service...");
    info!(target: "ltc_relayer", "DarkFi network: {}", args.network);
    info!(target: "ltc_relayer", "Litecoin RPC: {}", config.litecoin_rpc_url);
    info!(target: "ltc_relayer", "Min deposit: {} satoshis", config.min_deposit);
    info!(target: "ltc_relayer", "Required confirmations: {}", config.confirmations);

    let litecoin_client = LitecoinRpcClient::new(
        &config.litecoin_rpc_url,
        &config.litecoin_rpc_user,
        &config.litecoin_rpc_pass,
    )?;

    info!(target: "ltc_relayer", "Connected to Litecoin node");

    let mut last_scanned_height = litecoin_client.get_current_height().await?;
    info!(target: "ltc_relayer", "Starting scan from height: {}", last_scanned_height);

    loop {
        match litecoin_client.scan_for_deposits(last_scanned_height).await {
            Ok(deposits) => {
                for deposit in &deposits {
                    info!(
                        target: "ltc_relayer",
                        "Found deposit: {} LTC at height {}",
                        deposit.amount as f64 / 1e8,
                        deposit.block_height
                    );

                    if deposit.amount < config.min_deposit {
                        warn!(
                            target: "ltc_relayer",
                            "Deposit {} below minimum, skipping",
                            deposit.amount
                        );
                        continue;
                    }

                    match proof::submit_deposit(deposit, &config).await {
                        Ok(()) => {
                            info!(target: "ltc_relayer", "Deposit relayed successfully");
                        }
                        Err(e) => {
                            error!(target: "ltc_relayer", "Failed to relay deposit: {}", e);
                        }
                    }
                }

                if let Some(height) = deposits.last().map(|d| d.block_height) {
                    last_scanned_height = height + 1;
                }
            }
            Err(e) => {
                error!(target: "ltc_relayer", "Scan error: {}", e);
                Timer::after(Duration::from_secs(5)).await;
            }
        }

        Timer::after(Duration::from_secs(30)).await;
    }
}

fn derive_address(pub_x: String, pub_y: String, nonce: u64) -> Result<()> {
    let pub_x_bytes: [u8; 32] = hex::decode(&pub_x)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid pub_x length"))?;
    let pub_y_bytes: [u8; 32] = hex::decode(&pub_y)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid pub_y length"))?;

    // Derive bridge secret using blake3
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"bridge_secret");
    hasher.update(&pub_x_bytes);
    hasher.update(&pub_y_bytes);
    hasher.update(&nonce.to_le_bytes());
    let bridge_secret = *hasher.finalize().as_bytes();

    // For Litecoin, we derive a P2PKH or P2SH address
    // The full derivation would use Litecoin address encoding
    let mut addr_hasher = blake3::Hasher::new();
    addr_hasher.update(b"ltc_bridge_addr");
    addr_hasher.update(&bridge_secret);
    let addr_hash = *addr_hasher.finalize().as_bytes();

    // Encode as Litecoin address (simplified - would use base58check)
    println!("Bridge Litecoin Address: Ltc1... (derived from {:?})", &addr_hash[..8]);
    println!("Secret (hex): {}", hex::encode(bridge_secret));
    println!("NOTE: This address is for receiving LTC deposits only.");
    println!("Litecoin: The Monero trade pair - move in and out of privacy.");

    Ok(())
}

fn status() -> Result<()> {
    println!("Litecoin Relayer Status:");
    println!("  Network: testnet (placeholder)");
    println!("  Status: Running (placeholder)");
    println!();
    println!("Note: Litecoin is the natural segueway to the Bitcoin ecosystem.");
    println!("LTC/XMR is a popular trade pair on exchanges.");
    println!("Litecoin's MWEB adds privacy when you need it.");
    Ok(())
}

fn expand_path(path: &str) -> Result<String> {
    let expanded = shellexpand::env(path)
        .map_err(|_| anyhow::anyhow!("Invalid path"))?
        .into_owned();
    Ok(expanded)
}