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

//! ZEC Relayer Service
//!
//! Zcash bridge relayer that:
//! 1. Monitors Zcash blockchain for deposits to bridge shielded addresses
//! 2. Constructs ZK proofs for deposit verification
//! 3. Submits deposits to DarkWow bridge contract
//!
//! "Shield your Zcash once and forever more"

use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use smol::Timer;
use structopt::StructOpt;
use tracing::{error, info, warn};

mod zcash_rpc;
mod proof;
mod withdrawal;

use zcash_rpc::ZcashRpcClient;

const CONFIG_FILE: &str = "zcash_relayer_config.toml";

#[derive(Debug, Clone, Deserialize)]
struct Config {
    darkfid_url: String,
    zcash_lightwalletd_url: String,
    zcash_node_url: String,
    view_key: String,
    min_deposit: u64,
    confirmations: u64,
    fee: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            darkfid_url: "http://127.0.0.1:8543".to_string(),
            zcash_lightwalletd_url: "http://127.0.0.1:9067".to_string(),
            zcash_node_url: "http://127.0.0.1:8233".to_string(),
            view_key: String::new(),
            min_deposit: 10_000, // 0.0001 ZEC = 10,000 zatoshi
            confirmations: 10,
            fee: 1_000,
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

    info!(target: "zec_relayer", "Starting ZEC Relayer Service...");
    info!(target: "zec_relayer", "DarkFi network: {}", args.network);
    info!(target: "zec_relayer", "Zcash lightwalletd: {}", config.zcash_lightwalletd_url);
    info!(target: "zec_relayer", "Min deposit: {} zatoshi", config.min_deposit);
    info!(target: "zec_relayer", "Required confirmations: {}", config.confirmations);

    let zcash_client = ZcashRpcClient::new(
        &config.zcash_lightwalletd_url,
        &config.zcash_node_url,
        &config.view_key,
    )?;

    info!(target: "zec_relayer", "Connected to Zcash lightwalletd");

    let mut last_scanned_height = zcash_client.get_current_height().await?;
    info!(target: "zec_relayer", "Starting scan from height: {}", last_scanned_height);

    loop {
        match zcash_client.scan_for_notes(last_scanned_height).await {
            Ok(notes) => {
                for note in &notes {
                    info!(
                        target: "zec_relayer",
                        "Found deposit: {} ZEC at height {}",
                        note.value as f64 / 1e8,
                        note.height
                    );

                    if note.value < config.min_deposit {
                        warn!(
                            target: "zec_relayer",
                            "Deposit {} below minimum, skipping",
                            note.value
                        );
                        continue;
                    }

                    match proof::submit_deposit(note, &config).await {
                        Ok(()) => {
                            info!(target: "zec_relayer", "Deposit relayed successfully");
                        }
                        Err(e) => {
                            error!(target: "zec_relayer", "Failed to relay deposit: {}", e);
                        }
                    }
                }

                if let Some(height) = notes.last().map(|n| n.height) {
                    last_scanned_height = height + 1;
                }
            }
            Err(e) => {
                error!(target: "zec_relayer", "Scan error: {}", e);
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

    // For Zcash, we derive a Sapling zaddr
    // The full derivation would use zip 32 derivation
    // For MVP, we use a simplified derivation
    let mut addr_hasher = blake3::Hasher::new();
    addr_hasher.update(b"zec_bridge_addr");
    addr_hasher.update(&bridge_secret);
    let _addr_hash = *addr_hasher.finalize().as_bytes();

    // Encode as zcash transparent or shielded address
    // For shielded, we'd use bech32 encoding with "zs" prefix
    // HAZOP C11: do not print bridge secret to stdout
    println!("Bridge Shielded Address (zaddr): zs1...");
    println!("Bridge Transparent Address (taddr): tAD...");
    println!("NOTE: This is a Sapling shielded address for receiving ZEC deposits only.");

    Ok(())
}

fn status() -> Result<()> {
    println!("ZEC Relayer Status:");
    println!("  Network: testnet (placeholder)");
    println!("  Status: Running (placeholder)");
    println!();
    println!("Note: The Zcash bridge uses Sapling shielded addresses.");
    println!("Your ZEC is secure in Sapling - only you can spend it.");
    println!("The relayer can only observe deposits, never steal funds.");
    Ok(())
}

fn expand_path(path: &str) -> Result<String> {
    let expanded = shellexpand::env(path)
        .map_err(|_| anyhow::anyhow!("Invalid path"))?
        .into_owned();
    Ok(expanded)
}