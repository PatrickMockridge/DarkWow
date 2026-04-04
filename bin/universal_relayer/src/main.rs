/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Universal Relayer - main entry point

mod chain;
mod config;
mod deployer;
mod error;
mod executors;
mod feed;
mod pool;
mod stake;
mod watcher;

use config::Config;
use darkfi_sdk::crypto::pasta_prelude::PrimeField;
use deployer::CapitalDeployer;
use error::Result;
use executors::ExecutorRegistry;
use feed::FeedManager;
use pool::PoolManager;
use stake::StakeManager;
use std::path::PathBuf;
use structopt::StructOpt;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use watcher::Watcher;
use std::sync::Arc;

#[derive(Debug, StructOpt)]
struct Cli {
    /// Configuration file path
    #[structopt(short = "c", long = "config", default_value = "universal_relayer_config.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[structopt(short = "v", long = "verbose")]
    verbose: bool,

    /// Subcommand
    #[structopt(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Start the relayer
    Start,
    /// Show relayer status
    Status,
    /// Derive a bridge address for a recipient
    DeriveAddress {
        /// Recipient public key X coordinate
        recipient_pub_x: String,
        /// Recipient public key Y coordinate
        recipient_pub_y: String,
        /// Nonce for address derivation
        nonce: u64,
    },
}

fn main() -> Result<()> {
    let args = Cli::from_args();

    // Initialize logging
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    tracing::info!("Universal Relayer starting...");

    let executor = Arc::new(smol::Executor::new());

    smol::block_on(executor.run(async {
        match args.command {
            Some(Command::Start) | None => {
                run_relayer(&args.config, executor.clone()).await
            }
            Some(Command::Status) => {
                show_status(&args.config).await
            }
            Some(Command::DeriveAddress { recipient_pub_x, recipient_pub_y, nonce }) => {
                derive_address(&recipient_pub_x, &recipient_pub_y, nonce)
            }
        }
    }))
}

async fn run_relayer(config_path: &PathBuf, executor: Arc<smol::Executor<'_>>) -> Result<()> {
    // Load configuration
    let config = Config::load(config_path)?;

    // Validate configuration
    let errors = config.validate();
    if !errors.is_empty() {
        tracing::error!("Configuration errors:");
        for e in &errors {
            tracing::error!("  - {}", e);
        }
        return Err(error::RelayerError::Config("Invalid configuration".to_string()).into());
    }

    tracing::info!("Configuration loaded successfully");
    tracing::info!("Enabled chains:");
    if config.is_ethereum_enabled() { tracing::info!("  - Ethereum"); }
    if config.is_monero_enabled() { tracing::info!("  - Monero"); }
    if config.is_zcash_enabled() { tracing::info!("  - Zcash"); }
    if config.is_litecoin_enabled() { tracing::info!("  - Litecoin"); }
    if config.is_aztec_enabled() { tracing::info!("  - Aztec"); }

    // Initialize economic managers
    let mut stake_manager = StakeManager::new(config.stake.clone());
    let feed_manager = FeedManager::new(config.feed.clone(), config.relayer.fee_percentage);
    let pool_manager = PoolManager::new(config.pool.clone());
    let deployer_manager = CapitalDeployer::new(config.capital_deployer.clone());

    // Log economic configuration
    if stake_manager.is_enabled() {
        tracing::info!("Stake coverage enabled: {} total, {} max withdrawal",
            stake_manager.total_stake(), stake_manager.max_withdrawal());
    }

    if let config::FeedMode::Guaranteed { refund_premium_bp } = config.feed {
        tracing::info!("Feed mode: Guaranteed with {}bp refund premium", refund_premium_bp);
    } else {
        tracing::info!("Feed mode: Standard (fee only)");
    }

    if pool_manager.is_enabled() {
        tracing::info!("Pool enabled, ID: {:?}", pool_manager.pool_id());
    }

    if deployer_manager.is_enabled() {
        tracing::info!("Capital deployer enabled");
    }

    // Initialize executor registry
    let executors = ExecutorRegistry::new(&config);
    let enabled_chains = executors.enabled_chains();

    if enabled_chains.is_empty() {
        return Err(error::RelayerError::Config("No chains enabled".to_string()).into());
    }

    tracing::info!("Starting withdrawal watcher...");

    // Initialize watcher
    let mut watcher = Watcher::new(&config.darkfi).await?;

    tracing::info!("Universal Relayer running. Press Ctrl+C to stop.");

    // Main loop
    loop {
        // Get current block height
        let current_height = watcher.get_current_height().await?;
        tracing::debug!("Current block height: {}", current_height);

        // Clean up expired withdrawals from stake tracking
        if stake_manager.is_enabled() {
            stake_manager.cleanup_expired(current_height);
        }

        // Fetch pending withdrawals
        let pending = watcher.get_pending_withdrawals().await?;
        tracing::debug!("Found {} pending withdrawals", pending.len());

        for withdrawal in pending {
            // Check if timed out
            if withdrawal.is_timed_out(current_height) {
                tracing::warn!(
                    "Withdrawal {} has timed out at block {}",
                    hex::encode(&withdrawal.withdrawal_id),
                    withdrawal.timeout_height
                );

                // For guaranteed withdrawals, handle the refund
                if withdrawal.feed_mode == 1 {
                    tracing::info!(
                        "Guaranteed withdrawal {} timed out - user can claim refund",
                        hex::encode(&withdrawal.withdrawal_id)
                    );
                }
                continue;
            }

            // Check stake coverage if enabled
            if stake_manager.is_enabled() {
                if !stake_manager.can_accept(withdrawal.amount) {
                    tracing::warn!(
                        "Withdrawal {} exceeds available stake coverage (have {}, need {})",
                        hex::encode(&withdrawal.withdrawal_id),
                        stake_manager.available_stake(),
                        withdrawal.amount
                    );
                    continue;
                }

                // Lock stake for this withdrawal
                if let Err(e) = stake_manager.lock_for_withdrawal(&withdrawal) {
                    tracing::error!(
                        "Failed to lock stake for {}: {}",
                        hex::encode(&withdrawal.withdrawal_id),
                        e
                    );
                    continue;
                }
            }

            // Price the withdrawal according to feed mode
            let priced = match feed_manager.price_withdrawal(&withdrawal) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        "Failed to price withdrawal {}: {}",
                        hex::encode(&withdrawal.withdrawal_id),
                        e
                    );
                    continue;
                }
            };

            // Get the appropriate executor for this chain
            let chain = withdrawal.get_chain();
            let exec = executors.get_executor(chain);

            if !exec.is_enabled() {
                tracing::warn!("Withdrawal for disabled chain: {}", chain);
                continue;
            }

            tracing::info!(
                "Processing withdrawal: {} {} to {:?} (fee: {}, premium: {})",
                withdrawal.amount,
                chain,
                &withdrawal.recipient_hash,
                priced.relayer_fee,
                priced.guarantee_premium
            );

            // Execute withdrawal
            match exec.execute(&withdrawal).await {
                Ok(tx_hash) => {
                    tracing::info!("Withdrawal executed: {} on {}", tx_hash, chain);

                    // Release locked stake on success
                    if stake_manager.is_enabled() {
                        if let Err(e) = stake_manager.release(&withdrawal.withdrawal_id) {
                            tracing::error!(
                                "Failed to release stake for {}: {}",
                                hex::encode(&withdrawal.withdrawal_id),
                                e
                            );
                        }
                    }

                    watcher.mark_processed(&withdrawal.withdrawal_id);
                }
                Err(e) => {
                    tracing::error!("Failed to execute withdrawal: {}", e);

                    // For guaranteed withdrawals, user gets refund from stake
                    if priced.is_guaranteed() && stake_manager.is_enabled() {
                        tracing::warn!(
                            "Guaranteed withdrawal {} failed - {} stake to be refunded to user",
                            hex::encode(&withdrawal.withdrawal_id),
                            priced.get_refund_amount()
                        );

                        if let Err(e) = stake_manager.slash_for_failure(&withdrawal.withdrawal_id) {
                            tracing::error!(
                                "Failed to slash stake for {}: {}",
                                hex::encode(&withdrawal.withdrawal_id),
                                e
                            );
                        }
                    }
                    // Continue with next withdrawal
                }
            }
        }

        // Sleep until next poll
        smol::Timer::after(std::time::Duration::from_secs(watcher.poll_interval())).await;
    }
}

async fn show_status(config_path: &PathBuf) -> Result<()> {
    let config = Config::load(config_path)?;

    println!("Universal Relayer Status");
    println!("=======================");
    println!();

    println!("DarkFi RPC: {}", config.darkfi.darkfid_url);
    println!("Poll interval: {}s", config.darkfi.poll_interval_secs);
    println!();

    println!("Enabled chains:");
    println!("  Ethereum: {}", if config.is_ethereum_enabled() { "YES" } else { "NO" });
    println!("  Monero:   {}", if config.is_monero_enabled() { "YES" } else { "NO" });
    println!("  Zcash:    {}", if config.is_zcash_enabled() { "YES" } else { "NO" });
    println!("  Litecoin: {}", if config.is_litecoin_enabled() { "YES" } else { "NO" });
    println!("  Aztec:    {}", if config.is_aztec_enabled() { "YES" } else { "NO" });

    Ok(())
}

fn derive_address(recipient_pub_x: &str, recipient_pub_y: &str, nonce: u64) -> Result<()> {
    use darkfi_sdk::{crypto::poseidon_hash, pasta::pallas};

    // Parse the public key coordinates
    let x_bytes = hex::decode(recipient_pub_x)
        .map_err(|e| error::RelayerError::AddressDerivation(format!("Invalid pub_x hex: {}", e)))?;
    let y_bytes = hex::decode(recipient_pub_y)
        .map_err(|e| error::RelayerError::AddressDerivation(format!("Invalid pub_y hex: {}", e)))?;

    // Convert to pallas::Base
    let x: [u8; 32] = x_bytes[..32].try_into().unwrap();
    let y: [u8; 32] = y_bytes[..32].try_into().unwrap();
    let x_base = pallas::Base::from_repr(x.into()).unwrap();
    let y_base = pallas::Base::from_repr(y.into()).unwrap();

    // Use poseidon for ZK-friendly derivation
    let bridge_secret = poseidon_hash([x_base, y_base, pallas::Base::from(nonce)]);
    let bridge_address = poseidon_hash([bridge_secret]);

    println!("Bridge Address: {}", hex::encode(bridge_address.to_repr()));
    println!("Note: In production, chain-specific address encoding should be applied (e.g., 0x prefix for ETH)");

    Ok(())
}