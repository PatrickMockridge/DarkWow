/// Subcommand dispatch — classify + route to wallet methods.
///
/// Only 5 commands need network. Everything else is synchronous.

use std::io::Read;

use crate::wallet_error::{Error, Result};

use crate::args::{
    ContractSubcmd, SyncSubcmd,
    WalletCommand, WalletSubcmd,
};
use crate::config::WalletConfig;
use crate::{Dww, DwwPtr};

/// Command classification — makes the async boundary explicit in the type system.
#[derive(Debug, PartialEq)]
pub enum CommandCategory {
    Local,
    LocalStdin,
    LocalBuild,
    Network,
}

/// Prompt user for confirmation before broadcasting a transaction.
/// On piped stdin (CI, Docker exec, scripts), auto-confirms and returns true.
/// On interactive terminals, waits for 'y' or 'yes' input.
fn confirm_broadcast() -> bool {
    use std::io::{stdin, stdout, Write};
    print!("Broadcast this transaction? [y/N]: ");
    let _ = stdout().flush();
    let mut input = String::new();
    match stdin().read_line(&mut input) {
        Ok(_) => {
            let trimmed = input.trim().to_lowercase();
            trimmed == "y" || trimmed == "yes"
        }
        Err(_) => true, // piped stdin / non-interactive: auto-confirm
    }
}

/// What database access a command needs.
/// Matches the Python spec `DbDependency` enum.
#[derive(Debug, PartialEq)]
pub enum DbDependency {
    NeedsSled,
    SqliteOnly,
    Pure,
}

/// Classify a command by async requirement AND database dependency.
/// Matches the Python spec `_spec_classify()` and `_spec_classify_db_dependency()`.
///
/// # Architectural Groups
///
/// The wallet is a generic capability engine. Native Token is the sole special
/// citizen (fee payment, coinbase rewards). Everything else goes through the
/// generic AEAD + manifest path — zero per-contract code.
///
/// ```
/// 1. Native Token path    — sole special citizen (Merkle proofs, fee payment)
///    Transfer, Redeem, Burn
///
/// 2. Generic capability   — manifest-driven (ANY contract, zero wallet changes)
///    Contract { Deploy, Invoke, Lock }
///
/// 3. Infrastructure       — network sync, P2P, daemon, bootstrap
///    Broadcast, Scan, Sync, Daemon, Wallet { Initialize, Tree }
///
/// 4. SQLite-only          — no sled (runs alongside daemon's exclusive lock)
///    Keygen, Balance, Address, Addresses, Secrets, Capabilities, ImportSecrets
/// ```
///
/// DbDependency is an explicit per-command match — no derivation rule.
pub fn classify(cmd: &WalletCommand) -> (CommandCategory, DbDependency) {
    let cat = classify_category(cmd);
    let db = match cmd {
        // ── Native Token path (sole special citizen) ──────────────────
        // Native Token is the consensus asset. These commands exercise
        // capabilities that require Merkle proofs for fee payment.
        // Per wallet.md: Native Token is the ONLY special citizen.
        WalletCommand::Transfer { .. }
        | WalletCommand::Redeem { .. }
        | WalletCommand::Burn { .. }
            => DbDependency::NeedsSled,

        // ── Generic capability path (manifest-driven) ─────────────────
        // All contracts via AEAD decrypt → manifest resolution.
        // Adding a new contract requires ZERO wallet code changes.
        // Per wallet.md + manifest.md: the manifest IS the interface.
        WalletCommand::Contract { command: ContractSubcmd::Deploy { .. } }
        | WalletCommand::Contract { command: ContractSubcmd::Invoke { .. } }
        | WalletCommand::Contract { command: ContractSubcmd::Lock { .. } }
            => DbDependency::NeedsSled,

        // ── Infrastructure ────────────────────────────────────────────
        // Network sync, P2P broadcast, daemon lifecycle.
        WalletCommand::Broadcast
        | WalletCommand::Scan { .. }
        | WalletCommand::Sync { .. }
        | WalletCommand::Daemon
            => DbDependency::NeedsSled,

        // ── Wallet bootstrap ──────────────────────────────────────────
        // Initialize creates sled trees. Tree reads Merkle proofs.
        WalletCommand::Wallet { command: WalletSubcmd::Tree }
        | WalletCommand::Wallet { command: WalletSubcmd::Initialize }
            => DbDependency::NeedsSled,

        // ── SQLite-only ───────────────────────────────────────────────
        // These open SQLite directly via LocalWallet. No sled access —
        // no lock contention with the daemon's exclusive sled flock.
        _ => DbDependency::SqliteOnly,
    };
    (cat, db)
}

/// Classify by async requirement only (internal).
///
/// # Architectural Groups (same trichotomy)
///
/// - Network:     async, needs P2P (Broadcast, Scan, Sync, Daemon)
/// - LocalBuild:  sync, builds ZK proofs (Native Token + Generic Capability)
/// - LocalStdin:  sync, reads stdin (ImportSecrets)
/// - Local:       sync, SQLite-only queries
fn classify_category(cmd: &WalletCommand) -> CommandCategory {
    match cmd {
        // ── Infrastructure: async, needs P2P ──────────────────────────
        WalletCommand::Broadcast
        | WalletCommand::Scan { .. }
        | WalletCommand::Sync { .. }
        | WalletCommand::Daemon => CommandCategory::Network,

        // ── Stdin reader ──────────────────────────────────────────────
        WalletCommand::Wallet { command: WalletSubcmd::ImportSecrets } => CommandCategory::LocalStdin,

        // ── Native Token: capability exercise (Merkle proofs) ─────────
        // Transfer, Redeem, Burn exercise Native Token capabilities.
        // Sole special citizen per wallet.md.
        WalletCommand::Transfer { .. }
        | WalletCommand::Redeem { .. }
        | WalletCommand::Burn { .. } => CommandCategory::LocalBuild,

        // ── Generic capability: manifest-driven (any contract) ────────
        // Deploy, Invoke, Lock go through the manifest pipeline.
        // Zero per-contract code. The manifest IS the interface.
        WalletCommand::Contract { command } => match command {
            ContractSubcmd::Deploy { .. }
            | ContractSubcmd::Invoke { .. }
            | ContractSubcmd::Lock { .. } => CommandCategory::LocalBuild,
            _ => CommandCategory::Local,
        },

        // ── Pure (no database) ────────────────────────────────────────
        WalletCommand::Help { .. } | WalletCommand::Version => CommandCategory::Local,

        // ── SQLite-only ───────────────────────────────────────────────
        _ => CommandCategory::Local,
    }
}

/// Open wallet from config. Synchronous.
pub fn open_wallet(config: &WalletConfig) -> Result<Dww> {
    let network = match config.network.as_str() {
        "mainnet" | "localnet" => dwow_sdk::crypto::keypair::Network::Mainnet,
        _ => dwow_sdk::crypto::keypair::Network::Testnet,
    };
    Ok(Dww::new(
        network,
        config.chain_path.clone(),
        config.cache_path.clone(),
        config.wallet_path.clone(),
        config.wallet_pass.clone(),
        config.production_mode,
        config.p2p_settings.clone(),
    )?)
}

/// Dispatch a synchronous command. Core commands implemented; remainder stubbed.
pub fn dispatch_sync(dww: &Dww, cmd: &WalletCommand) -> Result<()> {
    // Deploy/transfer/broadcast require synced chain to confirm balances
    // and capabilities. Standard for all full-node wallets.
    if requires_sync(cmd) && dww.chain.get_height().unwrap_or(0) == 0 {
        return Err(Error::Custom(
            "No blocks in local chain — wallet has not synced yet. Wait for sync.".into()
        ));
    }
    match cmd {
        // === Wallet commands (most are sync local) ===
        WalletCommand::Wallet { command: WalletSubcmd::Keygen } => {
            let mut output = vec![];
            dww.keygen(&mut output)?;
            for line in &output { println!("{line}"); }
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Initialize } => {
            // Fast-path: skip if already initialized (container restart).
            if dww.wallet.get_addresses().is_ok() {
                println!("Wallet already initialized — skipping.");
                return Ok(());
            }
            if let Err(e) = dww.initialize_wallet() {
                return Err(Error::Custom(format!("init wallet: {e}")));
            }
            println!("Wallet initialized.");
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Balance } => {
            let balmap = dww.token_balance()?;
            let aliases_map = dww.get_aliases_mapped_by_token()?;
            if balmap.is_empty() {
                println!("No retained balances found");
                return Ok(());
            }
            for (token_id, balance) in balmap.iter() {
                let aliases = aliases_map.get(token_id).map(|a| a.as_str()).unwrap_or("-");
                println!("{token_id}\t{aliases}\t{balance}");
            }
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Address } => {
            match dww.default_address() {
                Ok(addr) => {
                    let addr: dwow_sdk::crypto::keypair::Address =
                        dwow_sdk::crypto::keypair::StandardAddress::from_public(
                            dww.network, *addr.public_key(),
                        ).into();
                    println!("{addr}");
                    Ok(())
                }
                Err(e) => Err(Error::Custom(format!("address: {e}"))),
            }
        }
        WalletCommand::Wallet { command: WalletSubcmd::Addresses } => {
            let addresses = dww.addresses()?;
            use crate::common::prettytable_addrs;
            let table = prettytable_addrs(dww.network, &addresses);
            if table.is_empty() { println!("No addresses found"); }
            else { println!("{table}"); }
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Secrets } => {
            for secret in dww.get_secrets()? {
                println!("{secret}");
            }
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::ImportSecrets } => {
            // Read bs58-encoded secrets from stdin, one per line
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)
                .map_err(|e| Error::Custom(format!("Failed to read stdin: {e}")))?;
            if input.trim().is_empty() {
                return Err(Error::Custom(
                    "No secrets provided on stdin. Pipe bs58-encoded keys.".into()
                ));
            }
            let mut secrets = Vec::new();
            for line in input.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }
                let bytes = bs58::decode(line).into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid bs58 secret: {e}")))?;
                let key_array: [u8; 32] = bytes.try_into()
                    .map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
                let secret = dwow_sdk::crypto::SecretKey::from_bytes(key_array)
                    .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;
                secrets.push(secret);
            }
            let mut output = vec![];
            let imported = dww.import_secrets(secrets, &mut output)?;
            for line in &output { println!("{line}"); }
            println!("Imported {} secret(s)", imported.len());
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Capabilities } => {
            let caps = dww.get_held_capabilities(None)?;  // all, include revoked
            if caps.is_empty() { return Ok(()); }
            let aliases_map = dww.get_aliases_mapped_by_token()?;
            use crate::common::prettytable_held_capabilities;
            let table = prettytable_held_capabilities(&caps, &aliases_map);
            println!("{table}");
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Tree } => {
            println!("{:#?}", dww.get_cap_tree()?);
            Ok(())
        }

        // === Contract manifest — show interface ===
        WalletCommand::Contract { command: ContractSubcmd::Show { contract_id } } => {
            match dww.get_contract_manifest(contract_id) {
                Ok(Some(manifest)) => {
                    // Resolve trust tier for display
                    let trust = resolve_show_trust(contract_id, dww);
                    let resolver = crate::manifest_resolver::ManifestResolver::new(&manifest);
                    println!("{}", resolver.describe_with_trust(trust.as_ref()));
                    Ok(())
                }
                Ok(None) => {
                    println!("No manifest found for contract {contract_id}. This contract was deployed without a manifest.");
                    Ok(())
                }
                Err(e) => Err(Error::Custom(format!("Failed to read manifest: {e}"))),
            }
        }

        // === Contract deploy — wire --manifest flag ===
        WalletCommand::Contract {
            command:
                ContractSubcmd::Deploy {
                    deploy_auth,
                    wasm_path,
                    deploy_ix,
                    manifest,
                },
        } => {
            // Build deploy ix: manifest TOML takes priority if provided
            let ix_bytes = build_deploy_ix(deploy_ix.as_deref(), manifest.as_deref())?;
            // Deploy via existing async deploy path
            smol::block_on(async {
                // Parse deploy key, read WASM, build and broadcast deploy tx
                let secret_bytes = hex::decode(&deploy_auth)
                    .map_err(|e| Error::Custom(format!("Invalid deploy auth hex: {e}")))?;
                let mut key_bytes = [0u8; 32];
                key_bytes.copy_from_slice(&secret_bytes);
                let deploy_key =
                    dwow_sdk::crypto::SecretKey::from_bytes(key_bytes)
                        .map_err(|e| Error::Custom(format!("Invalid deploy key: {e}")))?;
                let keypair = dwow_sdk::crypto::Keypair::new(deploy_key);
                let wasm_bin = smol::fs::read(
                    crate::wallet_util::expand_path(&wasm_path)
                        .map_err(|e| Error::Custom(format!("Bad path: {e}")))?,
                )
                .await
                .map_err(|e| Error::Custom(format!("Failed to read WASM: {e}")))?;
                let tx = dww
                    .deploy_contract(&keypair, wasm_bin, ix_bytes)
                    .await?;
                let tx_b64 =
                    crate::wallet_util::base64_encode(
                        &dwow_serial::serialize_async(&tx).await,
                    );
                println!("Transaction (base64): {tx_b64}");
                // Confirmation prompt before broadcast
                if !confirm_broadcast() {
                    println!("Broadcast cancelled.");
                    return Ok(());
                }
                // Broadcast via P2P
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output, false, None, None).await {
                    Ok(txid) => {
                        for line in &output { println!("{line}"); }
                        println!("Deployed: {txid}");
                    }
                    Err(e) => {
                        // Transaction is built and can be broadcast later
                        println!("Deploy tx built but broadcast failed: {e}");
                        println!("Re-run 'broadcast' when P2P is connected.");
                    }
                }
                Ok(())
            })
        }

        // === Contract invoke — generic manifest-driven dispatch ===
        WalletCommand::Contract {
            command:
                ContractSubcmd::Invoke {
                    contract_id,
                    function,
                    params,
                },
        } => {
            smol::block_on(async {
                let tx = dww
                    .invoke_contract(&contract_id, &function, params.as_deref(), vec![])
                    .await?;
                let tx_b64 =
                    crate::wallet_util::base64_encode(
                        &dwow_serial::serialize_async(&tx).await,
                    );
                println!("Transaction (base64): {tx_b64}");
                // Confirmation prompt before broadcast
                if !confirm_broadcast() {
                    println!("Broadcast cancelled.");
                    return Ok(());
                }
                // Broadcast via P2P
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output, false, None, None).await {
                    Ok(txid) => {
                        for line in &output { println!("{line}"); }
                        println!("Invoked: {txid}");
                    }
                    Err(e) => {
                        println!("Invoke tx built but broadcast failed: {e}");
                        println!("Re-run 'broadcast' when P2P is connected.");
                    }
                }
                Ok(())
            })
        }

        // === Contract lock — mark deployed contract as immutable ===
        WalletCommand::Contract {
            command:
                ContractSubcmd::Lock {
                    deploy_auth,
                },
        } => {
            smol::block_on(async {
                let tx = dww.lock_contract(&deploy_auth).await?;
                let tx_b64 =
                    crate::wallet_util::base64_encode(
                        &dwow_serial::serialize_async(&tx).await,
                    );
                println!("Transaction (base64): {tx_b64}");
                // Confirmation prompt before broadcast
                if !confirm_broadcast() {
                    println!("Broadcast cancelled.");
                    return Ok(());
                }
                // Broadcast via P2P
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output, false, None, None).await {
                    Ok(txid) => {
                        for line in &output { println!("{line}"); }
                        println!("Locked: {txid}");
                    }
                    Err(e) => {
                        println!("Lock tx built but broadcast failed: {e}");
                        println!("Re-run 'broadcast' when P2P is connected.");
                    }
                }
                Ok(())
            })
        }

        // === Transfer/Redeem/Burn — CLI convenience wrappers ===
        // These are UX sugar for common PN operations (transfer, redeem, burn).
        // They parse CLI args into JSON params and route through the SAME generic
        // invoke_contract("promissory_note", function, params) path as every other
        // contract. No special code path — just parameter construction.
        //
        // Spend hooks are a common PN pattern (protocol-owned liquidity, DAO
        // callbacks). The `--spend-hook` flag on transfer/redeem is a justified
        // convenience because nearly every PN transfer in a DeFi setting uses one.
        WalletCommand::Transfer { amount, token_id, recipient, spend_hook, user_data, half_split: _ } => {
            let params = format!(
                r#"{{"amount":{},"token_id":"{}","recipient":"{}"{}{}}}"#,
                amount, token_id, recipient,
                spend_hook.as_ref().map(|s| format!(r#","spend_hook":"{}""#, s)).unwrap_or_default(),
                user_data.as_ref().map(|s| format!(r#","user_data":"{}""#, s)).unwrap_or_default(),
            );
            smol::block_on(async {
                let tx = dww.invoke_contract("promissory_note", "TransferV1", Some(&params), vec![]).await?;
                let tx_b64 = crate::wallet_util::base64_encode(&dwow_serial::serialize_async(&tx).await);
                println!("Transaction (base64): {tx_b64}");
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output, false, None, None).await {
                    Ok(txid) => { for line in &output { println!("{line}"); } println!("Transferred: {txid}"); }
                    Err(e) => { println!("Transfer tx built but broadcast failed: {e}"); }
                }
                Ok(())
            })
        }
        WalletCommand::Redeem { cap_id, spend_hook } => {
            let params = format!(
                r#"{{"cap_id":"{}"{}}}"#,
                cap_id,
                spend_hook.as_ref().map(|s| format!(r#","spend_hook":"{}""#, s)).unwrap_or_default(),
            );
            smol::block_on(async {
                let tx = dww.invoke_contract("promissory_note", "RedeemV1", Some(&params), vec![]).await?;
                let tx_b64 = crate::wallet_util::base64_encode(&dwow_serial::serialize_async(&tx).await);
                println!("Transaction (base64): {tx_b64}");
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output, false, None, None).await {
                    Ok(txid) => { for line in &output { println!("{line}"); } println!("Redeemed: {txid}"); }
                    Err(e) => { println!("Redeem tx built but broadcast failed: {e}"); }
                }
                Ok(())
            })
        }
        WalletCommand::Burn { coin_ids } => {
            let ids_json: Vec<String> = coin_ids.iter().map(|id| format!(r#""{}""#, id)).collect();
            let params = format!(r#"{{"cap_ids":[{}]}}"#, ids_json.join(","));
            smol::block_on(async {
                let tx = dww.invoke_contract("promissory_note", "BurnV1", Some(&params), vec![]).await?;
                let tx_b64 = crate::wallet_util::base64_encode(&dwow_serial::serialize_async(&tx).await);
                println!("Transaction (base64): {tx_b64}");
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output, false, None, None).await {
                    Ok(txid) => { for line in &output { println!("{line}"); } println!("Burned: {txid}"); }
                    Err(e) => { println!("Burn tx built but broadcast failed: {e}"); }
                }
                Ok(())
            })
        }

        // === Default address by index ===
        WalletCommand::Wallet { command: WalletSubcmd::DefaultAddress { index } } => {
            let addresses = dww.addresses()?;
            if *index >= addresses.len() {
                return Err(Error::Custom(format!(
                    "Index {} out of range (have {} addresses)", index, addresses.len()
                )));
            }
            let (_, public, _, _) = &addresses[*index];
            let addr: dwow_sdk::crypto::keypair::Address = dwow_sdk::crypto::keypair::StandardAddress::from_public(dww.network, *public).into();
            println!("Default address: {}", addr);
            Ok(())
        }

        // ── SQLite-only or pure — not dispatched here ────────────────
        // Commands like Help, Version, Wallet { Keygen, Balance, Address,
        // Addresses, DefaultAddress, Secrets, Capabilities } are handled
        // in main.rs via LocalWallet or before config loading.
        _ => Err(Error::Custom(format!(
            "Sync dispatch not implemented for this command — route through classify()"
        ))),
    }
}

/// Dispatch a network command. Async — called via smol::block_on.
pub async fn dispatch_async(dww: &DwwPtr, cmd: &WalletCommand) -> Result<()> {
    // Lazy P2P initialization — connects to seeds, discovers peers.
    {
        let needs_init = {
            let dww_r = dww.read().await;
            dww_r.p2p.is_none() && dww_r.p2p_settings.is_some()
        };
        if needs_init {
            let mut dww_w = dww.write().await;
            dww_w.init_p2p().await?;

            // Spawn background sync task (sync_task rewrite pending — Step 5)
            if dww_w.p2p.is_some() {
                tracing::info!(target: "drk::wallet::dispatch", "P2P initialized — sync task pending Phase 5 rewrite");
            }
        }
    }

    let dww_r = dww.read().await;
    match cmd {
        WalletCommand::Sync { command: SyncSubcmd::Status } => {
            let height = dww_r.chain.get_height().unwrap_or(0);
            let peer_tip = dww_r.highest_peer_tip.get();
            let synced = dww_r.is_synced();
            let p2p_up = dww_r.p2p.is_some();
            let peer_count = dww_r.p2p.as_ref()
                .map(|p| p.read().unwrap().peer_count())
                .unwrap_or(0);
            println!("Sync status: {}", if synced { "SYNCED" } else { "SYNCING" });
            println!("  Local chain height: {}", height);
            println!("  Network tip: {}", peer_tip);
            println!("  Peers: {}", peer_count);
            println!("  P2P connected: {}", if p2p_up { "yes" } else { "no" });
            if !synced {
                if peer_count == 0 && p2p_up {
                    println!("  No peers available — seed may be unreachable or empty hostlist.");
                    println!("  Waiting for mining nodes to register with the seed...");
                } else {
                    println!("  Run 'sync init' to start syncing, then wait for peers.");
                }
            }
            return Ok(());
        }
        WalletCommand::Sync { command: SyncSubcmd::Init } => {
            // P2P already initialized by lazy init above (line ~478).
            // Spawn the sync loop — it runs on the smol executor spawned
            // by block_on in main.rs. Note: the sync loop dies when the
            // process exits; for persistent sync, use the daemon command.
            drop(dww_r);
            {
                let dww_r2 = dww.read().await;
                if let Some(ref p2p) = dww_r2.p2p {
                    let dww2 = dww.clone();
                    let p2p2 = p2p.clone();
                    let tip2 = dww_r2.highest_peer_tip.clone();
                    smol::spawn(async move {
                        crate::sync_task::run_wallet_sync(p2p2, dww2, tip2).await;
                    }).detach();
                }
            }
            println!("P2P sync started — run 'sync status' to check progress.");
            return Ok(());
        }
        WalletCommand::Daemon => {
            // Daemon mode: P2P already initialized by lazy init above.
            // Spawn continuous sync + RPC server, then block forever.
            drop(dww_r);
            {
                let dww_r2 = dww.read().await;

                // Auto-initialize wallet if schema is missing — the daemon
                // must call initialize_wallet() before syncing so that
                // scan_block_linear can discover capabilities.
                if dww_r2.wallet.get_addresses().is_err() {
                    println!("Wallet schema not found — auto-initializing...");
                    dww_r2.initialize_wallet().map_err(|e| Error::Custom(format!("auto-init wallet: {e}")))?;
                    println!("Wallet auto-initialized.");
                }

                if let Some(ref p2p) = dww_r2.p2p {
                    let dww2 = dww.clone();
                    let p2p2 = p2p.clone();
                    let tip2 = dww_r2.highest_peer_tip.clone();
                    smol::spawn(async move {
                        // Panic-safe: restart sync on panic
                        loop {
                            let result = std::panic::AssertUnwindSafe(
                                crate::sync_task::run_wallet_sync(
                                    p2p2.clone(), dww2.clone(), tip2.clone()
                                )
                            );
                            if futures::FutureExt::catch_unwind(result).await.is_err() {
                                tracing::error!(target: "drk::wallet::sync",
                                    "Sync task panicked — restarting in 5s");
                                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                            } else {
                                break; // normal exit or error
                            }
                        }
                    }).detach();
                }

                // Verify RPC socket binds before entering pending().
                // A daemon without RPC is useless — fail fast.
                let socket_path = format!("/tmp/drk-{:?}.sock", dww_r2.network).to_lowercase();
                let handler = crate::rpc_server::DwwRpcHandler::new(dww.clone());
                let socket = socket_path.clone();
                // Test-bind: remove stale socket, try bind, report result
                let _ = std::fs::remove_file(&socket_path);
                if smol::net::unix::UnixListener::bind(&socket_path).is_err() {
                    return Err(Error::Custom(format!(
                        "RPC bind failed for {} — daemon cannot start", socket_path
                    )));
                }
                smol::spawn(async move {
                    if let Err(e) = crate::rpc_server::listen(handler, &socket).await {
                        tracing::error!(target: "drk::wallet::rpc",
                            "RPC server stopped: {}", e);
                    }
                }).detach();
                println!("Wallet daemon started — P2P sync active, container alive.");
                println!("Wallet RPC listening on {}", socket_path);
            } // read lock dropped here
            smol::future::pending::<()>().await;
            // unreachable
            Ok(())
        }
        WalletCommand::Scan { reset } => {
            if !dww_r.is_synced() {
                println!("Wallet not yet synced. P2P connected — waiting for blocks.");
                println!("Chain height: {}", dww_r.chain.get_height().unwrap_or(0));
                println!("The wallet will sync automatically as peers become available.");
                println!("Run 'scan' again once synced.");
                return Ok(());
            }
            if let Some(height) = *reset {
                let mut buf = vec![];
                if let Err(e) = dww_r.reset_to_height(height, &mut buf) {
                    return Err(Error::Custom(format!("reset: {e}")));
                }
                for line in &buf { println!("{line}"); }
            }
            dww_r.scan_blocks(&mut vec![], None, &true).await
                .map_err(|e| Error::Custom(format!("scan: {e}")))
        }
        WalletCommand::Broadcast => {
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input)
                .map_err(|e| Error::Custom(format!("Failed to read stdin: {e}")))?;
            let tx: dwow_core::tx::Transaction = dwow_serial::deserialize_async(&input).await
                .map_err(|e| Error::Custom(format!("Failed to deserialize tx: {e}")))?;
            let mut output = vec![];
            let txid = dww_r.broadcast_tx(&tx, &mut output, false, None, None).await?;
            for line in &output { println!("{line}"); }
            println!("Broadcast: {txid}");
            return Ok(());
        }

        // ── Not a network command ────────────────────────────────────
        // SQLite-only and LocalBuild commands are dispatched elsewhere.
        _ => Err(Error::Custom(format!(
            "Async dispatch not implemented for this command — route through classify()"
        ))),
    }
}

/// Print help text for the given topic. None = top-level help.
pub fn print_help(topic: Option<&str>) {
    match topic {
        Some("wallet") => println!("{}", crate::args::HELP_WALLET),
        Some("wallet-initialize") => println!("{}", crate::args::HELP_WALLET_INITIALIZE),
        _ => println!("{}", crate::args::HELP_TOP),
    }
}

/// Print version string.
pub fn print_version() {
    println!("{}", crate::args::HELP_VERSION);
}

/// Commands that require the wallet to be synced before they can execute.
/// Deploy, transfer, and broadcast need confirmed balances and capabilities.
fn requires_sync(cmd: &WalletCommand) -> bool {
    matches!(cmd,
        WalletCommand::Transfer { .. }
        | WalletCommand::Broadcast
        | WalletCommand::Redeem { .. }
        | WalletCommand::Burn { .. }
        | WalletCommand::Contract { command: ContractSubcmd::Deploy { .. } }
        | WalletCommand::Contract { command: ContractSubcmd::Invoke { .. } }
        | WalletCommand::Contract { command: ContractSubcmd::Lock { .. } }
    )
}

/// Resolve trust tier for contract show display.
/// Genesis check is authoritative. Self-deploy and attestation checks
/// happen at scan time (scan.rs resolve_manifest_trust).
fn resolve_show_trust(contract_id: &str, _dww: &Dww) -> Option<dwow_sdk::manifest::TrustTier> {
    use dwow_sdk::manifest::TrustTier;
    let cid_bytes = bs58::decode(contract_id).into_vec().ok()?;
    let cid_arr: [u8; 32] = cid_bytes.try_into().ok()?;
    let genesis_ids: [[u8; 32]; 9] = [
        dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::IDENTITY_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::ORACLE_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::ATTESTATION_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::PURSE_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::BOX_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::MULTISIG_CONTRACT_ID.to_bytes(),
    ];
    if genesis_ids.contains(&cid_arr) {
        return Some(TrustTier::Genesis);
    }
    Some(TrustTier::Unverified)
}

/// Build deploy ix bytes from --manifest flag or legacy deploy_ix string.
fn build_deploy_ix(deploy_ix: Option<&str>, manifest_path: Option<&str>) -> Result<Vec<u8>> {
    match manifest_path {
        Some(path) => {
            use dwow_sdk::manifest::ContractManifest;
            let toml_str = std::fs::read_to_string(path)
                .map_err(|e| Error::Custom(format!("Failed to read manifest file: {e}")))?;
            let m = ContractManifest::from_toml(&toml_str)
                .map_err(|e| Error::Custom(format!("Invalid manifest TOML: {e}")))?;
            m.to_deploy_ix()
                .map_err(|e| Error::Custom(format!("Failed to encode manifest: {e}")))
        }
        None => Ok(deploy_ix
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::*;

    #[test]
    fn test_classify_transfer_is_local_build() {
        let cmd = WalletCommand::Transfer {
            amount: "1".into(), token_id: "t".into(),
            recipient: "r".into(), spend_hook: None,
            user_data: None, half_split: false,
        };
        assert!(matches!(classify(&cmd).0, CommandCategory::LocalBuild));
    }

    #[test]
    fn test_classify_scan_is_network() {
        assert!(matches!(
            classify(&WalletCommand::Scan { reset: None }).0,
            CommandCategory::Network
        ));
    }

    #[test]
    fn test_classify_sync_is_network() {
        assert!(matches!(
            classify(&WalletCommand::Sync { command: SyncSubcmd::Status }).0,
            CommandCategory::Network
        ));
    }

    #[test]
    fn test_classify_wallet_keygen_is_local() {
        assert!(matches!(
            classify(&WalletCommand::Wallet { command: WalletSubcmd::Keygen }).0,
            CommandCategory::Local
        ));
    }

    #[test]
    fn test_classify_wallet_address_is_local() {
        assert!(matches!(
            classify(&WalletCommand::Wallet { command: WalletSubcmd::Address }).0,
            CommandCategory::Local
        ));
    }

    #[test]
    fn test_requires_sync_transfer() {
        assert!(requires_sync(&WalletCommand::Transfer {
            amount: "1".into(), token_id: "t".into(),
            recipient: "r".into(), spend_hook: None,
            user_data: None, half_split: false,
        }));
    }

    #[test]
    fn test_requires_sync_keygen_is_false() {
        assert!(!requires_sync(&WalletCommand::Wallet { command: WalletSubcmd::Keygen }));
    }

    #[test]
    fn test_requires_sync_scan_is_false() {
        assert!(!requires_sync(&WalletCommand::Scan { reset: None }));
    }
}
