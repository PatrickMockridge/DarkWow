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