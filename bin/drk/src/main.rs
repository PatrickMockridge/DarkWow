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

use std::{process::ExitCode, sync::Arc};

use dwow_wallet::wallet_error::{Error, Result};
use dwow_wallet::args::WalletCommand;
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

    // 4. Try daemon RPC socket (Python: _try_connect_daemon)
    let daemon_available = dwow_wallet::wallet_rpc_client::WalletRpcClient::try_connect(
        &config.network
    ).is_some();

    // 5. Classify DB dependency (Python: _spec_classify_db_dependency)
    let db_dep = dispatch::classify_db_dependency(&args.command);

    // 6. Route: RPC-first for sled-backed commands when daemon is reachable
    if daemon_available && db_dep == dispatch::DbDependency::NeedsSled {
        println!("Daemon is running on /tmp/drk-{}.sock — use RPC for this command.", config.network);
        println!("RPC dispatch not yet implemented for this command.");
        return Err(Error::Custom("RPC dispatch not yet implemented".into()));
    }

    // 7. Open wallet — full (sled+SQLite) or local (SQLite only)
    //    Python: open_full() for NEEDS_SLED, open_local() for SQLITE_ONLY
    if db_dep == dispatch::DbDependency::NeedsSled {
        let dww = dispatch::open_wallet(&config)?;
        let dww_ptr: DwwPtr = dww.into_ptr();

        match dispatch::classify(&args.command) {
            dispatch::CommandCategory::Network => {
                smol::block_on(dispatch::dispatch_async(&dww_ptr, &args.command))
            }
            _ => {
                let dww = smol::block_on(dww_ptr.read());
                dispatch::dispatch_sync(&dww, &args.command)
            }
        }
    } else {
        // SQLite-only or pure — no sled needed
        let wallet = dwow_wallet::local_wallet::LocalWallet::open(
            &config.wallet_path, &config.wallet_pass
        )?;
        // Dispatch locally — these commands only need SQLite
        dispatch::dispatch_local(&wallet, &args.command)
    }
}
