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

//! Contract Deployment Orchestrator
//!
//! Automatically handles:
//! 1. Checking and building contract binaries (WASM + zk.bin)
//! 2. Running genesis if needed
//! 3. Deploying WASM contracts via Deployooor

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use dwow::Result;
use dwow_sdk::{crypto::ContractId, num_traits::One};
use num_bigint::BigUint;
use smol::Executor;
use tracing::info;

// Re-export for tests
pub use crate::tests::genesis::GenesisHarness;
use crate::tests::HarnessConfig;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum DeployerError {
    #[error("WASM binary not found: {0}")]
    WasmNotFound(String),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Genesis failed: {0}")]
    GenesisFailed(String),

    #[error("Contract already deployed: {0}")]
    AlreadyDeployed(String),

    #[error("Deployment failed: {0}")]
    DeploymentFailed(String),
}

// ============================================================================
// Genesis State (simplified - no serde)
// ============================================================================

/// Persisted genesis state (simplified for testing)
#[derive(serde::Serialize, serde::Deserialize)]
pub struct GenesisState {
    /// Block height after genesis
    pub block_height: u32,
    /// Deployed contract IDs as hex strings
    pub deployed_contracts: Vec<String>,
    /// When genesis was run (as unix timestamp)
    pub timestamp_secs: u64,
}

/// Status of genesis setup
pub enum GenesisStatus {
    /// No genesis state exists
    NotStarted,
    /// Genesis has been run and is valid
    Ready(GenesisState),
    /// Genesis state exists but appears corrupted or stale
    Invalid,
}

// ============================================================================
// Contract Deployer
// ============================================================================

/// Orchestrates contract deployment workflow
pub struct ContractDeployer {
    /// Contract name (e.g., "stablecoin", "dex")
    contract_name: String,
    /// Path to WASM binary (computed from contract name)
    wasm_path: PathBuf,
    /// Path to ZK binary directory
    zkbin_dir: PathBuf,
    /// Path to genesis state file
    state_file: PathBuf,
    /// Harness configuration
    harness_config: HarnessConfig,
    /// Executor
    ex: Arc<Executor<'static>>,
    /// Genesis harness instance (persisted to avoid recreation)
    genesis: Option<GenesisHarness>,
}

impl ContractDeployer {
    /// Create a new deployer for the given contract
    pub async fn new(
        contract_name: &str,
        harness_config: HarnessConfig,
        ex: Arc<Executor<'static>>,
    ) -> std::result::Result<Self, DeployerError> {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let wasm_path = base_dir
            .join("..")
            .join("..")
            .join("src")
            .join("contract")
            .join(contract_name)
            .join(format!("darkfi_{}_contract.wasm", contract_name));

        let zkbin_dir = base_dir
            .join("..")
            .join("..")
            .join("src")
            .join("contract")
            .join(contract_name)
            .join("proof");

        let state_file = base_dir.join(".genesis_state.json");

        Ok(Self {
            contract_name: contract_name.to_string(),
            wasm_path,
            zkbin_dir,
            state_file,
            harness_config,
            ex,
            genesis: None,
        })
    }

    /// Check if WASM binary exists
    pub fn check_wasm(&self) -> bool {
        self.wasm_path.exists()
    }

    /// Get the WASM path
    pub fn wasm_path(&self) -> &PathBuf {
        &self.wasm_path
    }

    /// Check if ZK binaries exist
    pub fn check_zkbins(&self) -> Vec<PathBuf> {
        let mut bins = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.zkbin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "bin") {
                    if path.file_name().map_or(false, |n| {
                        n.to_string_lossy().contains(".zk.")
                    }) {
                        bins.push(path);
                    }
                }
            }
        }
        bins
    }

    /// Check if genesis has been run
    pub async fn check_genesis(&self) -> std::result::Result<GenesisStatus, DeployerError> {
        if !self.state_file.exists() {
            return Ok(GenesisStatus::NotStarted);
        }

        match smol::fs::read_to_string(&self.state_file).await {
            Ok(content) => {
                match serde_json::from_str::<GenesisState>(&content) {
                    Ok(state) => Ok(GenesisStatus::Ready(state)),
                    Err(_) => Ok(GenesisStatus::Invalid),
                }
            }
            Err(_) => Ok(GenesisStatus::Invalid),
        }
    }

    /// Build WASM binary if missing
    pub async fn build_if_needed(&self) -> std::result::Result<(), DeployerError> {
        if self.check_wasm() {
            info!("WASM binary exists at {:?}", self.wasm_path);
            return Ok(());
        }

        info!("Building WASM binary for {}...", self.contract_name);

        // Run cargo build for this contract
        let output = std::process::Command::new("cargo")
            .args(["build", "--release", "-p", &format!("darkfi_{}_contract", self.contract_name)])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .map_err(|e| DeployerError::BuildFailed(format!("Failed to run cargo build: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DeployerError::BuildFailed(stderr.to_string()));
        }

        info!("WASM binary built successfully");
        Ok(())
    }

    /// Run genesis setup (GenesisHarness)
    pub async fn run_genesis(&mut self) -> std::result::Result<GenesisState, DeployerError> {
        // If genesis is already populated, just return the state
        if let Some(ref genesis) = self.genesis {
            info!("Genesis already initialized");
            let state = GenesisState {
                block_height: genesis.block_height()
                    .map_err(|e| DeployerError::GenesisFailed(e.to_string()))?,
                deployed_contracts: genesis.deployed_contracts.iter().map(|cid| cid.to_string()).collect(),
                timestamp_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            };
            return Ok(state);
        }

        info!("Running genesis setup...");

        let mut genesis = GenesisHarness::new(self.harness_config.clone(), &self.ex)
            .await
            .map_err(|e| DeployerError::GenesisFailed(e.to_string()))?;

        // Generate genesis blocks
        genesis.generate_genesis_blocks(3).await
            .map_err(|e| DeployerError::GenesisFailed(e.to_string()))?;

        let state = GenesisState {
            block_height: genesis.block_height()
                .map_err(|e| DeployerError::GenesisFailed(e.to_string()))?,
            deployed_contracts: genesis.deployed_contracts.iter().map(|cid| cid.to_string()).collect(),
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        // Save genesis state
        let json = serde_json::to_string_pretty(&state)
            .map_err(|e| DeployerError::GenesisFailed(e.to_string()))?;
        smol::fs::write(&self.state_file, json).await
            .map_err(|e| DeployerError::GenesisFailed(e.to_string()))?;

        // Store genesis harness for later use
        self.genesis = Some(genesis);

        info!("Genesis complete, state saved to {:?}", self.state_file);
        Ok(state)
    }

    /// Deploy a WASM contract via Deployooor
    pub async fn deploy(&mut self) -> std::result::Result<ContractId, DeployerError> {
        // Make sure WASM exists
        if !self.check_wasm() {
            return Err(DeployerError::WasmNotFound(self.wasm_path.to_string_lossy().to_string()));
        }

        // Use the stored genesis harness (set by ensure_ready() via run_genesis())
        let genesis = self.genesis
            .as_mut()
            .ok_or_else(|| {
                DeployerError::GenesisFailed(
                    "Genesis not initialized. Call ensure_ready() first.".to_string(),
                )
            })?;

        // Read WASM binary
        let wasm = smol::fs::read(&self.wasm_path).await
            .map_err(|e| DeployerError::DeploymentFailed(e.to_string()))?;

        // Deploy via genesis harness
        let contract_id = genesis.deploy_contract(wasm, &self.contract_name).await
            .map_err(|e| DeployerError::DeploymentFailed(e.to_string()))?;

        info!("Deployed {} contract: {:?}", self.contract_name, contract_id);
        Ok(contract_id)
    }

    /// Get the deployed contract ID if already deployed
    pub async fn get_deployed_contract_id(&self) -> Option<ContractId> {
        if let Ok(GenesisStatus::Ready(state)) = self.check_genesis().await {
            // Return the first deployed contract (if any)
            state.deployed_contracts.first().and_then(|s| {
                ContractId::from_str(s).ok()
            })
        } else {
            None
        }
    }

    /// Ensure everything is ready for testing
    /// - Builds WASM if missing
    /// - Runs genesis if needed (stores GenesisHarness in self.genesis)
    /// - Deploys contract using stored GenesisHarness
    pub async fn ensure_ready(&mut self) -> std::result::Result<ContractId, DeployerError> {
        // 1. Build WASM if needed
        self.build_if_needed().await?;

        // 2. Run genesis if needed (populate self.genesis)
        if self.genesis.is_none() {
            self.run_genesis().await?;
        }

        // 3. Deploy contract (use stored genesis harness)
        self.deploy().await
    }
}

// ============================================================================
// Tests
// ============================================================================

/// Test the deployer genesis workflow (no WASM deployment since no valid contract exists)
pub async fn test_deployer_impl(ex: Arc<Executor<'static>>) -> std::result::Result<(), DeployerError> {
    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(BigUint::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18450".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18451".to_string(),
    };

    info!("Creating deployer for dex (no WASM will be deployed)...");
    let mut deployer = ContractDeployer::new("dex", config, ex).await?;

    // Run genesis only (deploy() would fail since no valid WASM exists)
    info!("Running genesis setup...");
    deployer.run_genesis().await?;

    info!("test_deployer PASSED - genesis workflow verified");
    Ok(())
}

#[test]
fn test_deployer() -> std::result::Result<(), DeployerError> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_deployer_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}