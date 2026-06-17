/// Argument parsing module — visible, testable, no invisible derives.
///
/// Provides:
/// - WalletArgs: parsed CLI arguments
/// - WalletCommand: all 51 subcommand variants
/// - parse_args(): parses argv, returns Result — never calls exit()

use structopt::clap::{self, AppSettings};
use structopt::StructOpt;

use dwow_core::Error;

/// Parsed command-line arguments.
#[derive(Debug)]
pub struct WalletArgs {
    pub config: Option<String>,
    pub network: String,
    pub network_explicit: bool,  // true if -n/--network was passed on CLI
    pub command: WalletCommand,
    pub log: Option<String>,
    pub verbose: u8,
}

/// All wallet subcommands. Matches the Python spec `WalletCommand` type.
#[derive(Debug, StructOpt)]
pub enum WalletCommand {
    /// Wallet operations (keygen, balance, address, etc.)
    Wallet {
        #[structopt(subcommand)]
        command: WalletSubcmd,
    },

    /// Read a transaction from stdin and mark its input coins as spent
    Spend,

    /// Unspend a coin
    Unspend {
        /// base64-encoded coin to mark as unspent
        coin: String,
    },

    /// Create a payment transaction
    Transfer {
        amount: String,
        token: String,
        recipient: String,
        /// Optional contract spend hook
        spend_hook: Option<String>,
        /// Optional user data
        user_data: Option<String>,
        #[structopt(long)]
        half_split: bool,
    },

    /// Redeem a Promissory Note coin
    Redeem {
        coin_id: String,
        spend_hook: Option<String>,
    },

    /// Burn Promissory Note coins
    Burn {
        coin_ids: Vec<String>,
    },

    /// OTC atomic swap
    Otc {
        #[structopt(subcommand)]
        command: OtcSubcmd,
    },

    /// Attach the fee call to a transaction from stdin
    AttachFee,

    /// Create a transaction from newline-separated calls from stdin
    TxFromCalls {
        calls_map: Option<String>,
    },

    /// Inspect a transaction from stdin
    Inspect,

    /// Read a transaction from stdin and broadcast it
    Broadcast,

    /// Scan the blockchain and parse relevant transactions
    Scan {
        #[structopt(long)]
        reset: Option<u32>,
    },

    /// P2P sync management
    Sync {
        #[structopt(subcommand)]
        command: SyncSubcmd,
    },

    /// Explorer related subcommands
    Explorer {
        #[structopt(subcommand)]
        command: ExplorerSubcmd,
    },

    /// Manage Token aliases
    Alias {
        #[structopt(subcommand)]
        command: AliasSubcmd,
    },

    /// Token functionalities
    Token {
        #[structopt(subcommand)]
        command: TokenSubcmd,
    },

    /// Contract functionalities
    Contract {
        #[structopt(subcommand)]
        command: ContractSubcmd,
    },

    /// Mine blocks and receive rewards (LOCALNET ONLY)
    Mine,

    /// Show user position — capabilities held and available actions
    Position {
        #[structopt(long)]
        json: bool,
    },
}

#[derive(Debug, StructOpt)]
pub enum SyncSubcmd {
    /// Start P2P sync — connects to seeds, discovers peers, syncs blocks
    Init,
    /// Show sync status — local height, network tip, progress
    Status,
}

#[derive(Debug, StructOpt)]
pub enum WalletSubcmd {
    /// Initialize wallet database
    Initialize,
    /// Generate a new keypair
    Keygen,
    /// Query the wallet for known balances
    Balance,
    /// Get the default address
    Address,
    /// Print all addresses
    Addresses,
    /// Set the default address
    DefaultAddress { index: usize },
    /// Print all secret keys
    Secrets,
    /// Import secret keys from stdin
    ImportSecrets,
    /// Print the Merkle tree
    Tree,
    /// Print all coins
    Coins,
    /// Print a wallet address mining configuration
    MiningConfig {
        index: usize,
        spend_hook: Option<String>,
        user_data: Option<String>,
    },
}

#[derive(Debug, StructOpt)]
pub enum OtcSubcmd {
    /// Initialize the first half of an atomic swap
    Init {
        amount: String,
        token: String,
        receive_amount: String,
        receive_token: String,
    },
    /// Build entire swap tx given both swap halves from stdin
    Join,
    /// Inspect a swap half (JSON) from stdin
    Inspect,
    /// Sign a swap half
    Sign {
        coin_id: String,
        value: u64,
        token: String,
        receive_value: u64,
        receive_token: String,
    },
}

#[derive(Debug, StructOpt)]
pub enum ExplorerSubcmd {
    /// Fetch a blockchain transaction by hash
    FetchTx {
        tx_hash: String,
        #[structopt(long)]
        encode: bool,
    },
    /// Read a transaction from stdin and simulate it
    SimulateTx,
    /// Fetch broadcasted transactions history
    TxsHistory {
        tx_hash: Option<String>,
        #[structopt(long)]
        encode: bool,
    },
    /// Remove reverted transactions from history
    ClearReverted,
    /// Fetch scanned blocks records
    ScannedBlocks {
        height: Option<u32>,
    },
    /// Read a mining configuration from stdin and display its parts
    MiningConfig,
}

#[derive(Debug, StructOpt)]
pub enum AliasSubcmd {
    /// Create a Token alias
    Add { alias: String, token: String },
    /// Print alias info
    Show {
        #[structopt(short, long)]
        alias: Option<String>,
        #[structopt(short, long)]
        token: Option<String>,
    },
    /// Remove a Token alias
    Remove { alias: String },
}

#[derive(Debug, StructOpt)]
pub enum TokenSubcmd {
    /// Import a mint authority
    Import {
        secret_key: String,
        token_blind: String,
    },
    /// Generate a new mint authority locally
    GenerateMint,
    /// Create a new token type on-chain
    Create {
        name: String,
        supply: String,
        decimals: Option<u8>,
    },
    /// List token IDs with available mint authorities
    List,
    /// Mint more coins of an existing token
    Mint {
        token: String,
        amount: String,
        recipient: String,
        spend_hook: Option<String>,
        user_data: Option<String>,
    },
}

#[derive(Debug, StructOpt)]
pub enum ContractSubcmd {
    /// Generate a new deploy authority
    GenerateDeploy,
    /// List deploy authorities
    List {
        contract_id: Option<String>,
    },
    /// Export a contract history record
    ExportData {
        tx_hash: String,
    },
    /// Deploy a smart contract
    Deploy {
        deploy_auth: String,
        wasm_path: String,
        deploy_ix: Option<String>,
        /// Optional manifest TOML file for auto-discovery
        #[structopt(long)]
        manifest: Option<String>,
    },
    /// Show contract interface from its manifest
    Show {
        contract_id: String,
    },
    /// Lock a smart contract
    Lock {
        deploy_auth: String,
    },
    /// Invoke a smart contract function (generic — any contract via manifest)
    Invoke {
        contract_id: String,
        function: String,
        params: Option<String>,
    },
    /// Register a deployed contract ID for runtime use
    Register {
        contract_name: String,
        contract_id: String,
    },
}

/// Parse command-line arguments. Returns Result — never calls exit().
///
/// Uses clap directly via StructOpt. The App has all flags AND all
/// subcommands in ONE definition — no dual-App parsing, no from_iter_safe.
pub fn parse_args(argv: impl IntoIterator<Item = String>) -> Result<WalletArgs, Error> {
    let app = WalletCommand::clap()
        .name("dwow_wallet")
        .about("DarkWow wallet — command-line client for dwowd daemon")
        .arg(
            clap::Arg::with_name("config")
                .short("c")
                .long("config")
                .takes_value(true)
                .help("Configuration file to use"),
        )
        .arg(
            clap::Arg::with_name("network")
                .short("n")
                .long("network")
                .takes_value(true)
                .default_value("darkwow-devnet")
                .help("Blockchain network to use"),
        )
        .arg(
            clap::Arg::with_name("log")
                .short("l")
                .long("log")
                .takes_value(true)
                .help("Set log file to output into"),
        )
        .arg(
            clap::Arg::with_name("verbose")
                .short("v")
                .multiple(true)
                .help("Increase verbosity (-vvv supported)"),
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .version(&*Box::leak(
            format!("dwow_wallet {}\ncommit: {}\nbranch: {}",
                env!("CARGO_PKG_VERSION"),
                env!("GIT_HASH"),
                env!("GIT_BRANCH"),
            ).into_boxed_str(),
        ));

    let matches = app.get_matches_from_safe(argv).map_err(|e| {
        // clap prints the error message on exit; we capture and return it
        Error::Custom(e.to_string())
    })?;

    // Extract flat flags
    let config = matches.value_of("config").map(String::from);
    let network_explicit = matches.occurrences_of("network") > 0;
    let network = matches.value_of("network").unwrap_or("darkwow-devnet").to_string();
    let log = matches.value_of("log").map(String::from);
    let verbose = matches.occurrences_of("verbose") as u8;

    // Extract subcommand
    let command = WalletCommand::from_clap(&matches);

    Ok(WalletArgs { config, network, network_explicit, command, log, verbose })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_keygen() {
        let args = parse_args(vec![
            "dwow_wallet".into(),
            "-c".into(), "cfg.toml".into(),
            "wallet".into(), "keygen".into(),
        ]).unwrap();
        assert_eq!(args.config.as_deref(), Some("cfg.toml"));
        assert_eq!(args.network, "darkwow-devnet");
        assert!(matches!(args.command, WalletCommand::Wallet { command: WalletSubcmd::Keygen }));
    }

    #[test]
    fn test_parse_scan() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "scan".into(),
        ]).unwrap();
        assert!(matches!(args.command, WalletCommand::Scan { reset: None }));
    }

    #[test]
    fn test_parse_scan_reset() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "scan".into(), "--reset".into(), "42".into(),
        ]).unwrap();
        assert!(matches!(args.command, WalletCommand::Scan { reset: Some(42) }));
    }

    #[test]
    fn test_parse_transfer() {
        let args = parse_args(vec![
            "dwow_wallet".into(),
            "-n".into(), "darkwow-testnet".into(),
            "transfer".into(), "100.0".into(), "DRKW".into(), "addr1".into(),
        ]).unwrap();
        assert_eq!(args.network, "darkwow-testnet");
        assert!(matches!(args.command, WalletCommand::Transfer { .. }));
    }

    #[test]
    fn test_parse_balance() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "wallet".into(), "balance".into(),
        ]).unwrap();
        assert!(matches!(args.command,
            WalletCommand::Wallet { command: WalletSubcmd::Balance }));
    }

    #[test]
    fn test_parse_broadcast() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "broadcast".into(),
        ]).unwrap();
        assert!(matches!(args.command, WalletCommand::Broadcast));
    }

    #[test]
    fn test_parse_position() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "position".into(), "--json".into(),
        ]).unwrap();
        assert!(matches!(args.command, WalletCommand::Position { json: true }));
    }

    #[test]
    fn test_parse_unknown_flag() {
        let result = parse_args(vec![
            "dwow_wallet".into(), "--bad-flag".into(), "wallet".into(), "keygen".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_no_command() {
        let result = parse_args(vec![
            "dwow_wallet".into(), "-c".into(), "cfg.toml".into(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_contract_deploy() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "contract".into(), "deploy".into(),
            "auth".into(), "wasm.bin".into(),
        ]).unwrap();
        assert!(matches!(args.command, WalletCommand::Contract { .. }));
    }

    #[test]
    fn test_parse_contract_invoke() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "contract".into(), "invoke".into(),
            "cid".into(), "fn".into(),
        ]).unwrap();
        assert!(matches!(args.command, WalletCommand::Contract { .. }));
    }

    #[test]
    fn test_parse_mine() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "mine".into(),
        ]).unwrap();
        assert!(matches!(args.command, WalletCommand::Mine));
    }

    #[test]
    fn test_parse_verbose_multiple() {
        let args = parse_args(vec![
            "dwow_wallet".into(), "-vvv".into(), "wallet".into(), "keygen".into(),
        ]).unwrap();
        assert_eq!(args.verbose, 3);
    }
}
