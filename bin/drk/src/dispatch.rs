/// Subcommand dispatch — classify + route to wallet methods.
///
/// Only 5 commands need network. Everything else is synchronous.

use std::io::Read;

use dwow_core::{Error, Result};

use crate::args::{
    ContractSubcmd, ExplorerSubcmd, OtcSubcmd, SyncSubcmd, TokenSubcmd,
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

/// Classify a command by its async requirement.
/// Matches the Python spec `_spec_classify()`.
pub fn classify(cmd: &WalletCommand) -> CommandCategory {
    match cmd {
        WalletCommand::Broadcast
        | WalletCommand::Scan { .. }
        | WalletCommand::Mine
        | WalletCommand::Sync { .. } => CommandCategory::Network,

        WalletCommand::Explorer { command } => match command {
            ExplorerSubcmd::FetchTx { .. } | ExplorerSubcmd::SimulateTx => CommandCategory::Network,
            ExplorerSubcmd::MiningConfig => CommandCategory::LocalStdin,
            _ => CommandCategory::Local,
        },

        WalletCommand::Spend
        | WalletCommand::AttachFee
        | WalletCommand::TxFromCalls { .. }
        | WalletCommand::Inspect => CommandCategory::LocalStdin,

        WalletCommand::Wallet { command: WalletSubcmd::ImportSecrets } => CommandCategory::LocalStdin,

        WalletCommand::Otc { command: OtcSubcmd::Join } => CommandCategory::LocalStdin,

        WalletCommand::Transfer { .. }
        | WalletCommand::Redeem { .. }
        | WalletCommand::Burn { .. } => CommandCategory::LocalBuild,

        WalletCommand::Otc { command } => match command {
            OtcSubcmd::Init { .. } | OtcSubcmd::Sign { .. } => CommandCategory::LocalBuild,
            _ => CommandCategory::Local,
        },

        WalletCommand::Token { command } => match command {
            TokenSubcmd::Import { .. }
            | TokenSubcmd::GenerateMint
            | TokenSubcmd::Create { .. }
            | TokenSubcmd::Mint { .. } => CommandCategory::LocalBuild,
            _ => CommandCategory::Local,
        },

        WalletCommand::Contract { command } => match command {
            ContractSubcmd::Deploy { .. }
            | ContractSubcmd::Invoke { .. }
            | ContractSubcmd::ExportData { .. } => CommandCategory::LocalBuild,
            _ => CommandCategory::Local,
        },

        _ => CommandCategory::Local,
    }
}

/// Open wallet from config. Synchronous.
pub fn open_wallet(config: &WalletConfig) -> Result<Dww> {
    let network = match config.network.as_str() {
        "mainnet" | "localnet" => dwow_sdk::crypto::keypair::Network::Mainnet,
        _ => dwow_sdk::crypto::keypair::Network::Testnet,
    };
    Dww::new(
        network,
        config.database.clone(),
        config.cache_path.clone(),
        config.wallet_path.clone(),
        config.wallet_pass.clone(),
        config.p2p_settings.clone(),
    )
}

/// Dispatch a synchronous command. Core commands implemented; remainder stubbed.
pub fn dispatch_sync(dww: &Dww, cmd: &WalletCommand) -> Result<()> {
    // Deploy/transfer/broadcast require synced chain to confirm balances
    // and capabilities. Standard for all full-node wallets.
    if requires_sync(cmd) && !dww.is_synced() {
        return Err(Error::Custom(
            "Wallet not synced — run 'sync init' first, then check 'sync status'".into()
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
            if let Err(e) = dww.initialize_wallet() {
                return Err(Error::Custom(format!("init wallet: {e}")));
            }
            let mut output = vec![];
            if let Err(e) = dww.initialize_promissory_note(&mut output) {
                return Err(Error::Custom(format!("init PN: {e}")));
            }
            for line in &output { println!("{line}"); }
            if let Err(e) = dww.initialize_deployooor(&mut output) {
                return Err(Error::Custom(format!("init deployooor: {e}")));
            }
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Balance } => {
            let balmap = dww.token_balance()?;
            let aliases_map = dww.get_aliases_mapped_by_token()?;
            if balmap.is_empty() {
                println!("No unspent balances found");
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
        WalletCommand::Wallet { command: WalletSubcmd::Coins } => {
            let coins = dww.get_coins(true)?;
            if coins.is_empty() { return Ok(()); }
            let aliases_map = dww.get_aliases_mapped_by_token()?;
            use crate::common::prettytable_coins;
            let table = prettytable_coins(&coins, &aliases_map);
            println!("{table}");
            Ok(())
        }
        WalletCommand::Wallet { command: WalletSubcmd::Tree } => {
            println!("{:#?}", dww.get_coin_tree()?);
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
                    dwow_core::util::path::expand_path(&wasm_path)
                        .map_err(|e| Error::Custom(format!("Bad path: {e}")))?,
                )
                .await
                .map_err(|e| Error::Custom(format!("Failed to read WASM: {e}")))?;
                let tx = dww
                    .deploy_contract(&keypair, wasm_bin, ix_bytes)
                    .await?;
                let tx_b64 =
                    dwow_core::util::encoding::base64::encode(
                        &dwow_serial::serialize_async(&tx).await,
                    );
                println!("Transaction (base64): {tx_b64}");
                // Broadcast via P2P
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output).await {
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
                    dwow_core::util::encoding::base64::encode(
                        &dwow_serial::serialize_async(&tx).await,
                    );
                println!("Transaction (base64): {tx_b64}");
                // Broadcast via P2P
                let mut output = vec![];
                match dww.broadcast_tx(&tx, &mut output).await {
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

        // === All other commands — not yet ported ===
        _ => Err(Error::Custom(
            "Command not yet ported to sync dispatch".into()
        )),
    }
}

/// Dispatch a network command. Async — called via smol::block_on.
pub async fn dispatch_async(dww: &DwwPtr, cmd: &WalletCommand, executor: &dwow_core::system::ExecutorPtr) -> Result<()> {
    // Lazy P2P initialization — connects to seeds, discovers peers.
    {
        let needs_init = {
            let dww_r = dww.read().await;
            dww_r.p2p.is_none() && dww_r.p2p_settings.is_some()
        };
        if needs_init {
            let mut dww_w = dww.write().await;
            dww_w.init_p2p(executor).await?;

            // Spawn background sync task — periodically queries peers,
            // fetches missing blocks, scans for capabilities.
            // Runs as a detached background task so dispatch_async returns immediately.
            if let (Some(p2p), Some(ref _ex)) = (dww_w.p2p.clone(), &dww_w.executor) {
                let dww_sync = dww.clone();
                let tip = dww_w.highest_peer_tip.clone();
                smol::spawn(async move {
                    if let Err(e) = crate::sync_task::run_wallet_sync(
                        p2p, dww_sync, tip).await {
                        tracing::warn!(
                            target: "drk::wallet::dispatch",
                            "Sync task exited: {e}"
                        );
                    }
                }).detach();
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
                .map(|p| p.hosts().peers().len())
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
            // Need write lock for init_p2p
            drop(dww_r);
            let mut dww_w = dww.write().await;
            if dww_w.p2p.is_none() {
                dww_w.init_p2p(executor).await?;
                // Spawn background sync task
                if let (Some(p2p), Some(ref _ex)) = (dww_w.p2p.clone(), &dww_w.executor) {
                    let dww_sync = dww.clone();
                    let tip = dww_w.highest_peer_tip.clone();
                    smol::spawn(async move {
                        if let Err(e) = crate::sync_task::run_wallet_sync(
                            p2p, dww_sync, tip).await {
                            tracing::warn!(
                                target: "drk::wallet::dispatch",
                                "Sync task exited: {e}"
                            );
                        }
                    }).detach();
                }
                println!("P2P sync started — connecting to seeds, discovering peers.");
                println!("Run 'sync status' to check progress.");
            } else {
                println!("P2P sync already running.");
            }
            return Ok(());
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
        _ => Err(Error::Custom("Network command not yet implemented".into())),
    }
}

/// Commands that require the wallet to be synced before they can execute.
/// Deploy, transfer, and broadcast need confirmed balances and capabilities.
fn requires_sync(cmd: &WalletCommand) -> bool {
    matches!(cmd,
        WalletCommand::Transfer { .. }
        | WalletCommand::Broadcast
        | WalletCommand::Redeem { .. }
        | WalletCommand::Burn { .. }
        | WalletCommand::Spend
        | WalletCommand::Contract { command: ContractSubcmd::Deploy { .. } }
        | WalletCommand::Contract { command: ContractSubcmd::Invoke { .. } }
        | WalletCommand::Otc { command: OtcSubcmd::Init { .. } }
        | WalletCommand::Otc { command: OtcSubcmd::Join }
        | WalletCommand::Otc { command: OtcSubcmd::Sign { .. } }
    )
}

/// Resolve trust tier for contract show display.
/// Genesis check is authoritative. Self-deploy and attestation checks
/// happen at scan time (scan.rs resolve_manifest_trust).
fn resolve_show_trust(contract_id: &str, _dww: &Dww) -> Option<dwow_sdk::manifest::TrustTier> {
    use dwow_sdk::manifest::TrustTier;
    let cid_bytes = bs58::decode(contract_id).into_vec().ok()?;
    let cid_arr: [u8; 32] = cid_bytes.try_into().ok()?;
    let genesis_ids: [[u8; 32]; 3] = [
        dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes(),
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
