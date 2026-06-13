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

use std::{
    io::{stdin, Read},
    process::exit,
    str::FromStr,
};

use prettytable::{format, row, Table};
use rand::rngs::OsRng;
use smol::{channel::unbounded, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::info;

use dwow_core::{
    async_daemonize, cli_desc,
    system::ExecutorPtr,
    util::{
        encoding::base64,
        logger::{set_terminal_writer, ChannelWriter},
        parse::encode_base10,
        path::expand_path,
    },
    Error, Result,
};
use dwow_sdk::{
    crypto::{
        keypair::{Address, Keypair, SecretKey, StandardAddress},
        BaseBlind, ContractId, FuncId,
    },
    pasta::{group::ff::PrimeField, pallas},
    tx::TransactionHash,
};
use dwow_serial::{deserialize_async, serialize_async};

use dwow_wallet::{
    cli_util::{
        display_mining_config, generate_completions, kaching, parse_blockchain_config,
        parse_calls_from_stdin, parse_mining_config_from_stdin, parse_tree,
        parse_tx_from_stdin, print_output, tx_from_calls_mapped,
    },
    common::*,
    contract_imports::promissory_note::{TokenId, BALANCE_BASE10_DECIMALS},
    swap::PartialSwapData,
    Dww,
};
use dwow_sdk::crypto::{util::FieldElemAsStr, PublicKey};

const CONFIG_FILE: &str = "dww_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dww_config.toml");

// Dev Note: when adding/modifying args here,
// don't forget to update cli_util::generate_completions()
// and interactive::help().
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "dwow_wallet", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "darkwow-devnet")]
    /// Blockchain network to use
    network: String,

    #[structopt(subcommand)]
    /// Sub command to execute
    command: Subcmd,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

// Dev Note: when adding/modifying commands here,
// don't forget to update cli_util::generate_completions()
#[derive(Clone, Debug, Deserialize, StructOpt)]
enum Subcmd {
    /// Wallet operations
    Wallet {
        #[structopt(subcommand)]
        /// Sub command to execute
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
        /// Amount to send
        amount: String,

        /// Token ID to send
        token: String,

        /// Recipient address
        recipient: String,

        /// Optional contract spend hook to use
        spend_hook: Option<String>,

        /// Optional user data to use
        user_data: Option<String>,

        #[structopt(long)]
        /// Split the output coin into two equal halves
        half_split: bool,
    },

    /// Redeem a Promissory Note coin (RedeemV1) — destroys value, creates receipt
    Redeem {
        /// Coin ID to redeem (base58-encoded)
        coin_id: String,

        /// Optional spend_hook for the receipt coin
        spend_hook: Option<String>,
    },

    /// Burn Promissory Note coins (BurnV1) — destroys coins, publishes nullifiers
    Burn {
        /// Coin IDs to burn (base58-encoded), space-separated
        coin_ids: Vec<String>,
    },

    /// OTC atomic swap
    Otc {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: OtcSubcmd,
    },

    /// Attach the fee call to a transaction given from stdin
    AttachFee,

    /// Create a transaction from newline-separated calls from stdin and attach the fee call
    TxFromCalls {
        /// Optional parent/children dependency map for the calls
        calls_map: Option<String>,
    },

    /// Inspect a transaction from stdin
    Inspect,

    /// Read a transaction from stdin and broadcast it
    Broadcast,

    /// Scan the blockchain and parse relevant transactions
    Scan {
        #[structopt(long)]
        /// Reset wallet state to provided block height and start scanning
        reset: Option<u32>,
    },

    /// Explorer related subcommands
    Explorer {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: ExplorerSubcmd,
    },

    /// Manage Token aliases
    Alias {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: AliasSubcmd,
    },

    /// Token functionalities
    Token {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: TokenSubcmd,
    },

    /// Contract functionalities
    Contract {
        #[structopt(subcommand)]
        /// Sub command to execute
        command: ContractSubcmd,
    },

    /// Mine blocks and receive rewards (LOCALNET ONLY)
    Mine,

    /// Show user position — capabilities held and available actions
    Position {
        /// Output machine-parseable JSON instead of human-readable text.
        #[structopt(long)]
        json: bool,
    },

}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum WalletSubcmd {
    /// Initialize wallet database
    Initialize,

    /// Generate a new keypair in the wallet
    Keygen,

    /// Query the wallet for known balances
    Balance,

    /// Get the default address in the wallet
    Address,

    /// Print all the addresses in the wallet
    Addresses,

    /// Set the default address in the wallet
    DefaultAddress {
        /// Identifier of the address
        index: usize,
    },

    /// Print all the secret keys from the wallet
    Secrets,

    /// Import secret keys from stdin into the wallet, separated by newlines
    ImportSecrets,

    /// Print the Merkle tree in the wallet
    Tree,

    /// Print all the coins in the wallet
    Coins,

    /// Print a wallet address mining configuration
    MiningConfig {
        /// Identifier of the address
        index: usize,

        /// Optional contract spend hook to use
        spend_hook: Option<String>,

        /// Optional user data to use
        user_data: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum OtcSubcmd {
    /// Initialize the first half of an atomic swap
    Init {
        /// Amount to send (e.g., "100.00")
        amount: String,

        /// Token ID to send
        token: String,

        /// Amount to receive (e.g., "50.00")
        receive_amount: String,

        /// Token ID to receive
        receive_token: String,
    },

    /// Build entire swap tx given both swap halves from stdin
    Join,

    /// Inspect a swap half (JSON) from stdin
    Inspect,

    /// Sign a swap half — output JSON for out-of-band exchange
    Sign {
        /// Coin ID to send (base58-encoded)
        coin_id: String,

        /// Value to send
        value: u64,

        /// Token ID to send
        token: String,

        /// Value to receive
        receive_value: u64,

        /// Token ID to receive
        receive_token: String,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum ExplorerSubcmd {
    /// Fetch a blockchain transaction by hash
    FetchTx {
        /// Transaction hash
        tx_hash: String,

        #[structopt(long)]
        /// Encode transaction to base64
        encode: bool,
    },

    /// Read a transaction from stdin and simulate it
    SimulateTx,

    /// Fetch broadcasted transactions history
    TxsHistory {
        /// Fetch specific history record (optional)
        tx_hash: Option<String>,

        #[structopt(long)]
        /// Encode specific history record transaction to base64
        encode: bool,
    },

    /// Remove reverted transactions from history
    ClearReverted,

    /// Fetch scanned blocks records
    ScannedBlocks {
        /// Fetch specific height record (optional)
        height: Option<u32>,
    },

    /// Read a mining configuration from stdin and display its parts
    MiningConfig,
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum AliasSubcmd {
    /// Create a Token alias
    Add {
        /// Token alias
        alias: String,

        /// Token to create alias for
        token: String,
    },

    /// Print alias info of optional arguments.
    /// If no argument is provided, list all the aliases in the wallet.
    Show {
        /// Token alias to search for
        #[structopt(short, long)]
        alias: Option<String>,

        /// Token to search alias for
        #[structopt(short, long)]
        token: Option<String>,
    },

    /// Remove a Token alias
    Remove {
        /// Token alias to remove
        alias: String,
    },
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum TokenSubcmd {
    /// Import a mint authority
    Import {
        /// Mint authority secret key
        secret_key: String,

        /// Mint authority token blind
        token_blind: String,
    },

    /// Generate a new mint authority locally (no on-chain creation)
    GenerateMint,

    /// Create a new token type on-chain with initial supply
    Create {
        /// Token name
        name: String,

        /// Initial supply amount
        supply: String,

        /// Number of decimal places (default: 8)
        decimals: Option<u8>,
    },

    /// List token IDs with available mint authorities
    List,

    /// Mint more coins of an existing token
    Mint {
        /// Token ID to mint
        token: String,

        /// Amount to mint
        amount: String,

        /// Recipient of the minted tokens
        recipient: String,

        /// Optional contract spend hook to use
        spend_hook: Option<String>,

        /// Optional user data to use
        user_data: Option<String>,
    },

}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum ContractSubcmd {
    /// Generate a new deploy authority
    GenerateDeploy,

    /// List deploy authorities in the wallet (or a specific one)
    List {
        /// Contract ID (optional)
        contract_id: Option<String>,
    },

    /// Export a contract history record wasm bincode and deployment instruction, encoded to base64
    ExportData {
        /// Record transaction hash
        tx_hash: String,
    },

    /// Deploy a smart contract
    Deploy {
        /// Deploy authority secret key (hex encoded)
        deploy_auth: String,

        /// Path to contract wasm bincode
        wasm_path: String,

        /// Optional path to serialized deploy instruction
        deploy_ix: Option<String>,
    },

    /// Lock a smart contract
    Lock {
        /// Deploy authority secret key (hex encoded)
        deploy_auth: String,
    },

    /// Invoke a smart contract function
    Invoke {
        /// Contract ID to invoke
        contract_id: String,

        /// Function name to call
        function: String,

        /// Path to JSON file with function parameters
        params: Option<String>,
    },

    /// Initialize a DAO-Escrow endowment
    DaoEscrowInit {
        /// DAO bulla (use "zero" for standalone endowment)
        dao_bulla: String,

        /// Endowment token ID (Base58 encoded)
        endowment_token_id: String,

        /// Optional owner public key (derived from wallet if not provided)
        #[structopt(long)]
        owner_pubkey: Option<String>,

        /// Optional bulla blind (randomly generated if not provided)
        #[structopt(long)]
        bulla_blind: Option<String>,

        /// Enable drain protection
        #[structopt(long)]
        enable_drain_protection: bool,
    },

    /// Initialize a DrainProtection protected fund
    DrainProtectionInit {
        /// Fund ID (typically same as DAO-Escrow bulla, Base58 encoded)
        fund_id: String,

        /// Spend authority public key (Base58 encoded)
        spend_authority: String,

        /// DAO-Escrow bulla this fund protects (Base58 encoded)
        dao_escrow_bulla: String,

        /// Base rate limit in basis points (e.g., 100 = 1% per 1000 blocks)
        #[structopt(long)]
        rate_limit_bps: Option<u64>,

        /// Vote threshold in basis points (e.g., 667 = 66.7%)
        #[structopt(long)]
        vote_threshold_bps: Option<u64>,
    },

    /// Enable DrainProtection on a DAO-Escrow endowment
    EnableDrainProtection {
        /// DAO-Escrow bulla (Base58 encoded)
        dao_escrow_bulla: String,

        /// DrainProtection bulla (Base58 encoded)
        drain_protection_bulla: String,
    },

    /// Register a deployed contract ID for runtime use
    Register {
        /// Contract name (e.g., "promissory_note", "dao_escrow", "drain_protection")
        contract_name: String,

        /// Contract ID to register (Base58 encoded)
        contract_id: String,
    },
}

async_daemonize!(realmain);
async fn realmain(args: Args, _ex: ExecutorPtr) -> Result<()> {
    // Grab blockchain network configuration
    let (network, blockchain_config) = match args.network.as_str() {
        "localnet" => parse_blockchain_config(args.config, "localnet", CONFIG_FILE).await?,
        "testnet" => parse_blockchain_config(args.config, "testnet", CONFIG_FILE).await?,
        "darkwow-devnet" => parse_blockchain_config(args.config, "darkwow-devnet", CONFIG_FILE).await?,
        "darkwow-testnet" => parse_blockchain_config(args.config, "darkwow-testnet", CONFIG_FILE).await?,
        "mainnet" => parse_blockchain_config(args.config, "mainnet", CONFIG_FILE).await?,
        _ => {
            eprintln!("Unsupported chain `{}`", args.network);
            return Err(Error::UnsupportedChain)
        }
    };

    // Wallet setup — matches upstream drk pattern
    let dww = Dww::new(
        network,
        blockchain_config.cache_path,
        blockchain_config.wallet_path,
        blockchain_config.wallet_pass,
    )?;

    match args.command {

        Subcmd::Wallet { command } => {
            match command {
                WalletSubcmd::Initialize => {
                    if let Err(e) = dww.initialize_wallet() {
                        eprintln!("Error initializing wallet: {e}");
                        exit(2);
                    }
                    let mut output = vec![];
                    if let Err(e) = dww.initialize_promissory_note(&mut output) {
                        print_output(&output);
                        eprintln!("Failed to initialize PromissoryNote: {e}");
                        exit(2);
                    }
                    print_output(&output);
                    if let Err(e) = dww.initialize_deployooor(&mut output) {
                        eprintln!("Failed to initialize Deployooor: {e}");
                        exit(2);
                    }
                }

                WalletSubcmd::Keygen => {
                    let mut output = vec![];
                    if let Err(e) = dww.keygen(&mut output) {
                        print_output(&output);
                        eprintln!("Failed to generate keypair: {e}");
                        exit(2);
                    }
                    print_output(&output);
                }

                WalletSubcmd::Balance => {
                    let balmap = dww.token_balance()?;

                    let aliases_map = dww.get_aliases_mapped_by_token()?;

                    // Create a prettytable with the new data:
                    let mut table = Table::new();
                    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                    table.set_titles(row!["Token ID", "Aliases", "Balance"]);
                    for (token_id, balance) in balmap.iter() {
                        let aliases = match aliases_map.get(token_id) {
                            Some(a) => a,
                            None => "-",
                        };

                        table.add_row(row![
                            token_id,
                            aliases,
                            encode_base10(*balance, BALANCE_BASE10_DECIMALS)
                        ]);
                    }

                    if table.is_empty() {
                        println!("No unspent balances found");
                    } else {
                        println!("{table}");
                    }
                }

                WalletSubcmd::Address => match dww.default_address() {
                    Ok(address) => {
                        let addr: Address =
                            StandardAddress::from_public(dww.network, *address.public_key()).into();
                        println!("{addr}");
                    }
                    Err(e) => {
                        eprintln!("Failed to fetch default address: {e}");
                        exit(2);
                    }
                },

                WalletSubcmd::Addresses => {
                    let addresses = dww.addresses()?;
                    let table = prettytable_addrs(dww.network, &addresses);

                    if table.is_empty() {
                        println!("No addresses found");
                    } else {
                        println!("{table}");
                    }
                }

                WalletSubcmd::DefaultAddress { index } => {
                    if let Err(e) = dww.set_default_address(index) {
                        eprintln!("Failed to set default address: {e}");
                        exit(2);
                    }
                }

                WalletSubcmd::Secrets => {
                    for secret in dww.get_secrets()? {
                        println!("{secret}");
                    }
                }

                WalletSubcmd::ImportSecrets => {
                    let mut secrets = vec![];
                    let lines = stdin().lines();
                    for (i, line) in lines.enumerate() {
                        if let Ok(line) = line {
                            let bytes = bs58::decode(&line.trim()).into_vec()?;
                            let Ok(secret) = deserialize_async(&bytes).await else {
                                println!("Warning: Failed to deserialize secret on line {i}");
                                continue
                            };
                            secrets.push(secret);
                        }
                    }

                    let mut output = vec![];
                    let pubkeys = match dww.import_secrets(secrets, &mut output) {
                        Ok(p) => {
                            print_output(&output);
                            p
                        }
                        Err(e) => {
                            print_output(&output);
                            eprintln!("Failed to import secret keys into wallet: {e}");
                            exit(2);
                        }
                    };

                    for key in pubkeys {
                        println!("{key}");
                    }
                }

                WalletSubcmd::Tree => {
                    println!("{:#?}", dww.get_coin_tree()?);
                }

                WalletSubcmd::Coins => {
                    let coins = dww.get_coins(true)?;
                    if coins.is_empty() {
                        return Ok(())
                    }
                    let aliases_map = dww.get_aliases_mapped_by_token()?;
                    let table = prettytable_coins(&coins, &aliases_map);
                    println!("{table}");
                }

                WalletSubcmd::MiningConfig { index, spend_hook, user_data } => {
                    let spend_hook = match spend_hook {
                        Some(s) => match FuncId::from_str(&s) {
                            Ok(s) => Some(s),
                            Err(e) => {
                                eprintln!("Invalid spend hook: {e}");
                                exit(2);
                            }
                        },
                        None => None,
                    };

                    let user_data = match user_data {
                        Some(u) => {
                            let bytes: [u8; 32] = match bs58::decode(&u).into_vec()?.try_into() {
                                Ok(b) => b,
                                Err(e) => {
                                    eprintln!("Invalid user data: {e:?}");
                                    exit(2);
                                }
                            };

                            match pallas::Base::from_repr(bytes).into() {
                                Some(v) => Some(v),
                                None => {
                                    eprintln!("Invalid user data");
                                    exit(2);
                                }
                            }
                        }
                        None => None,
                    };

                    let mut output = vec![];
                    if let Err(e) =
                        dww.mining_config(index, spend_hook, user_data, &mut output)
                    {
                        print_output(&output);
                        eprintln!("Failed to generate wallet mining configuration: {e}");
                        exit(2);
                    }
                    print_output(&output);
                }
            }

            Ok(())
        }

        Subcmd::Spend => {
            let tx = parse_tx_from_stdin().await?;


            let mut output = vec![];
            if let Err(e) = dww.mark_tx_spend(&tx, &mut output) {
                print_output(&output);
                eprintln!("Failed to mark transaction coins as spent: {e}");
                exit(2);
            };
            print_output(&output);

            Ok(())
        }

        Subcmd::Unspend { coin } => {
            let bytes: [u8; 32] = match bs58::decode(&coin).into_vec()?.try_into() {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Invalid coin: {e:?}");
                    exit(2);
                }
            };

            let elem: pallas::Base = match pallas::Base::from_repr(bytes).into() {
                Some(v) => v,
                None => {
                    eprintln!("Invalid coin");
                    exit(2);
                }
            };


            if let Err(e) = dww.unspend_coin(&elem) {
                eprintln!("Failed to mark coin as unspent: {e}");
                exit(2);
            };

            Ok(())
        }

        Subcmd::Transfer { amount, token, recipient, spend_hook, user_data, half_split } => {
            // using pre-constructed dww

            if let Err(e) = f64::from_str(&amount) {
                eprintln!("Invalid amount: {e}");
                exit(2);
            }

            let rcpt = match Address::from_str(&recipient) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Invalid recipient: {e}");
                    exit(2);
                }
            };

            if rcpt.network() != dww.network {
                eprintln!("Recipient address prefix mismatch");
                exit(2);
            }

            let token_id = match dww.get_token(token) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Invalid token alias: {e}");
                    exit(2);
                }
            };

            let spend_hook = match spend_hook {
                Some(s) => {
                    let bytes: [u8; 32] = match bs58::decode(&s).into_vec()?.try_into() {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("Invalid spend hook: {e:?}");
                            exit(2);
                        }
                    };
                    match pallas::Base::from_repr(bytes).into() {
                        Some(v) => Some(v),
                        None => {
                            eprintln!("Invalid spend hook");
                            exit(2);
                        }
                    }
                },
                None => None,
            };

            let user_data = match user_data {
                Some(u) => {
                    let bytes: [u8; 32] = match bs58::decode(&u).into_vec()?.try_into() {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("Invalid user data: {e:?}");
                            exit(2);
                        }
                    };

                    match pallas::Base::from_repr(bytes).into() {
                        Some(v) => Some(v),
                        None => {
                            eprintln!("Invalid user data");
                            exit(2);
                        }
                    }
                }
                None => None,
            };

            let tx = match dww
                .transfer(&amount, token_id, *rcpt.public_key(), spend_hook, user_data, half_split)
                .await
            {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to create payment transaction: {e}");
                    exit(2);
                }
            };

            println!("{}", base64::encode(&serialize_async(&tx).await));

            dww.stop_rpc_client().await
        }

        Subcmd::Redeem { coin_id, spend_hook } => {
            // using pre-constructed dww

            let spend_hook = match spend_hook {
                Some(s) => {
                    let bytes: [u8; 32] = match bs58::decode(&s).into_vec()?.try_into() {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("Invalid spend hook: {e:?}");
                            exit(2);
                        }
                    };
                    match pallas::Base::from_repr(bytes).into() {
                        Some(v) => Some(v),
                        None => {
                            eprintln!("Invalid spend hook");
                            exit(2);
                        }
                    }
                }
                None => None,
            };

            let tx = match dww.redeem(coin_id, spend_hook).await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to create redeem transaction: {e}");
                    exit(2);
                }
            };

            println!("{}", base64::encode(&serialize_async(&tx).await));

            dww.stop_rpc_client().await
        }

        Subcmd::Burn { coin_ids } => {
            // using pre-constructed dww

            let tx = match dww.burn(coin_ids).await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Failed to create burn transaction: {e}");
                    exit(2);
                }
            };

            println!("{}", base64::encode(&serialize_async(&tx).await));

            dww.stop_rpc_client().await
        }

        Subcmd::Otc { command } => match command {
            OtcSubcmd::Init { amount, token, receive_amount, receive_token } => {
                let token_id = match TokenId::from_str(&token) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid send Token ID: {e}");
                        exit(2);
                    }
                };
                let receive_token_id = match TokenId::from_str(&receive_token) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid receive Token ID: {e}");
                        exit(2);
                    }
                };

                match dww.init_swap(&amount, token_id, &receive_amount, receive_token_id).await {
                    Ok(h) => println!("{}", h.to_json()),
                    Err(e) => {
                        eprintln!("Failed to create swap half: {e}");
                        exit(2);
                    }
                };
                dww.stop_rpc_client().await
            }

            OtcSubcmd::Join => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                // Expect two newline-separated JSON PartialSwapData objects
                let parts: Vec<&str> = buf.trim().split('\n').collect();
                if parts.len() != 2 {
                    eprintln!("Expected two newline-separated PartialSwapData JSON objects on stdin");
                    exit(2);
                }
                let our_swap = match PartialSwapData::from_json(parts[0]) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to parse our swap data: {e}");
                        exit(2);
                    }
                };
                let their_swap = match PartialSwapData::from_json(parts[1]) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Failed to parse their swap data: {e}");
                        exit(2);
                    }
                };

                let tx = match dww.join_swap(&our_swap, &their_swap).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create swap transaction: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));
                dww.stop_rpc_client().await
            }

            OtcSubcmd::Inspect => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;

                if let Err(e) = dww.inspect_swap(buf.trim()) {
                    eprintln!("Failed to inspect swap: {e}");
                    exit(2);
                };

                Ok(())
            }

            OtcSubcmd::Sign { coin_id, value, token, receive_value, receive_token } => {
                let token_id = match TokenId::from_str(&token) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid send Token ID: {e}");
                        exit(2);
                    }
                };
                let receive_token_id = match TokenId::from_str(&receive_token) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid receive Token ID: {e}");
                        exit(2);
                    }
                };

                match dww.sign_swap(coin_id, value, token_id, receive_value, receive_token_id).await {
                    Ok(h) => println!("{}", h.to_json()),
                    Err(e) => {
                        eprintln!("Failed to sign swap: {e}");
                        exit(2);
                    }
                };
                Ok(())
            }
        },


        Subcmd::AttachFee => {
            let mut tx = parse_tx_from_stdin().await?;

            // using pre-constructed dww
            if let Err(e) = dww.attach_fee(&mut tx, 0).await {
                eprintln!("Failed to attach the fee call to the transaction: {e}");
                exit(2);
            };

            println!("{}", base64::encode(&serialize_async(&tx).await));

            dww.stop_rpc_client().await
        }

        Subcmd::TxFromCalls { calls_map } => {
            // Parse calls
            let calls = parse_calls_from_stdin().await?;
            if calls.is_empty() {
                eprintln!("No calls were parsed");
                exit(1);
            }

            // If there is a given map, parse it, otherwise construct a
            // linear map.
            let calls_map = match calls_map {
                Some(cmap) => match parse_tree(&cmap) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Failed parsing calls map: {e}");
                        exit(1);
                    }
                },
                None => {
                    let mut calls_map = Vec::with_capacity(calls.len());
                    for (i, _) in calls.iter().enumerate() {
                        calls_map.push((i, vec![]));
                    }
                    calls_map
                }
            };

            if calls_map.len() != calls.len() {
                eprintln!("Calls map size not equal to parsed calls");
                exit(1);
            }

            // Create a transaction from the mapped calls
            let (mut tx_builder, signature_secrets) = tx_from_calls_mapped(&calls, &calls_map)?;

            // Now build the fee-less tx
            let mut tx = tx_builder.build()?;
            for secrets in &signature_secrets {
                tx.signatures.push(tx.create_sigs(secrets)?);
            }

            // Attach its fee and grab its signature
            // using pre-constructed dww
            if let Err(e) = dww.attach_fee(&mut tx, 0).await {
                eprintln!("Failed to attach the fee call to the transaction: {e}");
                exit(2);
            };
            // Its safe to unwrap here since we know the fee signature
            // is in the last position.
            let fee_signature = tx.signatures.last().unwrap().clone();

            // Re-sign the tx using the calls secrets
            tx.signatures = vec![];
            for secrets in &signature_secrets {
                tx.signatures.push(tx.create_sigs(secrets)?);
            }
            tx.signatures.push(fee_signature);

            println!("{}", base64::encode(&serialize_async(&tx).await));

            dww.stop_rpc_client().await
        }

        Subcmd::Inspect => {
            let tx = parse_tx_from_stdin().await?;

            println!("{}", pretty_tx(&tx));

            Ok(())
        }

        Subcmd::Broadcast => {
            let tx = parse_tx_from_stdin().await?;

            // using pre-constructed dww

            if let Err(e) = dww.simulate_tx(&tx).await {
                eprintln!("Failed to simulate tx: {e}");
                exit(2);
            };

            let mut output = vec![];
            if let Err(e) = dww.mark_tx_spend(&tx, &mut output) {
                print_output(&output);
                eprintln!("Failed to mark transaction coins as spent: {e}");
                exit(2);
            };

            let txid = match dww.broadcast_tx(&tx, &mut output).await {
                Ok(t) => t,
                Err(e) => {
                    print_output(&output);
                    eprintln!("Failed to broadcast transaction: {e}");
                    exit(2);
                }
            };
            print_output(&output);

            println!("Transaction ID: {txid}");

            dww.stop_rpc_client().await
        }

        Subcmd::Scan { reset } => {
            // using pre-constructed dww

            if let Some(height) = reset {
                let mut buf = vec![];
                if let Err(e) = dww.reset_to_height(height, &mut buf) {
                    print_output(&buf);
                    eprintln!("Failed during wallet reset: {e}");
                    exit(2);
                }
                print_output(&buf);
            }

            if let Err(e) = dww.scan_blocks(&mut vec![], None, &true).await {
                eprintln!("Failed during scanning: {e}");
                exit(2);
            }
            println!("Finished scanning blockchain");

            dww.stop_rpc_client().await
        }

        Subcmd::Explorer { command } => match command {
            ExplorerSubcmd::FetchTx { tx_hash, encode } => {
                let tx_hash = TransactionHash(*blake3::Hash::from_hex(&tx_hash)?.as_bytes());


                let tx = match dww.get_tx(&tx_hash).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to fetch transaction: {e}");
                        exit(2);
                    }
                };

                let Some(tx) = tx else {
                    println!("Transaction was not found");
                    exit(1);
                };

                // Make sure the tx is correct
                assert_eq!(tx.hash(), tx_hash);

                if encode {
                    println!("{}", base64::encode(&serialize_async(&tx).await));
                    exit(1)
                }

                println!("Transaction ID: {tx_hash}");
                println!("{tx:?}");

                dww.stop_rpc_client().await
            }

            ExplorerSubcmd::SimulateTx => {
                let tx = parse_tx_from_stdin().await?;


                let is_valid = match dww.simulate_tx(&tx).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Failed to simulate tx: {e}");
                        exit(2);
                    }
                };

                println!("Transaction ID: {}", tx.hash());
                println!("State: {}", if is_valid { "valid" } else { "invalid" });

                dww.stop_rpc_client().await
            }

            ExplorerSubcmd::TxsHistory { tx_hash, encode } => {

                if let Some(c) = tx_hash {
                    let (tx_hash, status, block_height, tx) = dww.get_tx_history_record(&c).await?;

                    if encode {
                        println!("{}", base64::encode(&serialize_async(&tx).await));
                        exit(1)
                    }

                    println!("Transaction ID: {tx_hash}");
                    println!("Status: {status}");
                    match block_height {
                        Some(block_height) => println!("Block height: {block_height}"),
                        None => println!("Block height: -"),
                    }
                    println!("{tx:?}");

                    return Ok(())
                }

                let map = match dww.get_txs_history() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to retrieve transactions history records: {e}");
                        exit(2);
                    }
                };

                // Create a prettytable with the new data:
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(row!["Transaction Hash", "Status", "Block Height"]);
                for (txs_hash, status, block_height) in map.iter() {
                    let block_height = match block_height {
                        Some(block_height) => block_height.to_string(),
                        None => String::from("-"),
                    };
                    table.add_row(row![txs_hash, status, block_height]);
                }

                if table.is_empty() {
                    println!("No transactions found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            ExplorerSubcmd::ClearReverted => {

                let mut output = vec![];
                if let Err(e) = dww.remove_reverted_txs(&mut output) {
                    print_output(&output);
                    eprintln!("Failed to remove reverted transactions: {e}");
                    exit(2);
                };
                print_output(&output);

                Ok(())
            }

            ExplorerSubcmd::ScannedBlocks { height } => {

                if let Some(height) = height {
                    let (hash, signing_key) = match dww.get_scanned_block(&height) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("Failed to retrieve scanned block record: {e}");
                            exit(2);
                        }
                    };

                    println!("Height: {height}");
                    println!("Hash: {hash}");
                    println!("Signing key: {signing_key}");

                    return Ok(())
                }

                let map = match dww.get_scanned_block_records() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to retrieve scanned blocks records: {e}");
                        exit(2);
                    }
                };

                let table = prettytable_scanned_blocks(&map);

                if table.is_empty() {
                    println!("No scanned blocks records found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            ExplorerSubcmd::MiningConfig => {
                let (config, recipient, spend_hook, user_data) =
                    parse_mining_config_from_stdin().await?;
                let mut output = vec![];
                display_mining_config(&config, &recipient, &spend_hook, &user_data, &mut output);
                print_output(&output);

                Ok(())
            }
        },

        Subcmd::Alias { command } => match command {
            AliasSubcmd::Add { alias, token } => {
                if alias.chars().count() > 5 {
                    eprintln!("Error: Alias exceeds 5 characters");
                    exit(2);
                }

                let token_id = match TokenId::from_str(token.as_str()) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e}");
                        exit(2);
                    }
                };

                let mut output = vec![];
                if let Err(e) = dww.add_alias(alias, token_id, &mut output) {
                    print_output(&output);
                    eprintln!("Failed to add alias: {e}");
                    exit(2);
                }
                print_output(&output);

                Ok(())
            }

            AliasSubcmd::Show { alias, token } => {
                let token_id = match token {
                    Some(t) => match TokenId::from_str(t.as_str()) {
                        Ok(t) => Some(t),
                        Err(e) => {
                            eprintln!("Invalid Token ID: {e}");
                            exit(2);
                        }
                    },
                    None => None,
                };

                let map = dww.get_aliases(alias, token_id)?;

                let table = prettytable_aliases(&map);

                if table.is_empty() {
                    println!("No aliases found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            AliasSubcmd::Remove { alias } => {
                let mut output = vec![];
                if let Err(e) = dww.remove_alias(alias, &mut output) {
                    print_output(&output);
                    eprintln!("Failed to remove alias: {e}");
                    exit(2);
                }
                print_output(&output);

                Ok(())
            }
        },

        Subcmd::Token { command } => match command {
            TokenSubcmd::Import { secret_key, token_blind } => {
                let mint_authority = match SecretKey::from_str(&secret_key) {
                    Ok(ma) => ma,
                    Err(e) => {
                        eprintln!("Invalid mint authority: {e}");
                        exit(2);
                    }
                };

                let token_blind = match BaseBlind::from_str(&token_blind) {
                    Ok(tb) => tb,
                    Err(e) => {
                        eprintln!("Invalid token blind: {e}");
                        exit(2);
                    }
                };

                let token_id = dww.import_mint_authority(mint_authority, token_blind).await?;
                println!("Successfully imported mint authority for token ID: {}", token_id.to_string());

                Ok(())
            }

            TokenSubcmd::GenerateMint => {
                let mint_authority = SecretKey::random(&mut OsRng);
                let token_blind = BaseBlind::random(&mut OsRng);
                let token_id = dww.import_mint_authority(mint_authority, token_blind).await?;
                println!("Successfully imported mint authority for token ID: {}", token_id.to_string());

                Ok(())
            }

            TokenSubcmd::Create { name, supply, decimals } => {

                let supply = match supply.parse::<u64>() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Invalid supply amount: {e}");
                        exit(2);
                    }
                };

                let decimals = decimals.unwrap_or(8);

                let tx = match dww.create_token(name, supply, decimals).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create token: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));
                dww.stop_rpc_client().await
            }

            TokenSubcmd::List => {
                let tokens = dww.get_mint_authorities()?;
                let aliases_map = match dww.get_aliases_mapped_by_token() {
                    Ok(map) => map,
                    Err(e) => {
                        eprintln!("Failed to fetch wallet aliases: {e}");
                        exit(2);
                    }
                };

                let table = prettytable_tokenlist(&tokens, &aliases_map);

                if table.is_empty() {
                    println!("No tokens found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            TokenSubcmd::Mint { token, amount, recipient, spend_hook, user_data } => {

                if let Err(e) = f64::from_str(&amount) {
                    eprintln!("Invalid amount: {e}");
                    exit(2);
                }

                let rcpt = match Address::from_str(&recipient) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Invalid recipient: {e}");
                        exit(2);
                    }
                };

                if rcpt.network() != dww.network {
                    eprintln!("Recipient address prefix mismatch");
                    exit(2);
                }

                let token_id = match dww.get_token(token) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("Invalid Token ID: {e}");
                        exit(2);
                    }
                };

                let _spend_hook = match spend_hook {
                    Some(s) => match FuncId::from_str(&s) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            eprintln!("Invalid spend hook: {e}");
                            exit(2);
                        }
                    },
                    None => None,
                };

                let _user_data = match user_data {
                    Some(u) => {
                        let bytes: [u8; 32] = match bs58::decode(&u).into_vec()?.try_into() {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("Invalid user data: {e:?}");
                                exit(2);
                            }
                        };

                        pallas::Base::from_repr(bytes).into_option()
                    }
                    None => None,
                };

                let mint_authority = match dww.get_mint_authority_for_token(&token_id) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("Failed to get mint authority: {e}");
                        exit(2);
                    }
                };

                // TODO: token_leaf_pos and token_path from token registry tree
                let tx = match dww
                    .mint_tokens(token_id, &amount, mint_authority, 0, vec![], Some(*rcpt.public_key()))
                    .await
                {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Failed to create token mint transaction: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                dww.stop_rpc_client().await
            }

        },

        Subcmd::Contract { command } => match command {
            ContractSubcmd::GenerateDeploy => {

                let mut output = vec![];
                if let Err(e) = dww.deploy_auth_keygen(&mut output) {
                    print_output(&output);
                    eprintln!("Error creating deploy auth keypair: {e}");
                    exit(2);
                }
                print_output(&output);

                Ok(())
            }

            ContractSubcmd::List { contract_id } => {

                if let Some(contract_id) = contract_id {
                    let _contract_id = match ContractId::from_str(&contract_id) {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("Invalid contract id: {e}");
                            exit(2);
                        }
                    };

                    let history = dww.get_deploy_auth_history()?;

                    let table = prettytable_contract_history(&history);

                    if table.is_empty() {
                        println!("No history records found");
                    } else {
                        println!("{table}");
                    }

                    return Ok(())
                }

                let auths = dww.list_deploy_auth()?;

                let table = prettytable_contract_auth(&auths);

                if table.is_empty() {
                    println!("No deploy authorities found");
                } else {
                    println!("{table}");
                }

                Ok(())
            }

            ContractSubcmd::ExportData { tx_hash } => {

                let pair = dww.get_deploy_history_record_data(&tx_hash)?;

                println!("{}", base64::encode(&serialize_async(&pair).await));

                Ok(())
            }

            ContractSubcmd::Deploy { deploy_auth, wasm_path, deploy_ix } => {
                // Parse the deploy authority secret key from hex
                let secret_bytes = match hex::decode(&deploy_auth) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Invalid deploy authority hex: {}", e);
                        exit(2);
                    }
                };
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&secret_bytes);
                let deploy_auth = match dwow_sdk::crypto::SecretKey::from_bytes(bytes) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Invalid deploy authority secret key: {}", e);
                        exit(2);
                    }
                };

                // Reconstruct keypair and derive contract ID for validation
                let keypair = Keypair::new(deploy_auth);
                let _contract_id = ContractId::derive_public(keypair.public);

                // Read the wasm bincode and deploy instruction
                let wasm_bin = smol::fs::read(expand_path(&wasm_path)?).await?;
                let deploy_ix = match deploy_ix {
                    Some(p) => smol::fs::read(expand_path(&p)?).await?,
                    None => vec![],
                };


                let tx = match dww.deploy_contract(&keypair, wasm_bin, deploy_ix).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Error creating contract deployment tx: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                dww.stop_rpc_client().await
            }

            ContractSubcmd::Lock { deploy_auth } => {
                // Parse the deployment authority contract id
                let deploy_auth = match ContractId::from_str(&deploy_auth) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("Invalid deploy authority: {e}");
                        exit(2);
                    }
                };


                let _tx = match dww.lock_contract(deploy_auth, 0, &mut vec![]) {
                    Ok(_) => {},
                    Err(e) => {
                        eprintln!("Error creating contract lock tx: {e}");
                        exit(2);
                    }
                };

                println!("lock contract created successfully");

                dww.stop_rpc_client().await
            }

            ContractSubcmd::Invoke { contract_id, function, params } => {
                // Read params from JSON file if provided
                let params_json = match params {
                    Some(p) => {
                        let contents = smol::fs::read(expand_path(&p)?).await?;
                        match String::from_utf8(contents) {
                            Ok(s) => Some(s),
                            Err(e) => {
                                eprintln!("Invalid UTF-8 in params file: {}", e);
                                exit(2);
                            }
                        }
                    }
                    None => None,
                };


                let tx = match dww.invoke_contract(&contract_id, &function, params_json.as_deref(), vec![]).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Error creating contract invocation tx: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                dww.stop_rpc_client().await
            }

            ContractSubcmd::DaoEscrowInit { dao_bulla, endowment_token_id, owner_pubkey, bulla_blind, enable_drain_protection } => {
                use dwow_sdk::pasta::pallas;

                // Parse DAO bulla (use Base::zero() for standalone)
                let dao_bulla = if dao_bulla.to_lowercase() == "zero" {
                    pallas::Base::zero()
                } else {
                    let bytes = bs58::decode(&dao_bulla)
                        .into_vec()
                        .map_err(|e| Error::Custom(format!("Invalid dao_bulla: {}", e)))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid dao_bulla length".to_string()))?;
                    pallas::Base::from_repr(bytes)
                        .into_option()
                        .ok_or_else(|| Error::Custom("Invalid dao_bulla".to_string()))?
                };

                // Parse optional owner public key
                let owner_pubkey = match &owner_pubkey {
                    Some(pk) if pk != "" => {
                        let bytes = bs58::decode(pk)
                            .into_vec()
                            .map_err(|e| Error::Custom(format!("Invalid owner_pubkey: {}", e)))?
                            .try_into()
                            .map_err(|_| Error::Custom("Invalid owner_pubkey length".to_string()))?;
                        Some(PublicKey::from_bytes(bytes)
                            .map_err(|_| Error::Custom("Invalid owner_pubkey".to_string()))?)
                    }
                    _ => None,
                };

                // Parse endowment token ID
                let endowment_token_id = {
                    let bytes = bs58::decode(&endowment_token_id)
                        .into_vec()
                        .map_err(|e| Error::Custom(format!("Invalid endowment_token_id: {}", e)))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid endowment_token_id length".to_string()))?;
                    pallas::Base::from_repr(bytes)
                        .into_option()
                        .ok_or_else(|| Error::Custom("Invalid endowment_token_id".to_string()))?
                };

                // Parse optional bulla blind
                let bulla_blind = match &bulla_blind {
                    Some(bb) if bb != "" => {
                        let bytes = bs58::decode(bb)
                            .into_vec()
                            .map_err(|e| Error::Custom(format!("Invalid bulla_blind: {}", e)))?
                            .try_into()
                            .map_err(|_| Error::Custom("Invalid bulla_blind length".to_string()))?;
                        Some(pallas::Base::from_repr(bytes)
                            .into_option()
                            .ok_or_else(|| Error::Custom("Invalid bulla_blind".to_string()))?)
                    }
                    _ => None,
                };


                let tx = match dww.dao_escrow_initialize(
                    dao_bulla,
                    owner_pubkey,
                    endowment_token_id,
                    bulla_blind,
                    enable_drain_protection,
                ).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Error creating dao_escrow_initialize tx: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                dww.stop_rpc_client().await
            }

            ContractSubcmd::DrainProtectionInit { fund_id, spend_authority, dao_escrow_bulla, rate_limit_bps, vote_threshold_bps } => {
                use dwow_sdk::pasta::pallas;

                // Parse fund ID
                let fund_id = {
                    let bytes = bs58::decode(&fund_id)
                        .into_vec()
                        .map_err(|e| Error::Custom(format!("Invalid fund_id: {}", e)))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid fund_id length".to_string()))?;
                    pallas::Base::from_repr(bytes)
                        .into_option()
                        .ok_or_else(|| Error::Custom("Invalid fund_id".to_string()))?
                };

                // Parse spend authority public key
                let spend_authority = {
                    let bytes = bs58::decode(&spend_authority)
                        .into_vec()
                        .map_err(|e| Error::Custom(format!("Invalid spend_authority: {}", e)))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid spend_authority length".to_string()))?;
                    PublicKey::from_bytes(bytes)
                        .map_err(|_| Error::Custom("Invalid spend_authority".to_string()))?
                };

                // Parse DAO-Escrow bulla
                let dao_escrow_bulla = {
                    let bytes = bs58::decode(&dao_escrow_bulla)
                        .into_vec()
                        .map_err(|e| Error::Custom(format!("Invalid dao_escrow_bulla: {}", e)))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid dao_escrow_bulla length".to_string()))?;
                    pallas::Base::from_repr(bytes)
                        .into_option()
                        .ok_or_else(|| Error::Custom("Invalid dao_escrow_bulla".to_string()))?
                };

                // Default rate and vote thresholds if not provided
                let rate_limit_bps = rate_limit_bps.unwrap_or(100); // 1% default
                let vote_threshold_bps = vote_threshold_bps.unwrap_or(667); // 66.7% default


                let tx = match dww.drain_protection_initialize(
                    fund_id,
                    spend_authority,
                    dao_escrow_bulla,
                    rate_limit_bps,
                    vote_threshold_bps,
                ) {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Error creating drain_protection_initialize tx: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                dww.stop_rpc_client().await
            }

            ContractSubcmd::EnableDrainProtection { dao_escrow_bulla, drain_protection_bulla } => {
                use dwow_sdk::pasta::pallas;

                // Parse DAO-Escrow bulla
                let dao_escrow_bulla = {
                    let bytes = bs58::decode(&dao_escrow_bulla)
                        .into_vec()
                        .map_err(|e| Error::Custom(format!("Invalid dao_escrow_bulla: {}", e)))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid dao_escrow_bulla length".to_string()))?;
                    pallas::Base::from_repr(bytes)
                        .into_option()
                        .ok_or_else(|| Error::Custom("Invalid dao_escrow_bulla".to_string()))?
                };

                // Parse DrainProtection bulla
                let drain_protection_bulla = {
                    let bytes = bs58::decode(&drain_protection_bulla)
                        .into_vec()
                        .map_err(|e| Error::Custom(format!("Invalid drain_protection_bulla: {}", e)))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid drain_protection_bulla length".to_string()))?;
                    pallas::Base::from_repr(bytes)
                        .into_option()
                        .ok_or_else(|| Error::Custom("Invalid drain_protection_bulla".to_string()))?
                };


                let tx = match dww.dao_escrow_enable_drain_protection(
                    dao_escrow_bulla,
                    drain_protection_bulla,
                ).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        eprintln!("Error creating dao_escrow_enable_drain_protection tx: {e}");
                        exit(2);
                    }
                };

                println!("{}", base64::encode(&serialize_async(&tx).await));

                dww.stop_rpc_client().await
            }

            ContractSubcmd::Register { contract_name, contract_id } => {
                let cid = match ContractId::from_str(&contract_id) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Invalid contract ID: {}", e);
                        exit(2);
                    }
                };


                if let Err(e) = dww.register_contract_id(&contract_name, cid) {
                    eprintln!("Failed to register contract ID: {}", e);
                    exit(2);
                }
                println!("Registered {} = {}", contract_name, contract_id);
                Ok(())
            }
        },

        Subcmd::Mine => {
            // using pre-constructed dww

            // Get default address for mining rewards
            let public_key = match dww.default_address() {
                Ok(pk) => pk,
                Err(e) => {
                    eprintln!("Failed to get default address: {e}");
                    exit(2);
                }
            };
            let recipient: Address =
                StandardAddress::from_public(dww.network, *public_key.public_key()).into();

            println!("Mining blocks to {}...", recipient);
            println!("Press Ctrl+C to stop mining");
            if let Err(e) = dww.miner_mine(&recipient.to_string()).await {
                eprintln!("Mining error: {}", e);
                exit(2);
            }
            dww.stop_rpc_client().await
        }

        Subcmd::Position { json } => {

            use dwow_wallet::capability::CapabilityResolver;

            let mut resolver = CapabilityResolver::new();

            // Promissory Note — foundational contract for all coins
            if let Some(cid) = dwow_wallet::contract_imports::PROMISSORY_NOTE_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_promissory_note_contract::capability::descriptor(*cid));
            }

            // Phase 1: escrow
            if let Some(cid) = dwow_wallet::contract_imports::ESCROW_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_escrow_contract::capability::descriptor(*cid));
            }
            // otc_swap — same architecture pattern as escrow
            if let Some(cid) = dwow_wallet::contract_imports::OTC_SWAP_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_otc_swap_contract::capability::descriptor(*cid));
            }
            // Phase 2: simple single-role contracts
            if let Some(cid) = dwow_wallet::contract_imports::AUCTION_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_auction_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::SUBSCRIPTION_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_subscription_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::BACCARAT_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_baccarat_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::DARKTOSHI_DICE_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_darktoshi_dice_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::SLOT_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_slot_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::ROULETTE_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_roulette_contract::capability::descriptor(*cid));
            }
            // Phase 3: dex
            if let Some(cid) = dwow_wallet::contract_imports::DEX_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_dex_contract::capability::descriptor(*cid));
            }
            // Phase 4: multi-role contracts
            if let Some(cid) = dwow_wallet::contract_imports::DAO_ESCROW_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_dao_escrow_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::DRAIN_PROTECTION_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_drain_protection_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::BETTING_STAKE_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_betting_stake_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::BEARER_BOND_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_bearer_bond_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::POOL_STAKE_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_pool_stake_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::RELAYER_ENDOWMENT_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_relayer_endowment_contract::capability::descriptor(*cid));
            }
            // Phase 5: complex multi-entity contracts
            if let Some(cid) = dwow_wallet::contract_imports::DARKBET_EXCHANGE_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_darkbet_exchange_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::GAME_ROOM_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_game_room_contract::capability::descriptor(*cid));
            }
            if let Some(cid) = dwow_wallet::contract_imports::LOTTERY_CONTRACT_ID.get() {
                resolver.register_descriptor(dwow_lottery_contract::capability::descriptor(*cid));
            }

            let position = resolver.resolve(&dww.wallet, &dww.cache);

            if json {
                // Machine-parseable JSON output for L4 cross-implementation tests.
                let coin_caps: Vec<&dwow_sdk::capability::Capability> = position
                    .capabilities
                    .iter()
                    .filter(|c| matches!(c.source, dwow_sdk::capability::CapabilitySource::Coin { .. }))
                    .collect();
                let generic_caps: Vec<&dwow_sdk::capability::Capability> = position
                    .capabilities
                    .iter()
                    .filter(|c| matches!(c.source, dwow_sdk::capability::CapabilitySource::Generic { .. }))
                    .collect();
                let role_caps: Vec<&dwow_sdk::capability::Capability> = position
                    .capabilities
                    .iter()
                    .filter(|c| matches!(c.source, dwow_sdk::capability::CapabilitySource::Role { .. }))
                    .collect();

                let capability_descriptions: Vec<&str> = position
                    .capabilities
                    .iter()
                    .map(|c| c.description.as_str())
                    .collect();
                let action_names: Vec<&str> = position
                    .available_actions
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect();

                let output = serde_json::json!({
                    "capability_count": position.capabilities.len(),
                    "action_count": position.available_actions.len(),
                    "coin_count": coin_caps.len(),
                    "generic_count": generic_caps.len(),
                    "role_count": role_caps.len(),
                    "capability_descriptions": capability_descriptions,
                    "action_names": action_names,
                    "descriptors_loaded": resolver.descriptors().len(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
                return Ok(());
            }

            // Display capabilities
            if position.capabilities.is_empty() {
                println!("No capabilities held.");
            } else {
                println!("=== Held Capabilities ===");
                for cap in &position.capabilities {
                    let consumed = if cap.consumable { " [consumable]" } else { "" };
                    let expires = match cap.expires_at {
                        Some(h) => format!(" [expires: block {}]", h),
                        None => String::new(),
                    };
                    println!("  {} — {}{}{}", cap.id, cap.description, consumed, expires);
                }
            }

            // Display available actions
            if position.available_actions.is_empty() {
                println!("No actions available.");
            } else {
                println!("\n=== Available Actions ===");
                for action in &position.available_actions {
                    println!("  {}::{} (0x{:02x}) — {}",
                        resolver.descriptors().values()
                            .find(|d| d.contract_id == action.contract_id)
                            .map(|d| d.name.as_str())
                            .unwrap_or("unknown"),
                        action.name,
                        action.function_id,
                        action.description,
                    );
                }
            }

            println!("\nDescriptors loaded: {}", resolver.descriptors().len());
            Ok(())
        }
    }
}
