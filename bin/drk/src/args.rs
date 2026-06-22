/// Argument parsing module — visible, testable, no library magic.
///
/// Bitcoin Core pattern: single deterministic argv pass. No structopt, no clap,
/// no derive macros, no dual-parser, no flag propagation. Iterate argv, classify
/// each token as flag or subcommand token, dispatch via match tree.
///
/// Provides:
/// - WalletArgs: parsed CLI arguments
/// - WalletCommand: all subcommand variants
/// - parse_args(): parses argv, returns Result — never calls exit()

use dwow_core::Error;

/// Parsed command-line arguments.
#[derive(Debug)]
pub struct WalletArgs {
    pub config: Option<String>,
    pub network: String,
    pub network_explicit: bool,
    pub production: bool,
    pub command: WalletCommand,
    pub log: Option<String>,
    pub verbose: u8,
}

/// All wallet subcommands.
#[derive(Debug, Clone, PartialEq)]
pub enum WalletCommand {
    /// Wallet operations (keygen, balance, address, etc.)
    Wallet { command: WalletSubcmd },
    /// Read a transaction from stdin and mark its input capabilities as revoked
    Exercise,
    /// Retain a capability
    Retain { cap: String },
    /// Create a payment transaction
    Transfer { amount: String, token: String, recipient: String, spend_hook: Option<String>, user_data: Option<String>, half_split: bool },
    /// Redeem a Promissory Note cap
    Redeem { cap_id: String, spend_hook: Option<String> },
    /// Burn Promissory Note caps
    Burn { coin_ids: Vec<String> },
    /// OTC atomic swap
    Otc { command: OtcSubcmd },
    /// Attach the fee call to a transaction from stdin
    AttachFee,
    /// Create a transaction from newline-separated calls from stdin
    TxFromCalls { calls_map: Option<String> },
    /// Inspect a transaction from stdin
    Inspect,
    /// Read a transaction from stdin and broadcast it
    Broadcast,
    /// Scan the blockchain and parse relevant transactions
    Scan { reset: Option<u32> },
    /// P2P sync management
    Sync { command: SyncSubcmd },
    /// Explorer related subcommands
    Explorer { command: ExplorerSubcmd },
    /// Manage Token aliases
    Alias { command: AliasSubcmd },
    /// Token functionalities
    Token { command: TokenSubcmd },
    /// Contract functionalities
    Contract { command: ContractSubcmd },
    /// Mine blocks and receive rewards (LOCALNET ONLY)
    Mine,
    /// Show user position — capabilities held and available actions
    Position { json: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncSubcmd {
    Init,
    Status,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WalletSubcmd {
    Initialize,
    Keygen,
    Balance,
    Address,
    Addresses,
    DefaultAddress { index: usize },
    Secrets,
    ImportSecrets,
    Tree,
    Capabilities,
    MiningConfig { index: usize, spend_hook: Option<String>, user_data: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum OtcSubcmd {
    Init { amount: String, token: String, receive_amount: String, receive_token: String },
    Join,
    Inspect,
    Sign { cap_id: String, value: u64, token: String, receive_value: u64, receive_token: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExplorerSubcmd {
    FetchTx { tx_hash: String, encode: bool },
    SimulateTx,
    TxsHistory { tx_hash: Option<String>, encode: bool },
    ClearReverted,
    ScannedBlocks { height: Option<u32> },
    MiningConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AliasSubcmd {
    Add { alias: String, token: String },
    Show { alias: Option<String>, token: Option<String> },
    Remove { alias: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenSubcmd {
    Import { secret_key: String, token_blind: String },
    GenerateMint,
    Create { name: String, supply: String, decimals: Option<u8> },
    List,
    Mint { token: String, amount: String, recipient: String, spend_hook: Option<String>, user_data: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContractSubcmd {
    GenerateDeploy,
    List { contract_id: Option<String> },
    ExportData { tx_hash: String },
    Deploy { deploy_auth: String, wasm_path: String, deploy_ix: Option<String>, manifest: Option<String> },
    Show { contract_id: String },
    Lock { deploy_auth: String },
    Invoke { contract_id: String, function: String, params: Option<String> },
    Register { contract_name: String, contract_id: String },
}

// ===========================================================================
// BITCOIN CORE-STYLE PARSER — single deterministic argv pass
// ===========================================================================

/// Parse command-line arguments. Pure function — no derive, no clap, no magic.
/// Bitcoin Core pattern: iterate argv, classify each token as flag or command.
pub fn parse_args(argv: impl IntoIterator<Item = String>) -> Result<WalletArgs, Error> {
    let mut config = None;
    let mut network = "darkwow-devnet".to_string();
    let mut network_explicit = false;
    let mut production = false;
    let mut log = None;
    let mut verbose: u8 = 0;

    let mut args: Vec<String> = argv.into_iter().collect();
    // Skip binary name (args[0])
    let mut i = 1;
    let mut cmd_start = args.len(); // index where subcommand tokens begin

    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--config" => {
                i += 1;
                if i >= args.len() { return Err(Error::Custom("missing config path after -c/--config".into())); }
                config = Some(args[i].clone());
                cmd_start = i + 1;
            }
            "-n" | "--network" => {
                i += 1;
                if i >= args.len() { return Err(Error::Custom("missing network after -n/--network".into())); }
                network = args[i].clone();
                network_explicit = true;
                cmd_start = i + 1;
            }
            "--production" => {
                production = true;
                cmd_start = i + 1;
            }
            "-l" | "--log" => {
                i += 1;
                if i >= args.len() { return Err(Error::Custom("missing log path after -l/--log".into())); }
                log = Some(args[i].clone());
                cmd_start = i + 1;
            }
            "-v" => { verbose = 1; cmd_start = i + 1; }
            "-vv" => { verbose = 2; cmd_start = i + 1; }
            "-vvv" => { verbose = 3; cmd_start = i + 1; }
            // Flag that starts with - but isn't recognized
            s if s.starts_with('-') && s != "-c" && s != "--config" && s != "-n" && s != "--network"
                && s != "--production" && s != "-l" && s != "--log" && s != "-v" && s != "-vv" && s != "-vvv" => {
                return Err(Error::Custom(format!("unknown flag: {}", s)));
            }
            // First non-flag token — everything from here is the subcommand
            _ => {
                cmd_start = i;
                break;
            }
        }
        i += 1;
    }

    if cmd_start >= args.len() {
        return Err(Error::Custom("no command specified. Try --help".into()));
    }

    let cmd_tokens: Vec<&str> = args[cmd_start..].iter().map(|s| s.as_str()).collect();
    let command = parse_command(&cmd_tokens)?;

    Ok(WalletArgs { config, network, network_explicit, production, command, log, verbose })
}

/// Parse subcommand tokens into a WalletCommand.
fn parse_command(tokens: &[&str]) -> Result<WalletCommand, Error> {
    if tokens.is_empty() {
        return Err(Error::Custom("no command specified".into()));
    }

    match tokens[0] {
        "wallet" => {
            if tokens.len() < 2 { return Err(Error::Custom("wallet requires a subcommand".into())); }
            Ok(WalletCommand::Wallet { command: parse_wallet_subcmd(&tokens[1..])? })
        }
        "exercise" => Ok(WalletCommand::Exercise),
        "retain" => {
            if tokens.len() < 2 { return Err(Error::Custom("retain requires <cap>".into())); }
            Ok(WalletCommand::Retain { cap: tokens[1].to_string() })
        }
        "transfer" => {
            if tokens.len() < 4 { return Err(Error::Custom("transfer requires <amount> <token> <recipient>".into())); }
            let half_split = tokens.contains(&"--half-split");
            let spend_hook = extract_flag_value(tokens, "--spend-hook");
            let user_data = extract_flag_value(tokens, "--user-data");
            Ok(WalletCommand::Transfer {
                amount: tokens[1].to_string(), token: tokens[2].to_string(),
                recipient: tokens[3].to_string(), spend_hook, user_data, half_split,
            })
        }
        "redeem" => {
            if tokens.len() < 2 { return Err(Error::Custom("redeem requires <cap_id>".into())); }
            Ok(WalletCommand::Redeem { cap_id: tokens[1].to_string(), spend_hook: extract_flag_value(tokens, "--spend-hook") })
        }
        "burn" => Ok(WalletCommand::Burn { coin_ids: tokens[1..].iter().map(|s| s.to_string()).collect() }),
        "otc" => {
            if tokens.len() < 2 { return Err(Error::Custom("otc requires a subcommand".into())); }
            Ok(WalletCommand::Otc { command: parse_otc_subcmd(&tokens[1..])? })
        }
        "attach-fee" => Ok(WalletCommand::AttachFee),
        "tx-from-calls" => Ok(WalletCommand::TxFromCalls { calls_map: extract_flag_value(tokens, "--calls-map") }),
        "inspect" => Ok(WalletCommand::Inspect),
        "broadcast" => Ok(WalletCommand::Broadcast),
        "scan" => {
            let reset = extract_flag_value(tokens, "--reset").and_then(|v| v.parse::<u32>().ok());
            Ok(WalletCommand::Scan { reset })
        }
        "sync" => {
            if tokens.len() < 2 { return Err(Error::Custom("sync requires a subcommand".into())); }
            Ok(WalletCommand::Sync { command: parse_sync_subcmd(&tokens[1..])? })
        }
        "explorer" => {
            if tokens.len() < 2 { return Err(Error::Custom("explorer requires a subcommand".into())); }
            Ok(WalletCommand::Explorer { command: parse_explorer_subcmd(&tokens[1..])? })
        }
        "alias" => {
            if tokens.len() < 2 { return Err(Error::Custom("alias requires a subcommand".into())); }
            Ok(WalletCommand::Alias { command: parse_alias_subcmd(&tokens[1..])? })
        }
        "token" => {
            if tokens.len() < 2 { return Err(Error::Custom("token requires a subcommand".into())); }
            Ok(WalletCommand::Token { command: parse_token_subcmd(&tokens[1..])? })
        }
        "contract" => {
            if tokens.len() < 2 { return Err(Error::Custom("contract requires a subcommand".into())); }
            Ok(WalletCommand::Contract { command: parse_contract_subcmd(&tokens[1..])? })
        }
        "mine" => Ok(WalletCommand::Mine),
        "position" => Ok(WalletCommand::Position { json: tokens.contains(&"--json") }),
        _ => Err(Error::Custom(format!("unknown command: {}", tokens[0]))),
    }
}

fn parse_wallet_subcmd(tokens: &[&str]) -> Result<WalletSubcmd, Error> {
    match tokens.first().copied() {
        Some("initialize") => Ok(WalletSubcmd::Initialize),
        Some("keygen") => Ok(WalletSubcmd::Keygen),
        Some("balance") => Ok(WalletSubcmd::Balance),
        Some("address") => Ok(WalletSubcmd::Address),
        Some("addresses") => Ok(WalletSubcmd::Addresses),
        Some("default-address") => {
            let index = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(WalletSubcmd::DefaultAddress { index })
        }
        Some("secrets") => Ok(WalletSubcmd::Secrets),
        Some("import-secrets") => Ok(WalletSubcmd::ImportSecrets),
        Some("tree") => Ok(WalletSubcmd::Tree),
        Some("capabilities") => Ok(WalletSubcmd::Capabilities),
        Some("mining-config") => {
            let index = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(WalletSubcmd::MiningConfig {
                index, spend_hook: extract_flag_value(tokens, "--spend-hook"),
                user_data: extract_flag_value(tokens, "--user-data"),
            })
        }
        Some(s) => Err(Error::Custom(format!("unknown wallet subcommand: {}", s))),
        None => Err(Error::Custom("wallet requires a subcommand".into())),
    }
}

fn parse_sync_subcmd(tokens: &[&str]) -> Result<SyncSubcmd, Error> {
    match tokens.first().copied() {
        Some("init") => Ok(SyncSubcmd::Init),
        Some("status") => Ok(SyncSubcmd::Status),
        Some(s) => Err(Error::Custom(format!("unknown sync subcommand: {}", s))),
        None => Err(Error::Custom("sync requires a subcommand".into())),
    }
}

fn parse_otc_subcmd(tokens: &[&str]) -> Result<OtcSubcmd, Error> {
    match tokens.first().copied() {
        Some("init") => {
            if tokens.len() < 5 { return Err(Error::Custom("otc init requires <amount> <token> <receive_amount> <receive_token>".into())); }
            Ok(OtcSubcmd::Init {
                amount: tokens[1].to_string(), token: tokens[2].to_string(),
                receive_amount: tokens[3].to_string(), receive_token: tokens[4].to_string(),
            })
        }
        Some("join") => Ok(OtcSubcmd::Join),
        Some("inspect") => Ok(OtcSubcmd::Inspect),
        Some("sign") => {
            if tokens.len() < 5 { return Err(Error::Custom("otc sign requires <cap_id> <value> <token> <receive_value> <receive_token>".into())); }
            Ok(OtcSubcmd::Sign {
                cap_id: tokens[1].to_string(),
                value: tokens[2].parse().map_err(|_| Error::Custom("invalid value".into()))?,
                token: tokens[3].to_string(),
                receive_value: tokens[4].parse().map_err(|_| Error::Custom("invalid receive_value".into()))?,
                receive_token: tokens[5].to_string(),
            })
        }
        Some(s) => Err(Error::Custom(format!("unknown otc subcommand: {}", s))),
        None => Err(Error::Custom("otc requires a subcommand".into())),
    }
}

fn parse_explorer_subcmd(tokens: &[&str]) -> Result<ExplorerSubcmd, Error> {
    match tokens.first().copied() {
        Some("fetch-tx") => Ok(ExplorerSubcmd::FetchTx {
            tx_hash: tokens.get(1).map(|s| s.to_string()).unwrap_or_default(),
            encode: tokens.contains(&"--encode"),
        }),
        Some("simulate-tx") => Ok(ExplorerSubcmd::SimulateTx),
        Some("txs-history") => Ok(ExplorerSubcmd::TxsHistory {
            tx_hash: tokens.get(1).map(|s| s.to_string()),
            encode: tokens.contains(&"--encode"),
        }),
        Some("clear-reverted") => Ok(ExplorerSubcmd::ClearReverted),
        Some("scanned-blocks") => Ok(ExplorerSubcmd::ScannedBlocks {
            height: tokens.get(1).and_then(|s| s.parse().ok()),
        }),
        Some("mining-config") => Ok(ExplorerSubcmd::MiningConfig),
        Some(s) => Err(Error::Custom(format!("unknown explorer subcommand: {}", s))),
        None => Err(Error::Custom("explorer requires a subcommand".into())),
    }
}

fn parse_alias_subcmd(tokens: &[&str]) -> Result<AliasSubcmd, Error> {
    match tokens.first().copied() {
        Some("add") => {
            if tokens.len() < 3 { return Err(Error::Custom("alias add requires <alias> <token>".into())); }
            Ok(AliasSubcmd::Add { alias: tokens[1].to_string(), token: tokens[2].to_string() })
        }
        Some("show") => Ok(AliasSubcmd::Show {
            alias: extract_flag_value(tokens, "--alias").or(extract_flag_value(tokens, "-a")),
            token: extract_flag_value(tokens, "--token").or(extract_flag_value(tokens, "-t")),
        }),
        Some("remove") => {
            if tokens.len() < 2 { return Err(Error::Custom("alias remove requires <alias>".into())); }
            Ok(AliasSubcmd::Remove { alias: tokens[1].to_string() })
        }
        Some(s) => Err(Error::Custom(format!("unknown alias subcommand: {}", s))),
        None => Err(Error::Custom("alias requires a subcommand".into())),
    }
}

fn parse_token_subcmd(tokens: &[&str]) -> Result<TokenSubcmd, Error> {
    match tokens.first().copied() {
        Some("import") => {
            if tokens.len() < 3 { return Err(Error::Custom("token import requires <secret_key> <token_blind>".into())); }
            Ok(TokenSubcmd::Import { secret_key: tokens[1].to_string(), token_blind: tokens[2].to_string() })
        }
        Some("generate-mint") => Ok(TokenSubcmd::GenerateMint),
        Some("create") => {
            if tokens.len() < 3 { return Err(Error::Custom("token create requires <name> <supply>".into())); }
            Ok(TokenSubcmd::Create {
                name: tokens[1].to_string(), supply: tokens[2].to_string(),
                decimals: tokens.get(3).and_then(|s| s.parse().ok()),
            })
        }
        Some("list") => Ok(TokenSubcmd::List),
        Some("mint") => {
            if tokens.len() < 4 { return Err(Error::Custom("token mint requires <token> <amount> <recipient>".into())); }
            Ok(TokenSubcmd::Mint {
                token: tokens[1].to_string(), amount: tokens[2].to_string(),
                recipient: tokens[3].to_string(),
                spend_hook: extract_flag_value(tokens, "--spend-hook"),
                user_data: extract_flag_value(tokens, "--user-data"),
            })
        }
        Some(s) => Err(Error::Custom(format!("unknown token subcommand: {}", s))),
        None => Err(Error::Custom("token requires a subcommand".into())),
    }
}

fn parse_contract_subcmd(tokens: &[&str]) -> Result<ContractSubcmd, Error> {
    match tokens.first().copied() {
        Some("generate-deploy") => Ok(ContractSubcmd::GenerateDeploy),
        Some("list") => Ok(ContractSubcmd::List { contract_id: tokens.get(1).map(|s| s.to_string()) }),
        Some("export-data") => {
            if tokens.len() < 2 { return Err(Error::Custom("contract export-data requires <tx_hash>".into())); }
            Ok(ContractSubcmd::ExportData { tx_hash: tokens[1].to_string() })
        }
        Some("deploy") => {
            if tokens.len() < 3 { return Err(Error::Custom("contract deploy requires <deploy_auth> <wasm_path>".into())); }
            Ok(ContractSubcmd::Deploy {
                deploy_auth: tokens[1].to_string(), wasm_path: tokens[2].to_string(),
                deploy_ix: tokens.get(3).map(|s| s.to_string()),
                manifest: extract_flag_value(tokens, "--manifest"),
            })
        }
        Some("show") => {
            if tokens.len() < 2 { return Err(Error::Custom("contract show requires <contract_id>".into())); }
            Ok(ContractSubcmd::Show { contract_id: tokens[1].to_string() })
        }
        Some("lock") => {
            if tokens.len() < 2 { return Err(Error::Custom("contract lock requires <deploy_auth>".into())); }
            Ok(ContractSubcmd::Lock { deploy_auth: tokens[1].to_string() })
        }
        Some("invoke") => {
            if tokens.len() < 3 { return Err(Error::Custom("contract invoke requires <contract_id> <function>".into())); }
            Ok(ContractSubcmd::Invoke {
                contract_id: tokens[1].to_string(), function: tokens[2].to_string(),
                params: extract_flag_value(tokens, "--params"),
            })
        }
        Some("register") => {
            if tokens.len() < 3 { return Err(Error::Custom("contract register requires <contract_name> <contract_id>".into())); }
            Ok(ContractSubcmd::Register { contract_name: tokens[1].to_string(), contract_id: tokens[2].to_string() })
        }
        Some(s) => Err(Error::Custom(format!("unknown contract subcommand: {}", s))),
        None => Err(Error::Custom("contract requires a subcommand".into())),
    }
}

/// Extract a flag value: --flag <value> → Some(value). returns None if flag not present.
fn extract_flag_value(tokens: &[&str], flag: &str) -> Option<String> {
    for i in 0..tokens.len().saturating_sub(1) {
        if tokens[i] == flag {
            return tokens.get(i + 1).map(|s| s.to_string());
        }
    }
    None
}

// ===========================================================================
// TESTS — same assertions as before, new parser
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        std::iter::once("dwow_wallet".to_string())
            .chain(s.split_whitespace().map(String::from))
            .collect()
    }

    #[test]
    fn test_parse_keygen() {
        let args = parse_args(argv("-c cfg.toml wallet keygen")).unwrap();
        assert_eq!(args.config.as_deref(), Some("cfg.toml"));
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Keygen }));
    }

    #[test]
    fn test_parse_wallet_initialize_with_config() {
        let args = parse_args(argv("-c cfg.toml wallet initialize")).unwrap();
        assert_eq!(args.config.as_deref(), Some("cfg.toml"));
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Initialize }));
    }

    #[test]
    fn test_parse_scan() {
        let args = parse_args(argv("scan")).unwrap();
        assert!(matches!(args.command, WalletCommand::Scan { reset: None }));
    }

    #[test]
    fn test_parse_scan_reset() {
        let args = parse_args(argv("scan --reset 42")).unwrap();
        assert!(matches!(args.command, WalletCommand::Scan { reset: Some(42) }));
    }

    #[test]
    fn test_parse_transfer() {
        let args = parse_args(argv("-n darkwow-testnet transfer 100.0 DRKW addr1")).unwrap();
        assert_eq!(args.network, "darkwow-testnet");
        assert!(matches!(args.command, WalletCommand::Transfer { .. }));
    }

    #[test]
    fn test_parse_balance() {
        let args = parse_args(argv("wallet balance")).unwrap();
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Balance }));
    }

    #[test]
    fn test_parse_broadcast() {
        let args = parse_args(argv("broadcast")).unwrap();
        assert!(matches!(args.command, WalletCommand::Broadcast));
    }

    #[test]
    fn test_parse_position() {
        let args = parse_args(argv("position --json")).unwrap();
        assert!(matches!(args.command, WalletCommand::Position { json: true }));
    }

    #[test]
    fn test_parse_unknown_flag() {
        assert!(parse_args(argv("--bad-flag wallet keygen")).is_err());
    }

    #[test]
    fn test_parse_no_command() {
        assert!(parse_args(argv("-c cfg.toml")).is_err());
    }

    #[test]
    fn test_parse_contract_deploy() {
        let args = parse_args(argv("contract deploy auth wasm.bin")).unwrap();
        assert!(matches!(args.command, WalletCommand::Contract { .. }));
    }

    #[test]
    fn test_parse_contract_invoke() {
        let args = parse_args(argv("contract invoke cid fn")).unwrap();
        assert!(matches!(args.command, WalletCommand::Contract { .. }));
    }

    #[test]
    fn test_parse_mine() {
        let args = parse_args(argv("mine")).unwrap();
        assert!(matches!(args.command, WalletCommand::Mine));
    }

    #[test]
    fn test_parse_verbose_multiple() {
        let args = parse_args(argv("-vvv wallet keygen")).unwrap();
        assert_eq!(args.verbose, 3);
    }

    /// Regression test: -n before wallet initialize MUST work.
    /// This was the bug that caused 40+ pipeline failures.
    #[test]
    fn test_parse_n_with_wallet_keygen() {
        let args = parse_args(argv("-n darkwow-testnet wallet keygen")).unwrap();
        assert_eq!(args.network, "darkwow-testnet");
        assert!(args.network_explicit);
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Keygen }));
    }
}
