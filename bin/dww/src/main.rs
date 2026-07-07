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

use std::process::ExitCode;

use dwow_wallet::wallet_error::Result;
use dwow_wallet::args::{WalletCommand, WalletSubcmd};
use dwow_wallet::{args, config, dispatch, DwwPtr};

/// Config file name — used by config module.
pub const CONFIG_FILE: &str = "dww_config.toml";
/// Default config contents — embedded at compile time.
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../dww_config.toml");

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    // Install rustls crypto provider before any TLS operations.
    // rustls 0.23 requires explicit provider selection; without this,
    // any TLS handshake (P2P seed connection) panics with:
    // "Could not automatically determine the process-level CryptoProvider"
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls ring crypto provider");

    // 1. Parse args — sync, returns Result, never calls exit()
    let args = args::parse_args(std::env::args())?;

    // 2. Handle help/version before config — matches Python spec
    //    Must precede load_config: help/version must work without any filesystem dependency
    match &args.command {
        WalletCommand::Help { topic } => {
            dispatch::print_help(topic.as_deref());
            return Ok(());
        }
        WalletCommand::Version => {
            dispatch::print_version();
            return Ok(());
        }
        _ => {}
    }

    // 3. Load config — sync, std::fs, no derive magic
    let config = config::load_config(&args)?;

    // 4. Classify command (Python: _spec_classify + _spec_classify_db_dependency)
    let (category, db_dep) = dispatch::classify(&args.command);

    // 5. Open wallet — full (sled+SQLite) or local (SQLite only).
    //    RPC-first: if the daemon is running (Unix socket exists), route
    //    sled-backed commands through the daemon's RPC to avoid sled lock
    //    contention. This matches the Python spec (wallet_model.py:6909-6919).
    //    The daemon owns sled exclusively; CLI processes never open sled
    //    directly when the daemon is reachable.
    if db_dep == dispatch::DbDependency::NeedsSled {
        // Try daemon RPC first — avoids WouldBlock from daemon's sled lock.
        // BUT: wallet initialize must open sled directly — no daemon exists yet.
        let is_init = matches!(&args.command,
            WalletCommand::Wallet { command: WalletSubcmd::Initialize });
        if !is_init {
            if let Some(rpc) = dwow_wallet::wallet_rpc_client::WalletRpcClient::try_connect(
                &config.network
            ) {
                return dwow_wallet::dispatch::rpc_dispatch(&rpc, &args.command);
            }
        }

        // Daemon not reachable — open sled directly (standalone mode)
        let dww = dispatch::open_wallet(&config)?;
        let dww_ptr: DwwPtr = dww.into_ptr();

        match category {
            dispatch::CommandCategory::Network => {
                // Create executor and run it on background threads — same
                // pattern as mining node's async_daemonize! (src/util/cli.rs:218-229).
                // P2P session tasks (seed slots, outbound slots, protocol handlers)
                // spawn on this executor. Without it, they never execute.
                let ex = std::sync::Arc::new(smol::Executor::new());
                let n_threads = 2;
                let (signal, shutdown) = smol::channel::unbounded::<()>();
                for _ in 0..n_threads {
                    let ex = ex.clone();
                    let shutdown = shutdown.clone();
                    std::thread::spawn(move || {
                        let _ = smol::future::block_on(ex.run(shutdown.recv()));
                    });
                }
                let result = smol::block_on(
                    dispatch::dispatch_async(&dww_ptr, &args.command, ex.clone())
                );
                drop(signal);
                result
            }
            _ => {
                let dww = smol::block_on(dww_ptr.read());
                dispatch::dispatch_sync(&dww, &args.command)
            }
        }
    } else {
        // SQLite-only or pure — no sled needed.
        // Open SQLite directly via LocalWallet for commands the daemon
        // would otherwise block with its exclusive sled lock.
        let network = match config.network.as_str() {
            "mainnet" | "localnet" => dwow_sdk::crypto::keypair::Network::Mainnet,
            _ => dwow_sdk::crypto::keypair::Network::Testnet,
        };
        let keys_toml = config.keys_toml.as_ref().map(std::path::Path::new);
        let section = config.section.as_deref().ok_or_else(|| {
            dwow_wallet::wallet_error::Error::Custom(
                "WALLET_NAME not set — the wallet must declare which keys.toml section is its identity".into())
        })?;
        let wallet = dwow_wallet::local_wallet::LocalWallet::open(
            &config.wallet_path, &config.wallet_pass, keys_toml, network, section,
        )?;
        // Inline dispatch — LocalWallet supports address, addresses,
        // balance, secrets, capabilities. Other SqliteOnly commands
        // (help, version) are handled before config loading.
        match &args.command {
            WalletCommand::Wallet { command: WalletSubcmd::Address } => {
                println!("{}", wallet.default_address()?);
                Ok(())
            }
            WalletCommand::Wallet { command: WalletSubcmd::Addresses } => {
                for addr in wallet.addresses()? {
                    println!("{addr}");
                }
                Ok(())
            }
            WalletCommand::Wallet { command: WalletSubcmd::Balance } => {
                let balances = wallet.capability_balance()?;
                if balances.is_empty() {
                    println!("No retained balances found");
                } else {
                    for (token, amount) in &balances {
                        println!("{token} {amount}");
                    }
                }
                Ok(())
            }
            WalletCommand::Wallet { command: WalletSubcmd::Secrets } => {
                for secret in wallet.secrets()? {
                    println!("{secret}");
                }
                Ok(())
            }
            WalletCommand::Wallet { command: WalletSubcmd::Capabilities } => {
                for cap in wallet.capabilities()? {
                    println!("{} value={} token={}", cap.cap_id, cap.value, cap.token_id);
                }
                Ok(())
            }
            // Commands classified as SqliteOnly that LocalWallet doesn't
            // support (default-address, tree, contract show). These need the
            // full Dww — open it.
            _ => {
                let dww = dispatch::open_wallet(&config)?;
                dispatch::dispatch_sync(&dww, &args.command)
            }
        }
    }
}
