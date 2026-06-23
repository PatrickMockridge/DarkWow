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
    /// Print help and exit
    Help { topic: Option<String> },
    /// Print version and exit
    Version,
    /// Wallet operations (keygen, balance, address, etc.)
    Wallet { command: WalletSubcmd },
    /// Read a transaction from stdin and mark its input capabilities as revoked
    Exercise,
    /// Retain a capability
    Retain { cap: String },
    /// Create a payment transaction
    Transfer { amount: String, token_id: String, recipient: String, spend_hook: Option<String>, user_data: Option<String>, half_split: bool },
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
    Cap { command: CapSubcmd },
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
    Init { amount: String, token_id: String, receive_amount: String, receive_token_id: String },
    Join,
    Inspect,
    Sign { cap_id: String, value: u64, token_id: String, receive_value: u64, receive_token_id: String },
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
    Add { alias: String, cap: String },
    Show { alias: Option<String>, cap: Option<String> },
    Remove { alias: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CapSubcmd {
    Import { secret_key: String, cap_blind: String },
    GenerateMint,
    Create { name: String, supply: String, decimals: Option<u8> },
    List,
    Mint { token_id: String, amount: String, recipient: String, spend_hook: Option<String>, user_data: Option<String> },
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
// HELP TEXT — matches old clap docstrings exactly for pipeline smoke test
// ===========================================================================

pub const HELP_TOP: &str = "\
dwow_wallet — DarkWow wallet command-line client

USAGE:
    dwow_wallet [FLAGS] [COMMAND]

FLAGS:
    -c, --config <PATH>      Configuration file to use
    -n, --network <NET>      Blockchain network to use (default: darkwow-devnet)
        --production         Enable production security checks
    -l, --log <PATH>         Set log file to output into
    -v, -vv, -vvv            Increase verbosity
    -V, --version            Print version and exit
    -h, --help               Print this help and exit

COMMANDS:
    wallet                   Wallet operations (initialize, keygen, balance, ...)
    transfer                 Create a payment transaction
    redeem                   Redeem a Promissory Note cap
    burn                     Burn Promissory Note caps
    otc                      OTC atomic swap
    exercise                 Read tx from stdin and mark inputs as revoked
    retain                   Retain a capability
    attach-fee               Attach the fee call to a tx from stdin
    tx-from-calls            Create tx from newline-separated calls from stdin
    inspect                  Inspect a transaction from stdin
    broadcast                Read tx from stdin and broadcast it
    scan                     Scan the blockchain for relevant transactions
    sync                     P2P sync management (init, status)
    explorer                 Explorer subcommands (fetch-tx, simulate-tx, ...)
    alias                    Manage token aliases (add, show, remove)
    cap                      Token functionalities (import, create, mint, ...)
    contract                 Contract functionalities (deploy, invoke, ...)
    mine                     Mine blocks and receive rewards (LOCALNET ONLY)
    position                 Show user position — capabilities held and actions";

pub const HELP_WALLET: &str = "\
dwow_wallet wallet — Wallet operations

USAGE:
    dwow_wallet wallet <SUBCOMMAND>

SUBCOMMANDS:
    initialize               Initialize wallet database
    keygen                   Generate a new keypair
    balance                  Query the wallet for known balances
    address                  Get the default address
    addresses                Print all addresses
    default-address [INDEX]  Set the default address
    secrets                  Print all secret keys
    import-secrets           Import secret keys from stdin
    tree                     Print the Merkle tree
    capabilities             Print all held capabilities
    mining-config [INDEX]    Print a wallet address mining configuration";

pub const HELP_WALLET_INITIALIZE: &str = "\
dwow_wallet wallet initialize — Initialize wallet database

Initialize wallet database";

pub const HELP_VERSION: &str = concat!(
    "dwow_wallet ", env!("CARGO_PKG_VERSION"), "\n",
    "commit: ", env!("GIT_HASH"), "\n",
    "branch: ", env!("GIT_BRANCH"),
);

// ===========================================================================
// PREFIX MATCHING — unambiguous prefix matching (clap v2 behavior)
// ===========================================================================

/// Match a user-provided string against a list of known subcommand names.
/// Returns the matched name if exactly one name starts with the input.
/// If multiple names match, returns an error listing the candidates.
/// If no names match, returns an error.
fn match_prefix<'a>(input: &'a str, candidates: &[&'a str]) -> Result<&'a str, Error> {
    // Exact match first
    if candidates.contains(&input) {
        return Ok(input);
    }
    // Prefix match — must be unambiguous
    let matches: Vec<&&str> = candidates.iter().filter(|c| c.starts_with(input)).collect();
    match matches.len() {
        0 => Err(Error::Custom(format!("unknown subcommand: {}. Candidates: {:?}", input, candidates))),
        1 => Ok(matches[0]),
        _ => Err(Error::Custom(format!(
            "ambiguous subcommand: {} matches: {:?}",
            input,
            matches.iter().map(|s| **s).collect::<Vec<_>>()
        ))),
    }
}

// ===========================================================================
// BITCOIN CORE-STYLE PARSER — single deterministic argv pass
// ===========================================================================

/// Parse command-line arguments. Pure function — no derive, no clap, no magic.
/// Hand-rolled parser. Single deterministic argv pass. Matches `spec_parse_args`
/// in the Python spec (contrib/model/wallet_model.py).
pub fn parse_args(argv: impl IntoIterator<Item = String>) -> Result<WalletArgs, Error> {
    let mut config = None;
    let mut network = "darkwow-devnet".to_string();
    let mut network_explicit = false;
    let mut production = false;
    let mut log = None;
    let mut verbose: u8 = 0;

    let mut args: Vec<String> = argv.into_iter().collect();
    let mut i = 1; // skip binary name
    let mut command_tokens: Vec<String> = Vec::new();
    let mut help_requested = false;
    let mut version_requested = false;
    let mut in_command = false; // once true, pass through everything including --flags

    while i < args.len() {
        let arg = args[i].clone();
        // -h/--help and -V/--version detected at any position
        if arg == "-h" || arg == "--help" {
            help_requested = true;
            i += 1;
            continue;
        }
        if arg == "-V" || arg == "--version" {
            version_requested = true;
            i += 1;
            continue;
        }
        if in_command {
            command_tokens.push(arg);
        } else {
            match arg.as_str() {
                "-c" | "--config" => {
                    i += 1;
                    if i >= args.len() { return Err(Error::Custom("missing config path after -c/--config".into())); }
                    config = Some(args[i].clone());
                }
                "-n" | "--network" => {
                    i += 1;
                    if i >= args.len() { return Err(Error::Custom("missing network after -n/--network".into())); }
                    network = args[i].clone();
                    network_explicit = true;
                }
                "--production" => { production = true; }
                "-l" | "--log" => {
                    i += 1;
                    if i >= args.len() { return Err(Error::Custom("missing log path after -l/--log".into())); }
                    log = Some(args[i].clone());
                }
                "-v" => { verbose = 1; }
                "-vv" => { verbose = 2; }
                "-vvv" => { verbose = 3; }
                s if s.starts_with('-') => {
                    return Err(Error::Custom(format!("unknown flag: {}", s)));
                }
                _ => {
                    command_tokens.push(arg);
                    in_command = true;
                }
            }
        }
        i += 1;
    }

    // --version takes priority
    if version_requested {
        return Ok(WalletArgs {
            config, network, network_explicit, production,
            command: WalletCommand::Version,
            log, verbose,
        });
    }

    // --help: context-aware
    if help_requested {
        let topic = if command_tokens.is_empty() {
            None
        } else if command_tokens[0] == "wallet" {
            if command_tokens.len() >= 2 {
                let sub = &command_tokens[1];
                let wallet_names = ["initialize", "keygen", "balance", "address",
                    "addresses", "default-address", "secrets",
                    "import-secrets", "tree", "capabilities", "mining-config"];
                if let Ok(matched) = match_prefix(sub, &wallet_names) {
                    if matched == "initialize" {
                        Some("wallet-initialize".to_string())
                    } else {
                        Some("wallet".to_string())
                    }
                } else {
                    Some("wallet".to_string())
                }
            } else {
                Some("wallet".to_string())
            }
        } else {
            None
        };
        return Ok(WalletArgs {
            config, network, network_explicit, production,
            command: WalletCommand::Help { topic },
            log, verbose,
        });
    }

    if command_tokens.is_empty() {
        return Err(Error::Custom("no command specified. Try --help".into()));
    }

    let cmd_tokens: Vec<&str> = command_tokens.iter().map(|s| s.as_str()).collect();
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
                amount: tokens[1].to_string(), token_id: tokens[2].to_string(),
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
        "cap" => {
            if tokens.len() < 2 { return Err(Error::Custom("cap requires a subcommand".into())); }
            Ok(WalletCommand::Cap { command: parse_cap_subcmd(&tokens[1..])? })
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
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("wallet requires a subcommand".into())),
    };
    let wallet_names = ["initialize", "keygen", "balance", "address", "addresses",
        "default-address", "secrets", "import-secrets", "tree", "capabilities", "mining-config"];
    match match_prefix(sub, &wallet_names)? {
        "initialize" => Ok(WalletSubcmd::Initialize),
        "keygen" => Ok(WalletSubcmd::Keygen),
        "balance" => Ok(WalletSubcmd::Balance),
        "address" => Ok(WalletSubcmd::Address),
        "addresses" => Ok(WalletSubcmd::Addresses),
        "default-address" => {
            let index = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(WalletSubcmd::DefaultAddress { index })
        }
        "secrets" => Ok(WalletSubcmd::Secrets),
        "import-secrets" => Ok(WalletSubcmd::ImportSecrets),
        "tree" => Ok(WalletSubcmd::Tree),
        "capabilities" => Ok(WalletSubcmd::Capabilities),
        "mining-config" => {
            let index = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(WalletSubcmd::MiningConfig {
                index, spend_hook: extract_flag_value(tokens, "--spend-hook"),
                user_data: extract_flag_value(tokens, "--user-data"),
            })
        }
        _ => unreachable!(), // match_prefix only returns values from wallet_names
    }
}

fn parse_sync_subcmd(tokens: &[&str]) -> Result<SyncSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("sync requires a subcommand".into())),
    };
    match match_prefix(sub, &["init", "status"])? {
        "init" => Ok(SyncSubcmd::Init),
        "status" => Ok(SyncSubcmd::Status),
        _ => unreachable!(),
    }
}

fn parse_otc_subcmd(tokens: &[&str]) -> Result<OtcSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("otc requires a subcommand".into())),
    };
    match match_prefix(sub, &["init", "join", "inspect", "sign"])? {
        "init" => {
            if tokens.len() < 5 { return Err(Error::Custom("otc init requires <amount> <token> <receive_amount> <receive_token>".into())); }
            Ok(OtcSubcmd::Init {
                amount: tokens[1].to_string(), token_id: tokens[2].to_string(),
                receive_amount: tokens[3].to_string(), receive_token_id: tokens[4].to_string(),
            })
        }
        "join" => Ok(OtcSubcmd::Join),
        "inspect" => Ok(OtcSubcmd::Inspect),
        "sign" => {
            if tokens.len() < 6 { return Err(Error::Custom("otc sign requires <cap_id> <value> <token> <receive_value> <receive_token>".into())); }
            Ok(OtcSubcmd::Sign {
                cap_id: tokens[1].to_string(),
                value: tokens[2].parse().map_err(|_| Error::Custom("invalid value".into()))?,
                token_id: tokens[3].to_string(),
                receive_value: tokens[4].parse().map_err(|_| Error::Custom("invalid receive_value".into()))?,
                receive_token_id: tokens[5].to_string(),
            })
        }
        _ => unreachable!(),
    }
}

fn parse_explorer_subcmd(tokens: &[&str]) -> Result<ExplorerSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("explorer requires a subcommand".into())),
    };
    match match_prefix(sub, &["fetch-tx", "simulate-tx", "txs-history",
        "clear-reverted", "scanned-blocks", "mining-config"])? {
        "fetch-tx" => Ok(ExplorerSubcmd::FetchTx {
            tx_hash: tokens.get(1).map(|s| s.to_string()).unwrap_or_default(),
            encode: tokens.contains(&"--encode"),
        }),
        "simulate-tx" => Ok(ExplorerSubcmd::SimulateTx),
        "txs-history" => Ok(ExplorerSubcmd::TxsHistory {
            tx_hash: tokens.get(1).map(|s| s.to_string()),
            encode: tokens.contains(&"--encode"),
        }),
        "clear-reverted" => Ok(ExplorerSubcmd::ClearReverted),
        "scanned-blocks" => Ok(ExplorerSubcmd::ScannedBlocks {
            height: tokens.get(1).and_then(|s| s.parse().ok()),
        }),
        "mining-config" => Ok(ExplorerSubcmd::MiningConfig),
        _ => unreachable!(),
    }
}

fn parse_alias_subcmd(tokens: &[&str]) -> Result<AliasSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("alias requires a subcommand".into())),
    };
    match match_prefix(sub, &["add", "show", "remove"])? {
        "add" => {
            if tokens.len() < 3 { return Err(Error::Custom("alias add requires <alias> <token>".into())); }
            Ok(AliasSubcmd::Add { alias: tokens[1].to_string(), cap: tokens[2].to_string() })
        }
        "show" => Ok(AliasSubcmd::Show {
            alias: extract_flag_value(tokens, "--alias").or(extract_flag_value(tokens, "-a")),
            cap: extract_flag_value(tokens, "--token").or(extract_flag_value(tokens, "-t")),
        }),
        "remove" => {
            if tokens.len() < 2 { return Err(Error::Custom("alias remove requires <alias>".into())); }
            Ok(AliasSubcmd::Remove { alias: tokens[1].to_string() })
        }
        _ => unreachable!(),
    }
}

fn parse_cap_subcmd(tokens: &[&str]) -> Result<CapSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("cap requires a subcommand".into())),
    };
    match match_prefix(sub, &["import", "generate-mint", "create", "list", "mint"])? {
        "import" => {
            if tokens.len() < 3 { return Err(Error::Custom("token import requires <secret_key> <token_blind>".into())); }
            Ok(CapSubcmd::Import { secret_key: tokens[1].to_string(), cap_blind: tokens[2].to_string() })
        }
        "generate-mint" => Ok(CapSubcmd::GenerateMint),
        "create" => {
            if tokens.len() < 3 { return Err(Error::Custom("token create requires <name> <supply>".into())); }
            Ok(CapSubcmd::Create {
                name: tokens[1].to_string(), supply: tokens[2].to_string(),
                decimals: tokens.get(3).and_then(|s| s.parse().ok()),
            })
        }
        "list" => Ok(CapSubcmd::List),
        "mint" => {
            if tokens.len() < 4 { return Err(Error::Custom("token mint requires <token> <amount> <recipient>".into())); }
            Ok(CapSubcmd::Mint {
                token_id: tokens[1].to_string(), amount: tokens[2].to_string(),
                recipient: tokens[3].to_string(),
                spend_hook: extract_flag_value(tokens, "--spend-hook"),
                user_data: extract_flag_value(tokens, "--user-data"),
            })
        }
        _ => unreachable!(),
    }
}

fn parse_contract_subcmd(tokens: &[&str]) -> Result<ContractSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("contract requires a subcommand".into())),
    };
    match match_prefix(sub, &["generate-deploy", "list", "export-data",
        "deploy", "show", "lock", "invoke", "register"])? {
        "generate-deploy" => Ok(ContractSubcmd::GenerateDeploy),
        "list" => Ok(ContractSubcmd::List { contract_id: tokens.get(1).map(|s| s.to_string()) }),
        "export-data" => {
            if tokens.len() < 2 { return Err(Error::Custom("contract export-data requires <tx_hash>".into())); }
            Ok(ContractSubcmd::ExportData { tx_hash: tokens[1].to_string() })
        }
        "deploy" => {
            if tokens.len() < 3 { return Err(Error::Custom("contract deploy requires <deploy_auth> <wasm_path>".into())); }
            Ok(ContractSubcmd::Deploy {
                deploy_auth: tokens[1].to_string(), wasm_path: tokens[2].to_string(),
                deploy_ix: tokens.get(3).map(|s| s.to_string()),
                manifest: extract_flag_value(tokens, "--manifest"),
            })
        }
        "show" => {
            if tokens.len() < 2 { return Err(Error::Custom("contract show requires <contract_id>".into())); }
            Ok(ContractSubcmd::Show { contract_id: tokens[1].to_string() })
        }
        "lock" => {
            if tokens.len() < 2 { return Err(Error::Custom("contract lock requires <deploy_auth>".into())); }
            Ok(ContractSubcmd::Lock { deploy_auth: tokens[1].to_string() })
        }
        "invoke" => {
            if tokens.len() < 3 { return Err(Error::Custom("contract invoke requires <contract_id> <function>".into())); }
            Ok(ContractSubcmd::Invoke {
                contract_id: tokens[1].to_string(), function: tokens[2].to_string(),
                params: extract_flag_value(tokens, "--params"),
            })
        }
        "register" => {
            if tokens.len() < 3 { return Err(Error::Custom("contract register requires <contract_name> <contract_id>".into())); }
            Ok(ContractSubcmd::Register { contract_name: tokens[1].to_string(), contract_id: tokens[2].to_string() })
        }
        _ => unreachable!(),
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
