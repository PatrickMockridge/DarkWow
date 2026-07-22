/// Subcommand dispatch — classify + route to wallet methods.
///
/// Only 5 commands need network. Everything else is synchronous.

use std::io::Read;

use crate::integrity::print_integrity_results;
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
/// The wallet is a generic capability engine. Two contracts are hardcoded
/// infrastructure — everything else goes through the generic AEAD + manifest
/// path with zero per-contract code:
///
///   NativeToken — consensus-critical. Fee payment and coinbase rewards
///       are consensus operations, not per-contract business logic.
///   Deployooor  — deployment infrastructure. DeployV1 detection enables
///       on-chain manifest discovery for all contracts.
///
/// ```text
/// 1. Native Token path    — consensus infrastructure (Merkle proofs, fee payment)
///    Transfer, Redeem, Burn
///
/// 2. Generic capability   — manifest-driven (ANY contract, zero wallet changes)
///    Contract { Deploy, Invoke, Lock }
///
/// 3. Infrastructure       — network sync, P2P, daemon, bootstrap
///    Broadcast, Scan, Sync, Daemon, Wallet { Initialize, Tree }
///
/// 4. SQLite-only          — no sled (runs alongside daemon's exclusive lock)
///    Balance, Address, Addresses, Secrets, Capabilities
/// ```
///
/// DbDependency is an explicit per-command match — no derivation rule.
pub fn classify(cmd: &WalletCommand) -> (CommandCategory, DbDependency) {
    let cat = classify_category(cmd);
    let db = match cmd {
        // ── NativeToken — consensus infrastructure ─────────────────────
        // Consensus-critical: fee payment requires Merkle proofs.
        // Per wallet.md: NativeToken + Deployooor are the only hardcoded
        // contracts. Everything else goes through the manifest path.
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
/// - Local:       sync, SQLite-only queries
fn classify_category(cmd: &WalletCommand) -> CommandCategory {
    match cmd {
        // ── Infrastructure: async, needs P2P ──────────────────────────
        WalletCommand::Broadcast
        | WalletCommand::Scan { .. }
        | WalletCommand::Sync { .. }
        | WalletCommand::Daemon => CommandCategory::Network,


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
    let keys_toml = config.keys_toml.as_ref().map(std::path::Path::new);
    let section = config.section.as_deref().ok_or_else(|| Error::Custom(
        "WALLET_NAME not set — the wallet must declare which keys.toml section is its identity".into()))?;
    Ok(Dww::new(
        network,
        keys_toml,
        section,
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
    if requires_sync(cmd) && dww.wallet.chain_height().map(|h| h == dwow_sdk::blockchain::BlockHeight::new(0)).unwrap_or(true) {
        return Err(Error::Custom(
            "No blocks in local chain — wallet has not synced yet. Wait for sync.".into()
        ));
    }
    match cmd {
        // === Wallet commands (most are sync local) ===
        WalletCommand::Wallet { command: WalletSubcmd::Initialize } => {
            // Fast-path: skip if already initialized (container restart).
            // Probe a surviving table (held_capabilities) — the addresses table
            // is gone; the wallet derives its identity on boot.
            if dww.wallet.get_held_capabilities(Some(false)).is_ok() {
                println!("Wallet already initialized — skipping.");
                return Ok(());
            }
            if let Err(e) = dww.initialize_wallet() {
                return Err(Error::Custom(format!("init wallet: {e}")));
            }
            println!("Wallet initialized.");
            // Run startup integrity check
            let results = dww.wallet.integrity_check()
                .map_err(|e| Error::Custom(format!("Database integrity check failed: {e:?}")))?;
            print_integrity_results(&results);
            for r in &results {
                if !r.passed && r.severity == crate::integrity::IntegritySeverity::Fatal {
                    return Err(Error::Custom(
                        "Fatal database integrity errors detected. See above for recovery actions.".into()
                    ));
                }
            }
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Balance { porcelain } } => {
            let balmap = dww.capability_balance()?;
            // --porcelain: diagnostic/testing output — frozen contract for the pipeline; do not
            // extend. One line per held token: "<token_id_base58>\t<amount>". Empty = no output.
            if *porcelain {
                for (token_id, balance) in balmap.iter() {
                    println!("{token_id}\t{balance}");
                }
                return Ok(());
            }
            let aliases_map = dww.get_aliases_mapped_by_asset()?;
            if balmap.is_empty() {
                println!("No retained balances found");
                let (last_height, _) = dww.get_last_scanned_block().unwrap_or((0, String::new()));
                let secrets = dww.get_secrets().map(|s| s.len()).unwrap_or(0);
                println!("  Last scanned block: {}", last_height);
                println!("  Secrets in wallet: {}", secrets);
                if secrets == 0 {
                    println!("  ACTION: Wallet has zero secrets — check keys.toml [section] declaration (WALLET_NAME).");
                } else {
                    println!("  ACTION: Secrets present but no coins found. Run 'scan' to scan for coins.");
                    println!("    Verify mining nodes have FORWARD_DESTINATION set to this wallet's address.");
                    println!("    Current address: {}", dww.default_address().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string()));
                }
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
            // WARNING: exposes all private keys. Require confirmation (RC5 fix)
            if !confirm_broadcast() {
                println!("Aborted.");
                return Ok(());
            }
            for secret in dww.get_secrets()? {
                println!("{secret}");
            }
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Capabilities } => {
            let caps = dww.get_held_capabilities(None)?;  // all, include revoked
            if caps.is_empty() { return Ok(()); }
            let aliases_map = dww.get_aliases_mapped_by_asset()?;
            use crate::common::prettytable_held_capabilities;
            let table = prettytable_held_capabilities(&caps, &aliases_map);
            println!("{table}");
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Tree } => {
            println!("{:#?}", dww.get_capability_commitment_tree()?);
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
                    // WASM verification (Layer 2 of trust model)
                    // The WASM binary is not stored locally during scan.
                    // To verify: fetch the deploy transaction from the chain
                    // and call manifest_verify::verify_manifest_against_wasm().
                    // For genesis contracts (height 0), the WASM is unavailable
                    // locally — the deploy tx can be fetched from chain state.
                    println!("  WASM verification: not available locally (WASM binary must be fetched from chain)");
                    Ok(())
                }
                Ok(None) => {
                    let trust = resolve_show_trust(contract_id, dww);
                    println!("Contract: {contract_id}");
                    println!("  Trust: [{}]", trust.map(|t| t.to_string()).unwrap_or_else(|| "Unknown".into()));
                    println!("  No manifest — interface unknown. Generic AEAD scan still works.");
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
                // deploy_contract attaches the FeeV1 call (real fee proofs,
                // fee nullifier) and signs per-call — deploy authority row +
                // fee ephemeral row (wallet.md §6.3 steps 6-7).
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
                    .invoke_contract(&contract_id, &function, params.as_deref(), vec![], vec![])
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

        // === Transfer — two paths, by law (wallet.md §6.4, §9) ===
        // DRKW (the native token) is the ONE bespoke write-path citizen: it is
        // built by the hardcoded NativeToken client via build_native_transfer
        // (real burn/mint proofs, fee attach, nullifiers, per-call signing) —
        // it NEVER goes through invoke_contract or any manifest path.
        //
        // Non-DRKW assets route through the manifest-driven generic path.
        // (The promissory_note hardcoding below is the fired agent's A1
        // violation — ripped out and replaced by generic routing in the
        // capability-side remediation phase.)
        WalletCommand::Transfer { amount, token_id, recipient, spend_hook, user_data, half_split: _, porcelain } => {
            let is_drkw = token_id == "DRKW" || token_id == "drkw";
            smol::block_on(async {
                let tx = if is_drkw {
                    let amount: u64 = amount.parse()
                        .map_err(|e| Error::Custom(format!("Invalid amount '{amount}': {e}")))?;
                    // The shell draws the Seed — the explicit randomness name
                    // (wallet.md §6.1). Everything below it is deterministic.
                    let mut seed = [0u8; 32];
                    use rand::RngCore;
                    rand::rngs::OsRng.fill_bytes(&mut seed);
                    dww.build_native_transfer(amount, recipient, seed).await?
                } else {
                    // Generic non-native transfer: manifest-driven path.
                    // Select held capability by asset_id → resolve contract →
                    // invoke_contract → manifest → CapabilityProvider → prover_impl.
                    let all_caps = dww.wallet.get_held_capabilities(Some(false))
                        .map_err(|e| Error::Custom(format!("{:?}", e)))?;
                    let token_bytes = bs58::decode(&token_id).into_vec().unwrap_or_default();
                    let rec = all_caps.iter()
                        .find(|c| c.asset_id.to_bytes().to_vec() == token_bytes)
                        .ok_or_else(|| Error::Custom(format!(
                            "no held capability found for asset_id '{}'", token_id,
                        )))?;
                    let (contract_id, function_name) = dww.resolve_transfer_contract(rec, "transfer")
                        .map_err(|e| Error::Custom(e))?;
                    let params_json = serde_json::json!({
                        "amount": amount,
                        "recipient": recipient,
                    });
                    let cid_str = bs58::encode(contract_id.to_bytes()).into_string();
                    dww.invoke_contract(
                        &cid_str, &function_name, Some(&params_json.to_string()),
                        vec![], vec![],
                    ).await?
                };
                let tx_b64 = crate::wallet_util::base64_encode(&dwow_serial::serialize_async(&tx).await);
                if !*porcelain { println!("Transaction (base64): {tx_b64}"); }
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output, false, None, None).await {
                    Ok(txid) => {
                        if !*porcelain { for line in &output { println!("{line}"); } }
                        // Mark transferred caps as exercised (ocap lifecycle:
                        // discover → hold → exercise → revoke). Block height is
                        // unknown at broadcast time (confirm=false); revoke at
                        // current tip to prevent double-spend. Reorg reconciler
                        // will un-revoke if the block is reverted.
                        if let Err(e) = dww.mark_tx_exercise(&tx, &mut output) {
                            tracing::warn!(target: "dww::dispatch",
                                "Transfer revoke mark failed (non-fatal): {e:?}");
                        }
                        // --porcelain: diagnostic/testing output — frozen contract for the
                        // pipeline; do not extend. One line: "txid=<hex>".
                        if *porcelain { println!("txid={txid}"); } else { println!("Transferred: {txid}"); }
                        Ok(())
                    }
                    Err(e) => Err(Error::Custom(format!("Transfer tx built but broadcast failed: {e}"))),
                }
            })
        }
        // Redeem / Burn — operations that belong to the promissory_note
        // contract. Ripped out: the wallet names no contract beyond the two
        // sanctioned citizens (native_token, deployooor). These verbs are
        // replaced by the generic `contract invoke <contract_id> <action>`
        // path (wallet.md §2.2, §6.4.1) once the generic engine is rebuilt
        // (Phase 6) and a stored manifest is available for the target contract.
        WalletCommand::Redeem { .. } => {
            Err(Error::Custom("Redeem is a contract-specific operation — use \
                'contract invoke <contract_id> redeem' (the generic manifest path, \
                Phase 6 pending). No per-contract verbs in the wallet.".into()))
        }
        WalletCommand::Burn { .. } => {
            Err(Error::Custom("Burn is a contract-specific operation — use \
                'contract invoke <contract_id> burn' (the generic manifest path, \
                Phase 6 pending). No per-contract verbs in the wallet.".into()))
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

        // ── Position: capability browser ──────────────────────────
        // Shows what the user holds and what they can do.
        // Per ocap.md: the wallet is a capability browser, not an identity manager.
        WalletCommand::Position => {
            let resolver = crate::capability::CapabilityResolver::new(dww.wallet.clone());
            let view = resolver.resolve()?;
            println!("=== Capabilities ===");
            for cap in &view.capabilities {
                let status = if cap.revoked { "[EXERCISED]" } else { "[RETAINED]" };
                let disc = cap.discriminant.map(|d| format!(" {d}")).unwrap_or_default();
                let res = cap.resource.as_deref().unwrap_or("-");
                let act = cap.action.as_deref().unwrap_or("-");
                println!("  {} value={} contract={} type={} rsc={} act={} disc={}{} {}",
                    &cap.cap_id[..12], cap.value, cap.contract_name, cap.capability_name,
                    res, act, disc, cap.discriminant.map(|_| "").unwrap_or("(none)"), status);
                let prims: Vec<&str> = cap.primitives.iter().map(|p| p.name()).collect();
                let barbs: Vec<&str> = cap.barbs.iter().map(|b| b.name()).collect();
                if !prims.is_empty() || !barbs.is_empty() {
                    println!("    primitives: [{}]  barbs: [{}]", prims.join(", "), barbs.join(", "));
                }
            }
            if view.capabilities.is_empty() {
                println!("  No capabilities discovered. Sync and scan to discover.");
            }
            if !view.actions.is_empty() {
                println!("=== Available Actions ===");
                for action in &view.actions {
                    println!("  {}::{} — {} ({})",
                        action.contract_name, action.function_name,
                        action.description, action.requires_description);
                }
            }
            Ok(())
        }

        // ── Diagnostic: P2P, sync, chain report ─────────────────────
        WalletCommand::Diagnostic => {
            dww.diagnostic(&mut vec![])?;
            Ok(())
        }

        // ── SQLite-only or pure — not dispatched here ────────────────
        // Commands like Help, Version, Wallet { Balance, Address,
        // Addresses, DefaultAddress, Secrets, Capabilities } are handled
        // in main.rs via LocalWallet or before config loading.
        _ => Err(Error::Custom(format!(
            "Sync dispatch not implemented for this command — route through classify()"
        ))),
    }
}

/// Dispatch a network command. Async — called via smol::block_on.
/// `executor` is the smol executor for P2P session tasks — same pattern
/// as mining node passing its executor through async_daemonize!.
pub async fn dispatch_async(
    dww: &DwwPtr,
    cmd: &WalletCommand,
    executor: std::sync::Arc<smol::Executor<'static>>,
) -> Result<()> {
    // Lazy P2P initialization — connects to seeds, discovers peers.
    {
        let needs_init = {
            let dww_r = dww.read().await;
            let has_settings = dww_r.p2p_settings.is_some();
            let has_p2p = dww_r.p2p.is_some();
            if !has_settings {
                eprintln!("[dww] P2P config NOT present — P2P networking DISABLED. \
                    Add a [net] section to your config to enable P2P.");
            }
            !has_p2p && has_settings
        };
        if needs_init {
            eprintln!("[dww] Initializing P2P...");
            let mut dww_w = dww.write().await;
            match dww_w.init_p2p(executor.clone()).await {
                Ok(()) => {
                    if dww_w.p2p.is_some() {
                        eprintln!("[dww] P2P initialized successfully.");
                    } else {
                        eprintln!("[dww] P2P init returned Ok but p2p is None!");
                    }
                }
                Err(e) => {
                    eprintln!("[dww] P2P initialization FAILED: {e}");
                    return Err(e);
                }
            }
        } else if !needs_init {
            eprintln!("[dww] P2P init skipped (needs_init=false).");
        }
    }

    let dww_r = dww.read().await;
    match cmd {
        WalletCommand::Sync { command: SyncSubcmd::Status } => {
            let height = dww_r.wallet.chain_height().map(|h| h.get()).unwrap_or(0);
            let peer_tip = dww_r.highest_peer_tip.get();
            let synced = dww_r.is_synced();
            let p2p_up = dww_r.p2p.is_some();
            let peer_count = dww_r.p2p.as_ref()
                .map(|p| p.hosts().peers().len())
                .unwrap_or(0);
            println!("Sync status: {}", if synced { "SYNCED" } else { "SYNCING" });
            println!("  Local chain height: {}", height);
            println!("  Network tip: {}", peer_tip.get());
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
                    if let Err(e) = crate::sync_task::run_wallet_sync(p2p2, dww2, tip2).await {
                        tracing::error!("Sync task exited with error: {e}");
                    }
                    }).detach();
                }
            }
            println!("P2P sync started — run 'sync status' to check progress.");

            // Spawn auto-scan — discovers capabilities from synced blocks.
            // Sync inserts; scan decrypts. Matches the daemon's coupled
            // insert+scan model (the sync task itself only inserts; scanning
            // is a separate concurrent task).
            {
                let dww_scan = dww.clone();
                smol::spawn(async move {
                    loop {
                        smol::Timer::after(std::time::Duration::from_secs(5)).await;
                        // Ensure schema exists before attempting scan.
                        {
                            let dww_r = dww_scan.read().await;
                            if dww_r.wallet.get_held_capabilities(Some(false)).is_err() {
                                if let Err(e) = dww_r.initialize_wallet() {
                                    tracing::error!(target: "dww::sync_init::autoscan",
                                        "Schema init failed: {e:?} — retrying");
                                }
                                continue;
                            }
                        }
                        let dww_r = dww_scan.read().await;
                        if let Err(e) = dww_r.scan_blocks(
                            &mut vec![], None, &false,
                        ).await {
                            tracing::warn!(target: "dww::sync_init::autoscan",
                                "Scan cycle failed: {e:?}");
                        }
                    }
                }).detach();
            }

            return Ok(());
        }
        WalletCommand::Daemon => {
            // Daemon mode: P2P already initialized by lazy init above.
            // Spawn continuous sync + RPC server, then block forever.
            drop(dww_r);
            {
                let dww_r2 = dww.read().await;

                // L4: AEAD self-test — prove encrypt/decrypt works before
                // touching the network. If this fails, the binary's crypto
                // is broken and the daemon exits immediately.
                if let Err(e) = dww_r2.aead_self_test() {
                    eprintln!("FATAL: AEAD self-test failed: {e}");
                    eprintln!("The wallet binary's AEAD encrypt/decrypt roundtrip is broken.");
                    eprintln!("This is a build or linking error — not a network issue.");
                    return Err(e);
                }
                eprintln!("[dww] AEAD self-test passed.");

                // Auto-initialize wallet if schema is missing — the daemon
                // must call initialize_wallet() before syncing so that
                // scan_block_linear can discover capabilities.
                if dww_r2.wallet.get_held_capabilities(Some(false)).is_err() {
                    println!("Wallet schema not found — auto-initializing...");
                    dww_r2.initialize_wallet().map_err(|e| Error::Custom(format!("auto-init wallet: {e}")))?;
                    println!("Wallet auto-initialized.");
                    // Run startup integrity check
                    let results = dww_r2.wallet.integrity_check()
                        .map_err(|e| {
                            tracing::error!("[daemon] integrity check failed: {e:?}");
                            Error::Custom(format!("Database integrity check failed: {e:?}"))
                        })?;
                    print_integrity_results(&results);
                    for r in &results {
                        if !r.passed && r.severity == crate::integrity::IntegritySeverity::Fatal {
                            return Err(Error::Custom(
                                "Fatal database integrity errors detected at daemon startup. See above for recovery actions.".into()
                            ));
                        }
                    }
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
                                tracing::error!(target: "dww::wallet::sync",
                                    "Sync task panicked — restarting in 5s");
                                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                            } else {
                                break; // normal exit or error
                            }
                        }
                    }).detach();
                }

                // Spawn auto-scan task — silently scans synced blocks for
                // wallet-relevant transactions as they arrive. Uses the Dww
                // read lock (same pattern as the RPC handler), so it runs
                // concurrently with the sync task without contention.
                {
                    let dww_scan = dww.clone();
                    smol::spawn(async move {
                        // Panic-safe: restart auto-scan on panic
                        loop {
                            let result = std::panic::AssertUnwindSafe(async {
                                // Brief initial delay — let sync connect and
                                // collect the first peer tips.
                                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                                tracing::info!(target: "dww::wallet::autoscan",
                                    "Auto-scan task started");
                                let mut consecutive_failures: u32 = 0;
                                loop {
                                    let should_scan = {
                                        let dww_r = dww_scan.read().await;
                                        let last = dww_r.get_last_scanned_block()
                                            .map(|(h, _)| h as u64)
                                            .unwrap_or(0);
                                        let chain = dww_r.chain_height().map(|h| h.get()).unwrap_or(0);
                                        last < chain
                                    };
                                    if should_scan {
                                        let dww_r = dww_scan.read().await;
                                        let (last_h, _) = dww_r.get_last_scanned_block()
                                            .unwrap_or((0, String::new()));
                                        let chain_h = dww_r.chain_height().map(|h| h.get()).unwrap_or(0);
                                        tracing::info!(target: "dww::wallet::autoscan",
                                            "Scanning blocks {}-{}",
                                            last_h as u64 + 1, chain_h);
                                        if let Err(e) = dww_r.scan_blocks(
                                            &mut vec![], None, &false
                                        ).await {
                                            consecutive_failures += 1;
                                            tracing::warn!(target: "dww::wallet::autoscan",
                                                "Scan cycle failed ({}/3 consecutive): {}",
                                                consecutive_failures, e);
                                            if consecutive_failures >= 3 {
                                                tracing::error!(target: "dww::wallet::autoscan",
                                                    "Scan failed 3 consecutive times — exiting daemon. Error: {}", e);
                                                std::process::exit(1);
                                            }
                                        } else {
                                            consecutive_failures = 0;
                                        }
                                    }
                                    smol::Timer::after(std::time::Duration::from_secs(5)).await;
                                }
                            });
                            if futures::FutureExt::catch_unwind(result).await.is_err() {
                                tracing::error!(target: "dww::wallet::autoscan",
                                    "Auto-scan task panicked — restarting in 5s");
                                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                            } else {
                                break; // normal exit (unreachable in practice)
                            }
                        }
                    }).detach();
                }

                // Verify RPC socket binds before entering pending().
                // A daemon without RPC is useless — fail fast.
                // Match the config file's network name, not the enum Debug fmt.
                // Config uses "darkwow-testnet" / "darkwow-devnet"; Debug fmt is "Testnet".
                let net_str = match dww_r2.network {
                    dwow_sdk::crypto::keypair::Network::Testnet => "darkwow-testnet",
                    dwow_sdk::crypto::keypair::Network::Mainnet => "mainnet",
                };
                let socket_path = format!("/tmp/dww-{}.sock", net_str);
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
                        tracing::error!(target: "dww::wallet::rpc",
                            "RPC server stopped: {}", e);
                    }
                }).detach();
                // Loud diagnostic: announce secret count so operator knows
                // exactly what keys the wallet has for scanning.
                // HARD FAIL on error or zero secrets — a daemon that cannot
                // decrypt is a broken daemon that wastes pipeline time.
                let secrets = match dww_r2.get_secrets() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("FATAL: Cannot load secrets: {e}");
                        eprintln!("The wallet database may be corrupt or inaccessible.");
                        return Err(e);
                    }
                };
                if secrets.is_empty() {
                    eprintln!("FATAL: Wallet daemon starting with ZERO secrets — cannot decrypt coinbase.");
                    eprintln!("       Ensure keys.toml declares this wallet's section (WALLET_NAME) and is mounted.");
                    return Err(Error::Custom(
                        "Wallet daemon requires at least one secret key to decrypt coinbase".into()
                    ));
                }
                let addr = dww_r2.default_address()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                println!("Wallet daemon starting with {} secret(s). Address: {}", secrets.len(), addr);
                if dww_r2.p2p.is_some() {
                    println!("Wallet daemon — P2P sync active, container alive.");
                } else {
                    println!("Wallet daemon — P2P NOT configured (no [net] section or parse failed). Running in local-only mode.");
                }
                println!("Wallet RPC listening on {}", socket_path);
            } // read lock dropped here
            smol::future::pending::<()>().await;
            // unreachable
            Ok(())
        }
        WalletCommand::Scan { reset, porcelain } => {
            // Retry: if not synced, poll for up to 25s before giving up.
            // Transient P2P drops should not cause pipeline failures (HAZID RC6.3).
            if !dww_r.is_synced() {
                println!("Wallet not yet synced — polling for blocks (retry 5×5s)...");
                for attempt in 1..=5 {
                    smol::Timer::after(std::time::Duration::from_secs(5)).await;
                    if dww_r.is_synced() {
                        println!("Sync detected on retry {}", attempt);
                        break;
                    }
                    println!("  Retry {}/5: still waiting for sync...", attempt);
                }
                if !dww_r.is_synced() {
                    println!("Wallet still not synced after 25s. P2P connected — waiting for blocks.");
                    println!("Chain height: {}", dww_r.wallet.chain_height().map(|h| h.get()).unwrap_or(0));
                    println!("Run 'scan' again once synced.");
                    return Ok(());
                }
            }
            if let Some(height) = *reset {
                let mut buf = vec![];
                if let Err(e) = dww_r.reset_to_height(height, &mut buf) {
                    return Err(Error::Custom(format!("reset: {e}")));
                }
                for line in &buf { println!("{line}"); }
            }
            dww_r.scan_blocks(&mut vec![], None, &true).await
                .map_err(|e| Error::Custom(format!("scan: {e}")))?;

            // Post-scan summary — propagate ALL errors, no silent defaults
            let (last_height, _) = dww_r.get_last_scanned_block()
                .map_err(|e| Error::Custom(format!("get_last_scanned_block: {e}")))?;
            let cap_count = dww_r.wallet.get_held_capabilities(Some(false))
                .map_err(|e| Error::Custom(format!("get_held_capabilities: {:?}", e)))?
                .len();
            let secrets_count = dww_r.get_secrets()
                .map_err(|e| Error::Custom(format!("get_secrets: {e}")))?
                .len();
            // --porcelain: diagnostic/testing output — frozen contract for the pipeline; do not
            // extend. One line: "capabilities=<N>\tblocks=<M>" (N = held caps after scan).
            if *porcelain {
                println!("capabilities={}\tblocks={}", cap_count, last_height);
                return Ok(());
            }
            println!("Scan complete:");
            println!("  Blocks scanned through: {}", last_height);
            println!("  Capabilities discovered: {}", cap_count);
            println!("  Secrets in wallet: {}", secrets_count);
            if cap_count == 0 && secrets_count == 0 {
                println!("  ACTION: Wallet has zero secrets — check keys.toml [section] declaration (WALLET_NAME).");
            } else if cap_count == 0 {
                println!("  ACTION: Secrets present but no coins found. Check:");
                println!("    - Mining nodes have FORWARD_DESTINATION set to this wallet's address");
                println!("    - Run 'wallet address' to see this wallet's address");
                println!("    - Verify the address matches what miners are forwarding to");
            }
            return Ok(());
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

/// Dispatch a NeedsSled command via the daemon's Unix socket RPC.
/// Routes sync status, scan, balance, and other sled-backed commands
/// through the daemon to avoid sled lock contention. Matches the Python
/// spec's `_spec_rpc_dispatch()` in wallet_model.py.
pub fn rpc_dispatch(
    rpc: &crate::wallet_rpc_client::WalletRpcClient,
    cmd: &crate::args::WalletCommand,
) -> crate::wallet_error::Result<()> {
    use crate::args::{WalletCommand, WalletSubcmd, SyncSubcmd};
    match cmd {
        WalletCommand::Sync { command: SyncSubcmd::Status } => {
            match rpc.sync_status() {
                Ok(status) => {
                    let peers = status.get("peers").and_then(|v| v.as_u64()).unwrap_or(0);
                    let height = status.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
                    let synced = status.get("synced").and_then(|v| v.as_bool()).unwrap_or(false);
                    println!("Sync status: {}", if synced { "SYNCED" } else { "SYNCING" });
                    println!("  Local chain height: {}", height);
                    println!("  Network tip: {}", status.get("peer_tip").and_then(|v| v.as_u64()).unwrap_or(0));
                    println!("  Peers: {}", peers);
                    println!("  P2P connected: {}", if peers > 0 { "yes" } else { "no" });
                    Ok(())
                }
                Err(e) => Err(crate::wallet_error::Error::Custom(format!("RPC sync_status: {e}"))),
            }
        }
        WalletCommand::Wallet { command: WalletSubcmd::Balance { porcelain } } => {
            match rpc.balance() {
                Ok(balances) => {
                    // --porcelain: diagnostic/testing output — frozen contract for the pipeline;
                    // do not extend. One line per token: "<token_id>\t<amount>". Empty = no output.
                    if *porcelain {
                        for (token, amount) in &balances {
                            println!("{token}\t{amount}");
                        }
                        return Ok(());
                    }
                    if balances.is_empty() {
                        println!("No retained balances found");
                        println!("  Run 'wallet address' to see this wallet's address.");
                        println!("  Run 'scan' to scan for coinbase rewards from mining nodes.");
                        println!("  Verify mining nodes have FORWARD_DESTINATION set.");
                    } else {
                        for (token, amount) in &balances {
                            println!("{token} {amount}");
                        }
                    }
                    Ok(())
                }
                Err(e) => Err(crate::wallet_error::Error::Custom(format!("RPC balance: {e}"))),
            }
        }
        WalletCommand::Scan { porcelain, .. } => {
            let output = rpc.scan()
                .map_err(|e| crate::wallet_error::Error::Custom(format!("RPC scan: {e}")))?;
            // --porcelain: diagnostic/testing output — frozen contract for the pipeline; do not
            // extend. One line: "capabilities=<N>\tblocks=<M>". Uses the honest held-cap count.
            if *porcelain {
                let caps = rpc.get_capability_count()
                    .map_err(|e| crate::wallet_error::Error::Custom(format!("RPC capability_count: {e}")))?;
                let height = rpc.sync_status().ok()
                    .and_then(|s| s["height"].as_u64()).unwrap_or(0);
                println!("capabilities={}\tblocks={}", caps, height);
                return Ok(());
            }
            // Print scan progress from daemon
            for line in &output {
                println!("{line}");
            }
            // Print summary — same format as direct scan path
            match (rpc.get_capability_count(), rpc.sync_status(), rpc.get_secret_count()) {
                (Ok(cap_count), Ok(status), Ok(secrets_count)) => {
                    let height = status["height"].as_u64().unwrap_or(0);
                    println!("Scan complete:");
                    println!("  Blocks scanned through: {}", height);
                    println!("  Capabilities discovered: {}", cap_count);
                    println!("  Secrets in wallet: {}", secrets_count);
                }
                (Err(e), _, _) => {
                    println!("Scan complete: (capability query failed: {e})");
                }
                (_, Err(e), _) => {
                    println!("Scan complete: (sync status query failed: {e})");
                }
                (_, _, Err(e)) => {
                    println!("Scan complete: (secret count query failed: {e})");
                }
            }
            Ok(())
        }
        WalletCommand::Transfer { amount, token_id, recipient, spend_hook, user_data, porcelain, .. } => {
            let txid = rpc.transfer(
                &amount, &token_id, &recipient,
                spend_hook.as_deref(), user_data.as_deref(),
            ).map_err(|e| crate::wallet_error::Error::Custom(format!("RPC transfer: {e}")))?;
            // --porcelain: diagnostic/testing output — frozen contract for the pipeline; do not extend.
            if *porcelain { println!("txid={txid}"); } else { println!("Transferred: {txid}"); }
            Ok(())
        }
        _ => Err(crate::wallet_error::Error::Custom(format!(
            "RPC dispatch not implemented for this command — open sled directly"
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
/// Genesis check is authoritative. Self-deploy checks wallet addresses.
/// Attested tier: deferred — requires on-chain Attestation contract query.
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
    // Self-deploy tier: the deployer pubkey is stored in contract_metadata during
    // scan; comparison against the wallet's declared identity happens in
    // resolve_manifest_trust (scan path). Here we only distinguish genesis vs
    // unverified for display.
    // Attested tier: requires querying the Attestation contract on-chain.
    // The attestations_json column in contract_metadata stores attestation data
    // discovered during scan. When populated, parse and display as [ATTESTED by X].
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
            user_data: None, half_split: false, porcelain: false,
        };
        assert!(matches!(classify(&cmd).0, CommandCategory::LocalBuild));
    }

    #[test]
    fn test_classify_scan_is_network() {
        assert!(matches!(
            classify(&WalletCommand::Scan { reset: None, porcelain: false }).0,
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
            user_data: None, half_split: false, porcelain: false,
        }));
    }

    #[test]
    fn test_requires_sync_address_is_false() {
        assert!(!requires_sync(&WalletCommand::Wallet { command: WalletSubcmd::Address }));
    }

    #[test]
    fn test_requires_sync_scan_is_false() {
        assert!(!requires_sync(&WalletCommand::Scan { reset: None, porcelain: false }));
    }
}
