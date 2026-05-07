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

//! XMR Relayer Service
//!
//! Monero bridge relayer that:
//! 1. Monitors Monero blockchain for deposits to bridge one-time addresses
//! 2. Constructs ZK proofs for deposit verification
//! 3. Submits deposits to DarkWow bridge contract

use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use smol::Timer;
use structopt::StructOpt;
use tracing::{error, info, warn};

mod monero_rpc;
mod proof;
mod withdrawal;

use monero_rpc::MoneroRpcClient;

const CONFIG_FILE: &str = "xmr_relayer_config.toml";

#[derive(Debug, Clone, Deserialize)]
struct Config {
    darkfid_url: String,
    monero_wallet_url: String,
    monero_node_url: String,
    view_key: String,
    min_deposit: u64,
    confirmations: u64,
    fee: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            darkfid_url: "http://127.0.0.1:8543".to_string(),
            monero_wallet_url: "http://127.0.0.1:18083".to_string(),
            monero_node_url: "http://127.0.0.1:18081".to_string(),
            view_key: String::new(),
            min_deposit: 1_000_000_000,
            confirmations: 10,
            fee: 1_000_000,
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

    info!(target: "xmr_relayer", "Starting XMR Relayer Service...");
    info!(target: "xmr_relayer", "DarkFi network: {}", args.network);
    info!(target: "xmr_relayer", "Monero wallet: {}", config.monero_wallet_url);
    info!(target: "xmr_relayer", "Min deposit: {} piconero", config.min_deposit);
    info!(target: "xmr_relayer", "Required confirmations: {}", config.confirmations);

    let monero_client = MoneroRpcClient::new(
        &config.monero_wallet_url,
        &config.monero_node_url,
        &config.view_key,
    )?;

    info!(target: "xmr_relayer", "Connected to Monero node");

    let mut last_scanned_height = monero_client.get_current_height().await?;
    info!(target: "xmr_relayer", "Starting scan from height: {}", last_scanned_height);

    loop {
        match monero_client.scan_for_transfers(last_scanned_height).await {
            Ok(transfers) => {
                for transfer in &transfers {
                    info!(
                        target: "xmr_relayer",
                        "Found deposit: {} XMR at height {}",
                        transfer.amount as f64 / 1e12,
                        transfer.height
                    );

                    if transfer.amount < config.min_deposit {
                        warn!(
                            target: "xmr_relayer",
                            "Deposit {} below minimum, skipping",
                            transfer.amount
                        );
                        continue;
                    }

                    match proof::submit_deposit(transfer, &config).await {
                        Ok(()) => {
                            info!(target: "xmr_relayer", "Deposit relayed successfully");
                        }
                        Err(e) => {
                            error!(target: "xmr_relayer", "Failed to relay deposit: {}", e);
                        }
                    }
                }

                if let Some(height) = transfers.last().map(|t| t.height) {
                    last_scanned_height = height + 1;
                }
            }
            Err(e) => {
                error!(target: "xmr_relayer", "Scan error: {}", e);
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

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"bridge_secret");
    hasher.update(&pub_x_bytes);
    hasher.update(&pub_y_bytes);
    hasher.update(&nonce.to_le_bytes());
    let bridge_secret = *hasher.finalize().as_bytes();

    let mut addr_hasher = blake3::Hasher::new();
    addr_hasher.update(b"one_time_addr");
    addr_hasher.update(&bridge_secret);
    let one_time_addr = bs58::encode(addr_hasher.finalize().as_bytes()).into_string();

    println!("Bridge One-Time Address: {}", one_time_addr);
    println!("Secret (hex): {}", hex::encode(bridge_secret));
    println!("NOTE: This address is for receiving XMR deposits only.");

    Ok(())
}

fn status() -> Result<()> {
    println!("XMR Relayer Status:");
    println!("  Network: testnet (placeholder)");
    println!("  Status: Running (placeholder)");
    Ok(())
}

fn expand_path(path: &str) -> Result<String> {
    let expanded = shellexpand::env(path)
        .map_err(|_| anyhow::anyhow!("Invalid path"))?
        .into_owned();
    Ok(expanded)
}