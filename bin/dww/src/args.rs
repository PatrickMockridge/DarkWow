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

use crate::wallet_error::Error;

/// Parsed command-line arguments.
#[derive(Debug)]
pub struct WalletArgs {
    pub config: Option<String>,
    /// Path to keys.toml — the wallet's declared key (mirrors dwowd `--keys`).
    /// Falls back to the `KEYS_FILE` env var; section from `WALLET_NAME`.
    pub keys: Option<String>,
    pub network: String,
    pub network_explicit: bool,
    pub production: bool,
    pub command: WalletCommand,
    pub log: Option<String>,
    pub verbose: u8,
}

/// All wallet subcommands. Only dispatched commands exist — no zombie variants.
/// Mirrors the wallet.md CLI section (keygen/import removed — identity is declared).
#[derive(Debug, Clone, PartialEq)]
pub enum WalletCommand {
    /// Print help and exit
    Help { topic: Option<String> },
    /// Print version and exit
    Version,
    /// Wallet operations (balance, address, secrets, etc.)
    Wallet { command: WalletSubcmd },
    /// Create a payment transaction
    Transfer { amount: String, token_id: String, recipient: String, spend_hook: Option<String>, user_data: Option<String>, half_split: bool, porcelain: bool },
    /// Redeem a Promissory Note cap
    Redeem { cap_id: String, spend_hook: Option<String> },
    /// Burn Promissory Note caps
    Burn { cap_ids: Vec<String> },
    /// Read a transaction from stdin and broadcast it
    Broadcast,
    /// Scan the blockchain and parse relevant transactions
    Scan { reset: Option<u32>, porcelain: bool },
    /// P2P sync management
    Sync { command: SyncSubcmd },
    /// Start wallet daemon — P2P sync + block forever (container mode)
    Daemon,
    /// Contract functionalities
    Contract { command: ContractSubcmd },
    /// Show user position — capabilities held and available actions
    Position,
    /// Diagnostic — P2P, sync, chain, seed connectivity report
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncSubcmd {
    Init,
    Status,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WalletSubcmd {
    Initialize,
    Balance { porcelain: bool },
    Address,
    Addresses,
    DefaultAddress { index: usize },
    Secrets,
    Tree,
    Capabilities,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContractSubcmd {
    Deploy { deploy_auth: String, wasm_path: String, deploy_ix: Option<String>, manifest: Option<String> },
    Show { contract_id: String },
    Lock { deploy_auth: String },
    Invoke { contract_id: String, function: String, params: Option<String> },
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
    wallet                   Wallet operations (initialize, balance, address, ...)
    transfer                 Create a payment transaction
    redeem                   Redeem a Promissory Note cap
    burn                     Burn Promissory Note caps
    broadcast                Read tx from stdin and broadcast it
    scan                     Scan the blockchain for relevant transactions
    sync                     P2P sync management (init, status)
    contract                 Contract functionalities (deploy, invoke, ...)
    daemon                   Start wallet daemon — P2P sync + block forever";

pub const HELP_WALLET: &str = "\
dwow_wallet wallet — Wallet operations

USAGE:
    dwow_wallet wallet <SUBCOMMAND>

SUBCOMMANDS:
    initialize               Initialize wallet database
    balance                  Query the wallet for known balances
    address                  Get the default address
    addresses                Print all addresses
    default-address [INDEX]  Set the default address
    secrets                  Print all secret keys
    tree                     Print the Merkle tree
    capabilities             Print all held capabilities";

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
    let mut keys: Option<String> = None;
    let mut network = "darkwow-devnet".to_string();
    let mut network_explicit = false;
    let mut production = false;
    let mut log = None;
    let mut verbose: u8 = 0;

    let args: Vec<String> = argv.into_iter().collect();
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
                "--keys" => {
                    i += 1;
                    if i >= args.len() { return Err(Error::Custom("missing keys.toml path after --keys".into())); }
                    keys = Some(args[i].clone());
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
            config, keys, network, network_explicit, production,
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
                let wallet_names = ["initialize", "balance", "address",
                    "addresses", "default-address", "secrets",
                    "tree", "capabilities", "coins"];
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
            config, keys, network, network_explicit, production,
            command: WalletCommand::Help { topic },
            log, verbose,
        });
    }

    if command_tokens.is_empty() {
        return Err(Error::Custom("no command specified. Try --help".into()));
    }

    let cmd_tokens: Vec<&str> = command_tokens.iter().map(|s| s.as_str()).collect();
    let command = parse_command(&cmd_tokens)?;

    Ok(WalletArgs { config, keys, network, network_explicit, production, command, log, verbose })
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
        "transfer" => {
            if tokens.len() < 4 { return Err(Error::Custom("transfer requires <amount> <token> <recipient>".into())); }
            let half_split = tokens.contains(&"--half-split");
            // --porcelain: diagnostic/testing output — frozen contract for the pipeline; do not extend.
            let porcelain = tokens.contains(&"--porcelain");
            let spend_hook = extract_flag_value(tokens, "--spend-hook");
            let user_data = extract_flag_value(tokens, "--user-data");
            Ok(WalletCommand::Transfer {
                amount: tokens[1].to_string(), token_id: tokens[2].to_string(),
                recipient: tokens[3].to_string(), spend_hook, user_data, half_split, porcelain,
            })
        }
        "redeem" => {
            if tokens.len() < 2 { return Err(Error::Custom("redeem requires <cap_id>".into())); }
            Ok(WalletCommand::Redeem { cap_id: tokens[1].to_string(), spend_hook: extract_flag_value(tokens, "--spend-hook") })
        }
        "burn" => Ok(WalletCommand::Burn { cap_ids: tokens[1..].iter().map(|s| s.to_string()).collect() }),
        "broadcast" => Ok(WalletCommand::Broadcast),
        "scan" => {
            let reset = extract_flag_value(tokens, "--reset").and_then(|v| v.parse::<u32>().ok());
            // --porcelain: diagnostic/testing output — frozen contract for the pipeline; do not extend.
            let porcelain = tokens.contains(&"--porcelain");
            Ok(WalletCommand::Scan { reset, porcelain })
        }
        "sync" => {
            if tokens.len() < 2 { return Err(Error::Custom("sync requires a subcommand".into())); }
            Ok(WalletCommand::Sync { command: parse_sync_subcmd(&tokens[1..])? })
        }
        "contract" => {
            if tokens.len() < 2 { return Err(Error::Custom("contract requires a subcommand".into())); }
            Ok(WalletCommand::Contract { command: parse_contract_subcmd(&tokens[1..])? })
        }
        "daemon" => Ok(WalletCommand::Daemon),
        "position" => Ok(WalletCommand::Position),
        "diagnostic" => Ok(WalletCommand::Diagnostic),
        _ => Err(Error::Custom(format!("unknown command: {}", tokens[0]))),
    }
}

fn parse_wallet_subcmd(tokens: &[&str]) -> Result<WalletSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("wallet requires a subcommand".into())),
    };
    let wallet_names = ["initialize", "balance", "address", "addresses",
        "default-address", "secrets",
        "tree", "capabilities", "coins"];
    match match_prefix(sub, &wallet_names)? {
        "initialize" => Ok(WalletSubcmd::Initialize),
        "balance" => Ok(WalletSubcmd::Balance { porcelain: tokens.contains(&"--porcelain") }),
        "address" => Ok(WalletSubcmd::Address),
        "addresses" => Ok(WalletSubcmd::Addresses),
        "default-address" => {
            let index = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(WalletSubcmd::DefaultAddress { index })
        }
        "secrets" => Ok(WalletSubcmd::Secrets),
        "tree" => Ok(WalletSubcmd::Tree),
        "capabilities" | "coins" => Ok(WalletSubcmd::Capabilities),
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

fn parse_contract_subcmd(tokens: &[&str]) -> Result<ContractSubcmd, Error> {
    let sub = match tokens.first().copied() {
        Some(s) => s,
        None => return Err(Error::Custom("contract requires a subcommand".into())),
    };
    match match_prefix(sub, &["deploy", "show", "lock", "invoke"])? {
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
    fn test_parse_wallet_address() {
        let args = parse_args(argv("-c cfg.toml wallet address")).unwrap();
        assert_eq!(args.config.as_deref(), Some("cfg.toml"));
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Address }));
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
        assert!(matches!(args.command, WalletCommand::Scan { reset: None, .. }));
    }

    #[test]
    fn test_parse_scan_reset() {
        let args = parse_args(argv("scan --reset 42")).unwrap();
        assert!(matches!(args.command, WalletCommand::Scan { reset: Some(42), .. }));
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
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Balance { .. } }));
    }

    #[test]
    fn test_parse_broadcast() {
        let args = parse_args(argv("broadcast")).unwrap();
        assert!(matches!(args.command, WalletCommand::Broadcast));
    }

    #[test]
    fn test_parse_unknown_flag() {
        assert!(parse_args(argv("--bad-flag wallet balance")).is_err());
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
    fn test_parse_verbose_multiple() {
        let args = parse_args(argv("-vvv wallet balance")).unwrap();
        assert_eq!(args.verbose, 3);
    }

    /// Regression test: -n before a wallet subcommand MUST work.
    /// This was the bug that caused 40+ pipeline failures.
    #[test]
    fn test_parse_n_with_wallet_balance() {
        let args = parse_args(argv("-n darkwow-testnet wallet balance")).unwrap();
        assert_eq!(args.network, "darkwow-testnet");
        assert!(args.network_explicit);
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Balance { .. } }));
    }
}
