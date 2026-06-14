/// Subcommand dispatch — classify + route to wallet methods.
///
/// Only 5 commands need network. Everything else is synchronous.

use dwow_core::{Error, Result};

use crate::args::{
    AliasSubcmd, ContractSubcmd, ExplorerSubcmd, OtcSubcmd, TokenSubcmd,
    WalletCommand, WalletSubcmd,
};
use crate::config::WalletConfig;
use crate::Dww;

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
        | WalletCommand::Mine => CommandCategory::Network,

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
            | ContractSubcmd::DaoEscrowInit { .. }
            | ContractSubcmd::DrainProtectionInit { .. }
            | ContractSubcmd::EnableDrainProtection { .. }
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
        config.cache_path.clone(),
        config.wallet_path.clone(),
        config.wallet_pass.clone(),
    )
}

/// Dispatch a synchronous command. Core commands implemented; remainder stubbed.
pub fn dispatch_sync(dww: &Dww, cmd: &WalletCommand) -> Result<()> {
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
            use dwow_sdk::manifest::ContractManifest;
            // Read manifest from wallet DB if stored
            match dww.get_contract_manifest(contract_id) {
                Ok(Some(manifest)) => {
                    let resolver = crate::manifest_resolver::ManifestResolver::new(&manifest);
                    println!("{}", resolver.describe());
                    Ok(())
                }
                Ok(None) => {
                    println!("No manifest found for contract {contract_id}. This contract was deployed without a manifest.");
                    Ok(())
                }
                Err(e) => Err(Error::Custom(format!("Failed to read manifest: {e}"))),
            }
        }

        // === All other commands — not yet ported ===
        _ => Err(Error::Custom(
            "Command not yet ported to sync dispatch".into()
        )),
    }
}

/// Dispatch a network command. Async — called via smol::block_on.
pub async fn dispatch_async(dww: &Dww, cmd: &WalletCommand) -> Result<()> {
    match cmd {
        WalletCommand::Scan { reset } => {
            if let Some(height) = *reset {
                let mut buf = vec![];
                if let Err(e) = dww.reset_to_height(height, &mut buf) {
                    return Err(Error::Custom(format!("reset: {e}")));
                }
                for line in &buf { println!("{line}"); }
            }
            dww.scan_blocks(&mut vec![], None, &true).await
                .map_err(|e| Error::Custom(format!("scan: {e}")))
        }
        _ => Err(Error::Custom("Network command not yet implemented".into())),
    }
}
