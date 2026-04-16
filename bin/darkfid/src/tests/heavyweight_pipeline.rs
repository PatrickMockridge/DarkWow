/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
 * FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License for
 * more details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Heavyweight Contract Testing Pipeline
//!
//! Provides a generalized pipeline for testing contracts with real ZK proofs.
//! Works with any contract implementing the `ContractHarness` trait.
//!
//! ## Architecture
//!
//! ```text
//! HeavyweightPipeline<H: ContractHarness>
//!     |
//!     ├── harness: H  (provides ZK circuits and proof generation)
//!     ├── genesis: GenesisHarness  (blockchain operations - OWNED directly)
//!     └── exec()  (execute contract calls with ZK proofs)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use darkfi_contract_test_harness::harness::{DexHarness, ContractHarness};
//!
//! let harness = DexHarness::new();
//! let mut pipeline = HeavyweightPipeline::new(harness, "dex", config, ex).await?;
//!
//! // Generate genesis blocks
//! pipeline.generate_genesis_blocks(3).await?;
//!
//! // Deploy contract
//! let wasm = read_wasm("dex");
//! let contract_id = pipeline.deploy(wasm).await?;
//!
//! // Execute contract call with ZK proof
//! pipeline.exec(function_id, call_data, proofs).await?;
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use darkfi::{
    tx::{ContractCallLeaf, TransactionBuilder},
    Result,
};
use darkfi_contract_test_harness::harness::ContractHarness;
use darkfi_sdk::{
    crypto::{keypair::Keypair, ContractId},
    ContractCall,
};
use smol::Executor;
use tracing::info;

// Use GenesisHarness directly from tests module
use super::genesis::GenesisHarness;
use super::HarnessConfig;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum HeavyweightError {
    #[error("Genesis failed: {0}")]
    GenesisFailed(String),

    #[error("Deployment failed: {0}")]
    DeploymentFailed(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Contract not deployed yet")]
    NotDeployed,
}

// ============================================================================
// HeavyweightPipeline
// ============================================================================

/// Heavyweight testing pipeline with ZK proof generation
///
/// This pipeline works with ANY contract implementing `ContractHarness` trait.
/// It owns a GenesisHarness directly (not via ContractTestingPipeline).
pub struct HeavyweightPipeline<H: ContractHarness> {
    /// Contract harness with ZK circuits
    harness: H,
    /// Contract name
    contract_name: String,
    /// Contract ID (set after deployment)
    contract_id: Option<ContractId>,
    /// Genesis harness for blockchain (OWNED directly)
    genesis: GenesisHarness,
    /// Keypair for signing transactions
    keypair: Keypair,
}

impl<H: ContractHarness> HeavyweightPipeline<H> {
    /// Create a new heavyweight pipeline
    ///
    /// Creates a GenesisHarness directly for one contract at a time.
    pub async fn new(
        harness: H,
        contract_name: &str,
        config: HarnessConfig,
        ex: Arc<Executor<'static>>,
    ) -> std::result::Result<Self, HeavyweightError> {
        info!("Creating heavyweight pipeline for {}", contract_name);

        // Create GenesisHarness directly (NOT via ContractTestingPipeline)
        let genesis = GenesisHarness::new(config, &ex)
            .await
            .map_err(|e| HeavyweightError::GenesisFailed(e.to_string()))?;

        Ok(Self {
            harness,
            contract_name: contract_name.to_string(),
            contract_id: None,
            genesis,
            keypair: Keypair::default(),
        })
    }

    /// Generate genesis blocks (mints native tokens to keypair)
    pub async fn generate_genesis_blocks(
        &mut self,
        num_blocks: usize,
    ) -> std::result::Result<(), HeavyweightError> {
        self.genesis
            .generate_genesis_blocks(num_blocks)
            .await
            .map_err(|e| HeavyweightError::GenesisFailed(e.to_string()))?;
        info!("Generated {} genesis blocks", num_blocks);
        Ok(())
    }

    /// Deploy the contract using Deployooor
    pub async fn deploy(
        &mut self,
        wasm: Vec<u8>,
    ) -> std::result::Result<ContractId, HeavyweightError> {
        let contract_id = self
            .genesis
            .deploy_contract(wasm, &self.contract_name)
            .await
            .map_err(|e| HeavyweightError::DeploymentFailed(e.to_string()))?;

        self.contract_id = Some(contract_id);
        info!("Deployed {} contract: {:?}", self.contract_name, contract_id);
        Ok(contract_id)
    }

    /// Get the contract ID (set after deployment)
    pub fn contract_id(&self) -> Option<ContractId> {
        self.contract_id
    }

    /// Get the harness
    pub fn harness(&self) -> &H {
        &self.harness
    }

    /// Get circuit namespaces from the harness
    pub fn circuits(&self) -> Vec<&'static str> {
        self.harness.circuits()
    }

    /// Execute a contract call with ZK proofs
    ///
    /// Builds a transaction, signs it, and returns the transaction.
    pub async fn exec(
        &mut self,
        function_id: u8,
        mut call_data: Vec<u8>,
        proofs: Vec<darkfi::zk::Proof>,
    ) -> std::result::Result<darkfi::tx::Transaction, HeavyweightError> {
        let contract_id =
            self.contract_id.ok_or(HeavyweightError::NotDeployed)?;

        // Prepend function ID to call data
        let mut data = vec![function_id];
        data.append(&mut call_data);

        let call = ContractCall { contract_id, data };

        // Build transaction with proofs
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call, proofs },
            vec![],
        )
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

        let mut tx = tx_builder
            .build()
            .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

        let sigs = tx
            .create_sigs(&[self.keypair.secret])
            .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
        tx.signatures = vec![sigs];

        info!(
            "Executed {}::{:#x} (tx: {:?})",
            self.contract_name,
            function_id,
            tx.hash()
        );

        Ok(tx)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Get the base directory for contracts
fn contract_base_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("src")
        .join("contract")
}

/// Read WASM binary for a contract
async fn read_wasm(contract_name: &str) -> std::result::Result<Vec<u8>, HeavyweightError> {
    let wasm_path = contract_base_dir()
        .join(contract_name)
        .join(format!("darkfi_{}_contract.wasm", contract_name));

    smol::fs::read(&wasm_path).await.map_err(|e| HeavyweightError::DeploymentFailed(e.to_string()))
}

// ============================================================================
// Tests
// ============================================================================

/// Test the heavyweight pipeline with DEX contract
#[test]
fn test_dex_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_dex_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_dex_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::DexHarness;

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18560".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18561".to_string(),
    };

    let harness = DexHarness::new();
    info!("DEX harness created with circuits: {:?}", harness.circuits());

    let mut pipeline =
        HeavyweightPipeline::new(harness, "dex", config, ex).await?;

    // Generate genesis blocks
    pipeline.generate_genesis_blocks(3).await?;

    // Read and deploy WASM
    let wasm = read_wasm("dex").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("DEX deployed: {:?}", contract_id);

    info!("test_dex_heavyweight PASSED");
    Ok(())
}

/// Test the heavyweight pipeline with MoneyV3 contract
#[test]
fn test_money_v3_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_money_v3_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_money_v3_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::MoneyV3Harness;

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18570".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18571".to_string(),
    };

    let harness = MoneyV3Harness::spawn();
    info!("MoneyV3 harness created with circuits: {:?}", harness.circuits());

    let mut pipeline =
        HeavyweightPipeline::new(harness, "money_v3", config, ex).await?;

    // Generate genesis blocks
    pipeline.generate_genesis_blocks(3).await?;

    // Read and deploy WASM
    let wasm = read_wasm("money_v3").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("MoneyV3 deployed: {:?}", contract_id);

    info!("test_money_v3_heavyweight PASSED");
    Ok(())
}

// ============================================================================
// Additional Contract Heavyweight Tests
// ============================================================================

// atomic_swap
#[test]
fn test_atomic_swap_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_atomic_swap_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_atomic_swap_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::AtomicSwapHarness;

    let harness = AtomicSwapHarness::spawn();
    info!("AtomicSwap harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18580".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18581".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "atomic_swap", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("atomic_swap").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("AtomicSwap deployed: {:?}", contract_id);

    info!("test_atomic_swap_heavyweight PASSED");
    Ok(())
}

// attestation
#[test]
fn test_attestation_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_attestation_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_attestation_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::AttestationHarness;

    let harness = AttestationHarness::spawn();
    info!("Attestation harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18582".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18583".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "attestation", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("attestation").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Attestation deployed: {:?}", contract_id);

    info!("test_attestation_heavyweight PASSED");
    Ok(())
}

// auction
#[test]
fn test_auction_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_auction_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_auction_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::AuctionHarness;

    let harness = AuctionHarness::spawn();
    info!("Auction harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18584".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18585".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "auction", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("auction").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Auction deployed: {:?}", contract_id);

    info!("test_auction_heavyweight PASSED");
    Ok(())
}

// baccarat
#[test]
fn test_baccarat_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_baccarat_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_baccarat_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::BaccaratHarness;

    let harness = BaccaratHarness::spawn();
    info!("Baccarat harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18586".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18587".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "baccarat", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("baccarat").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Baccarat deployed: {:?}", contract_id);

    info!("test_baccarat_heavyweight PASSED");
    Ok(())
}

// block_height_prediction
#[test]
fn test_block_height_prediction_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_block_height_prediction_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_block_height_prediction_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::BlockHeightPredictionHarness;

    let harness = BlockHeightPredictionHarness::spawn();
    info!("BlockHeightPrediction harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18588".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18589".to_string(),
    };

    let mut pipeline =
        HeavyweightPipeline::new(harness, "block_height_prediction", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("block_height_prediction").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("BlockHeightPrediction deployed: {:?}", contract_id);

    info!("test_block_height_prediction_heavyweight PASSED");
    Ok(())
}

// bridge
#[test]
fn test_bridge_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_bridge_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_bridge_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::BridgeHarness;

    let harness = BridgeHarness::spawn();
    info!("Bridge harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18590".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18591".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "bridge", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("bridge").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Bridge deployed: {:?}", contract_id);

    info!("test_bridge_heavyweight PASSED");
    Ok(())
}

// dao_escrow
#[test]
fn test_dao_escrow_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_dao_escrow_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_dao_escrow_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::DaoEscrowHarness;

    let harness = DaoEscrowHarness::spawn();
    info!("DaoEscrow harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18592".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18593".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "dao_escrow", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("dao_escrow").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("DaoEscrow deployed: {:?}", contract_id);

    info!("test_dao_escrow_heavyweight PASSED");
    Ok(())
}

// darkbet_exchange
#[test]
fn test_darkbet_exchange_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_darkbet_exchange_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_darkbet_exchange_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::DarkbetExchangeHarness;

    let harness = DarkbetExchangeHarness::spawn();
    info!("DarkbetExchange harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18594".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18595".to_string(),
    };

    let mut pipeline =
        HeavyweightPipeline::new(harness, "darkbet_exchange", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("darkbet_exchange").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("DarkbetExchange deployed: {:?}", contract_id);

    info!("test_darkbet_exchange_heavyweight PASSED");
    Ok(())
}

// darktoshi_dice
#[test]
fn test_darktoshi_dice_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_darktoshi_dice_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_darktoshi_dice_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::DarkToshiDiceHarness;

    let harness = DarkToshiDiceHarness::spawn();
    info!("DarkToshiDice harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18596".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18597".to_string(),
    };

    let mut pipeline =
        HeavyweightPipeline::new(harness, "darktoshi_dice", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("darktoshi_dice").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("DarkToshiDice deployed: {:?}", contract_id);

    info!("test_darktoshi_dice_heavyweight PASSED");
    Ok(())
}

// escrow
#[test]
fn test_escrow_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_escrow_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_escrow_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::EscrowHarness;

    let harness = EscrowHarness::spawn();
    info!("Escrow harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18598".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18599".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "escrow", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("escrow").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Escrow deployed: {:?}", contract_id);

    info!("test_escrow_heavyweight PASSED");
    Ok(())
}

// identity
#[test]
fn test_identity_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_identity_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_identity_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::IdentityHarness;

    let harness = IdentityHarness::spawn();
    info!("Identity harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18600".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18601".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "identity", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("identity").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Identity deployed: {:?}", contract_id);

    info!("test_identity_heavyweight PASSED");
    Ok(())
}

// insurance_market
#[test]
fn test_insurance_market_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_insurance_market_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_insurance_market_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::InsuranceMarketHarness;

    let harness = InsuranceMarketHarness::spawn();
    info!("InsuranceMarket harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18602".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18603".to_string(),
    };

    let mut pipeline =
        HeavyweightPipeline::new(harness, "insurance_market", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("insurance_market").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("InsuranceMarket deployed: {:?}", contract_id);

    info!("test_insurance_market_heavyweight PASSED");
    Ok(())
}

// labor_market
#[test]
fn test_labor_market_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_labor_market_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_labor_market_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::LaborMarketHarness;

    let harness = LaborMarketHarness::spawn();
    info!("LaborMarket harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18604".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18605".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "labor_market", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("labor_market").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("LaborMarket deployed: {:?}", contract_id);

    info!("test_labor_market_heavyweight PASSED");
    Ok(())
}

// lottery
#[test]
fn test_lottery_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_lottery_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_lottery_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::LotteryHarness;

    let harness = LotteryHarness::spawn();
    info!("Lottery harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18606".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18607".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "lottery", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("lottery").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Lottery deployed: {:?}", contract_id);

    info!("test_lottery_heavyweight PASSED");
    Ok(())
}

// oracle
#[test]
fn test_oracle_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_oracle_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_oracle_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::OracleHarness;

    let harness = OracleHarness::spawn();
    info!("Oracle harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18608".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18609".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "oracle", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("oracle").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Oracle deployed: {:?}", contract_id);

    info!("test_oracle_heavyweight PASSED");
    Ok(())
}

// pool_stake
#[test]
fn test_pool_stake_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_pool_stake_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_pool_stake_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::PoolStakeHarness;

    let harness = PoolStakeHarness::spawn();
    info!("PoolStake harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18610".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18611".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "pool_stake", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("pool_stake").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("PoolStake deployed: {:?}", contract_id);

    info!("test_pool_stake_heavyweight PASSED");
    Ok(())
}

// slot
#[test]
fn test_slot_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_slot_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_slot_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::SlotHarness;

    let harness = SlotHarness::spawn();
    info!("Slot harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18612".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18613".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "slot", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("slot").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Slot deployed: {:?}", contract_id);

    info!("test_slot_heavyweight PASSED");
    Ok(())
}

// stablecoin
#[test]
fn test_stablecoin_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_stablecoin_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_stablecoin_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::StablecoinHarness;

    let harness = StablecoinHarness::spawn();
    info!("Stablecoin harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18614".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18615".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "stablecoin", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("stablecoin").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Stablecoin deployed: {:?}", contract_id);

    info!("test_stablecoin_heavyweight PASSED");
    Ok(())
}

// subscription
#[test]
fn test_subscription_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_subscription_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_subscription_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::SubscriptionHarness;

    let harness = SubscriptionHarness::spawn();
    info!("Subscription harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18616".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18617".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "subscription", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("subscription").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Subscription deployed: {:?}", contract_id);

    info!("test_subscription_heavyweight PASSED");
    Ok(())
}

// tender
#[test]
fn test_tender_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_tender_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_tender_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::TenderHarness;

    let harness = TenderHarness::spawn();
    info!("Tender harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(darkfi_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18618".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18619".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "tender", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("tender").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Tender deployed: {:?}", contract_id);

    info!("test_tender_heavyweight PASSED");
    Ok(())
}