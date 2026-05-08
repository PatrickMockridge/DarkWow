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

//! Aztec Relayer Service
//!
//! Aztec bridge relayer that:
//! 1. Monitors Aztec rollup for deposits to bridge addresses
//! 2. Constructs ZK proofs for deposit verification
//! 3. Submits deposits to DarkWow bridge contract
//!
//! "Private DAI and ETH - Aztec's private DeFi made portable"

use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use smol::Timer;
use structopt::StructOpt;
use tracing::{error, info, warn};

mod aztec_rpc;
mod proof;
mod withdrawal;

use aztec_rpc::AztecRpcClient;

const CONFIG_FILE: &str = "aztec_relayer_config.toml";

#[derive(Debug, Clone, Deserialize)]
struct Config {
    darkfid_url: String,
    ethereum_node_url: String,
    aztec_rollup_address: String,
    view_key: String,
    min_deposit_eth: u64,
    min_deposit_dai: u64,
    confirmations: u64,
    fee: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            darkfid_url: "http://127.0.0.1:8543".to_string(),
            ethereum_node_url: "http://127.0.0.1:8545".to_string(),
            aztec_rollup_address: String::new(),
            view_key: String::new(),
            min_deposit_eth: 1_000_000_000_000_000, // 0.001 ETH
            min_deposit_dai: 1_000_000_000_000_000, // 0.001 DAI (in wei)
            confirmations: 5,
            fee: 1_000_000_000_000_000, // 0.001 ETH equivalent
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

    info!(target: "aztec_relayer", "Starting Aztec Relayer Service...");
    info!(target: "aztec_relayer", "DarkFi network: {}", args.network);
    info!(target: "aztec_relayer", "Ethereum node: {}", config.ethereum_node_url);
    info!(target: "aztec_relayer", "Aztec rollup: {}", config.aztec_rollup_address);
    info!(target: "aztec_relayer", "Min deposit (ETH): {} wei", config.min_deposit_eth);
    info!(target: "aztec_relayer", "Min deposit (DAI): {} wei", config.min_deposit_dai);
    info!(target: "aztec_relayer", "Required confirmations: {}", config.confirmations);

    let aztec_client = AztecRpcClient::new(
        &config.ethereum_node_url,
        &config.aztec_rollup_address,
        &config.view_key,
    )?;

    info!(target: "aztec_relayer", "Connected to Ethereum node");

    let mut last_scanned_rollup = aztec_client.get_current_rollup_height().await?;
    info!(target: "aztec_relayer", "Starting scan from rollup height: {}", last_scanned_rollup);

    loop {
        match aztec_client.scan_for_notes(last_scanned_rollup).await {
            Ok(notes) => {
                for note in &notes {
                    let min_deposit = match note.asset_id {
                        0 => config.min_deposit_eth,  // ETH
                        1 => config.min_deposit_dai,  // DAI
                        _ => {
                            warn!(target: "aztec_relayer", "Unknown asset_id {}, skipping", note.asset_id);
                            continue;
                        }
                    };

                    info!(
                        target: "aztec_relayer",
                        "Found deposit: {} (asset_id={}) at rollup {}",
                        note.value, note.asset_id, note.rollup_height
                    );

                    if note.value < min_deposit {
                        warn!(
                            target: "aztec_relayer",
                            "Deposit {} below minimum, skipping",
                            note.value
                        );
                        continue;
                    }

                    match proof::submit_deposit(note, &config).await {
                        Ok(()) => {
                            info!(target: "aztec_relayer", "Deposit relayed successfully");
                        }
                        Err(e) => {
                            error!(target: "aztec_relayer", "Failed to relay deposit: {}", e);
                        }
                    }
                }

                if let Some(height) = notes.last().map(|n| n.rollup_height) {
                    last_scanned_rollup = height + 1;
                }
            }
            Err(e) => {
                error!(target: "aztec_relayer", "Scan error: {}", e);
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

    // For Aztec, we derive a private note
    // The full derivation would use Aztec's note encryption scheme
    // For MVP, we use a simplified derivation
    let mut addr_hasher = blake3::Hasher::new();
    addr_hasher.update(b"aztec_bridge_addr");
    addr_hasher.update(&bridge_secret);
    let _addr_hash = *addr_hasher.finalize().as_bytes();

    println!("Bridge Aztec Address: (private note)");
    println!("Secret (hex): {}", hex::encode(bridge_secret));
    println!("NOTE: This is an Aztec private note for receiving ETH/DAI deposits.");
    println!("Your deposit amount and identity remain private on Aztec rollup.");

    Ok(())
}

fn status() -> Result<()> {
    println!("Aztec Relayer Status:");
    println!("  Network: testnet (placeholder)");
    println!("  Status: Running (placeholder)");
    println!();
    println!("Note: The Aztec bridge supports ETH and DAI deposits.");
    println!("Your funds are private in Aztec's rollup - only you can reveal them.");
    println!("The relayer can only observe deposits, never steal or link funds.");
    Ok(())
}

fn expand_path(path: &str) -> Result<String> {
    let expanded = shellexpand::env(path)
        .map_err(|_| anyhow::anyhow!("Invalid path"))?
        .into_owned();
    Ok(expanded)
}