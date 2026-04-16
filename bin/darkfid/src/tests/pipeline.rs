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
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Unified Contract Testing Pipeline
//!
//! Provides a standardized, modular workflow for testing WASM contracts:
//!
//! 1. **Binary Checking** - Detect missing/stale WASM and ZK binaries
//! 2. **Genesis Setup** - Run GenesisHarness (NativeToken + Deployooor) if needed
//! 3. **Contract Deployment** - Deploy WASM contract via Deployooor
//!
//! ## Architecture
//!
//! ```text
//! Pipeline::new("dex")
//!     │
//!     ├─► BinaryChecker
//!     │       ├─► check_wasm() → Missing/Stale/Current
//!     │       └─► check_zkbins() → Vec<BinaryInfo>
//!     │
//!     ├─► GenesisRunner
//!     │       ├─► check_genesis() → NotStarted/Ready/Invalid
//!     │       └─► ensure_genesis() → GenesisState
//!     │
//!     └─► ContractDeployer
//!             └─► deploy() → ContractId
//! ```
//!
//! ## Usage
//!
//! ### One-shot (most common)
//! ```ignore
//! let contract_id = ContractTestingPipeline::new("dex", config, &ex)
//!     .await
//!     .ensure_ready_and_deploy()
//!     .await?;
//! ```
//!
//! ### Step-by-step (for debugging)
//! ```ignore
//! let mut pipeline = ContractTestingPipeline::new("dex", config, &ex).await;
//!
//! // Check status
//! let report = pipeline.status_report().await;
//!
//! // Build if needed
//! if matches!(report.wasm_status, BinaryStatus::Missing) {
//!     pipeline.build_if_needed().await?;
//! }
//!
//! // Run genesis
//! pipeline.ensure_genesis().await?;
//!
//! // Deploy
//! let contract_id = pipeline.deploy().await?;
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use darkfi_sdk::{crypto::ContractId, num_traits::One};
use num_bigint::BigUint;
use smol::Executor;
use tracing::info;

// Re-export GenesisHarness for use in tests
pub use crate::tests::genesis::GenesisHarness;
use crate::tests::HarnessConfig;

// ============================================================================
// Binary Status Types
// ============================================================================

/// Status of a binary (WASM or ZK)
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryStatus {
    /// Binary doesn't exist
    Missing,
    /// Binary exists but source is newer
    Stale,
    /// Binary exists and is current
    Current,
}

/// Detailed binary info
#[derive(Debug, Clone)]
pub struct BinaryInfo {
    /// Path to the binary
    pub path: PathBuf,
    /// Status of the binary
    pub status: BinaryStatus,
    /// Last modified time (if available)
    pub last_modified: Option<std::time::SystemTime>,
}

// ============================================================================
// Genesis Status Types
// ============================================================================

/// Persisted genesis state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenesisState {
    /// Block height after genesis
    pub block_height: u32,
    /// Deployed contract IDs as hex strings
    pub deployed_contracts: Vec<String>,
    /// When genesis was run (as unix timestamp)
    pub timestamp_secs: u64,
}

/// Status of genesis setup
#[derive(Debug, Clone)]
pub enum GenesisStatus {
    /// No genesis state exists
    NotStarted,
    /// Genesis has been run and is valid
    Ready(GenesisState),
    /// Genesis state exists but appears corrupted or stale
    Invalid,
}

// ============================================================================
// Pipeline Status Report
// ============================================================================

/// Complete status report for all pipeline components
#[derive(Debug, Clone)]
pub struct PipelineStatusReport {
    /// WASM binary status
    pub wasm_status: BinaryStatus,
    /// ZK binary info
    pub zkbin_status: Vec<BinaryInfo>,
    /// Genesis status
    pub genesis_status: GenesisStatus,
    /// Whether contract has been deployed
    pub contract_deployed: bool,
}

// ============================================================================
// Contract Manifest
// ============================================================================

/// Contract manifest parsed from pipeline.toml
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ContractManifest {
    pub name: String,
    pub dependencies: Vec<String>,
}

impl ContractManifest {
    /// Get the base directory for contracts
    fn base_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("src")
            .join("contract")
    }

    /// Load manifest for a contract by name
    pub fn load(contract_name: &str) -> std::result::Result<Self, PipelineError> {
        let manifest_path = Self::base_dir()
            .join(contract_name)
            .join("pipeline.toml");

        if !manifest_path.exists() {
            // Return empty manifest if no pipeline.toml exists
            return Ok(ContractManifest {
                name: contract_name.to_string(),
                dependencies: vec![],
            });
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PipelineError::ManifestFailed(format!("Failed to read {}: {}", manifest_path.display(), e)))?;

        toml::from_str(&content)
            .map_err(|e| PipelineError::ManifestFailed(format!("Failed to parse {}: {}", manifest_path.display(), e)))
    }

    /// Resolve all dependencies recursively, returning in deployment order
    pub fn resolve_dependencies(&self) -> std::result::Result<Vec<String>, PipelineError> {
        let mut resolved = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.resolve_deps_recursive(&mut resolved, &mut seen)?;
        Ok(resolved)
    }

    fn resolve_deps_recursive(
        &self,
        resolved: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) -> std::result::Result<(), PipelineError> {
        for dep in &self.dependencies {
            if !seen.contains(dep) {
                seen.insert(dep.clone());
                let manifest = ContractManifest::load(dep)?;
                manifest.resolve_deps_recursive(resolved, seen)?;
                if !resolved.contains(&dep.clone()) {
                    resolved.push(dep.clone());
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// ContractTestingPipeline
// ============================================================================

/// Unified contract testing pipeline
///
/// Orchestrates binary checking, genesis setup, and contract deployment
/// in a modular, composable way.
pub struct ContractTestingPipeline {
    /// Contract name (e.g., "stablecoin", "dex")
    contract_name: String,
    /// Path to WASM binary
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

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("WASM binary not found: {0}")]
    WasmNotFound(String),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Genesis failed: {0}")]
    GenesisFailed(String),

    #[error("Deployment failed: {0}")]
    DeploymentFailed(String),

    #[error("Genesis not initialized. Call ensure_genesis() first.")]
    GenesisNotInitialized,

    #[error("Manifest error: {0}")]
    ManifestFailed(String),

    #[error("Dependency cycle detected involving: {0}")]
    DependencyCycle(String),
}

// ============================================================================
// ContractTestingPipeline Implementation
// ============================================================================

impl ContractTestingPipeline {
    /// Create a new pipeline for the given contract
    pub async fn new(
        contract_name: &str,
        harness_config: HarnessConfig,
        ex: Arc<Executor<'static>>,
    ) -> std::result::Result<Self, PipelineError> {
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

    /// Get WASM binary status
    pub fn check_wasm(&self) -> BinaryStatus {
        if !self.wasm_path.exists() {
            return BinaryStatus::Missing
        }

        // TODO: Add stale checking by comparing source modification time
        // For now, if it exists, it's Current
        BinaryStatus::Current
    }

    /// Get ZK binary status
    pub fn check_zkbins(&self) -> Vec<BinaryInfo> {
        let mut bins = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.zkbin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "bin") {
                    if path.file_name().map_or(false, |n| n.to_string_lossy().contains(".zk.")) {
                        let last_modified = entry.metadata().ok().and_then(|m| m.modified().ok());
                        bins.push(BinaryInfo {
                            path: path.clone(),
                            status: BinaryStatus::Current, // TODO: add stale checking
                            last_modified,
                        });
                    }
                }
            }
        }

        bins
    }

    /// Get path to zkas compiler binary
    fn zkas_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("release")
            .join("zkas")
    }

    /// Check zkas compiler status
    pub fn check_zkas() -> BinaryStatus {
        let path = Self::zkas_path();
        if !path.exists() {
            return BinaryStatus::Missing;
        }
        BinaryStatus::Current
    }

    /// Build zkas compiler if missing
    pub async fn build_zkas_if_needed(&self) -> std::result::Result<BinaryStatus, PipelineError> {
        let status = Self::check_zkas();
        if matches!(status, BinaryStatus::Current) {
            info!("zkas compiler is current at {:?}", Self::zkas_path());
            return Ok(status);
        }

        info!("Building zkas compiler...");
        let output = std::process::Command::new("cargo")
            .args(["build", "--release", "-p", "zkas"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .map_err(|e| PipelineError::BuildFailed(format!("Failed to run cargo build: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PipelineError::BuildFailed(stderr.to_string()));
        }

        info!("zkas compiler built successfully");
        Ok(BinaryStatus::Current)
    }

    /// Build ZK binaries from .zk source files for this contract
    pub async fn build_zk_bins(&self) -> std::result::Result<(), PipelineError> {
        let zkas = Self::zkas_path();
        let zk_src_dir = &self.zkbin_dir;

        let src_pattern = zk_src_dir.join("*.zk");
        let src_files: Vec<PathBuf> = glob::glob(&src_pattern.to_string_lossy())
            .map_err(|e| PipelineError::BuildFailed(e.to_string()))?
            .flatten()
            .collect();

        if src_files.is_empty() {
            info!("No .zk source files found in {:?}", zk_src_dir);
            return Ok(());
        }

        info!("Found {} .zk source files in {:?}", src_files.len(), zk_src_dir);

        for src in src_files {
            let bin_path = src.with_extension("zk.bin");

            // Check if binary exists and is newer than source
            if bin_path.exists() {
                if let (Ok(src_meta), Ok(bin_meta)) = (src.metadata(), bin_path.metadata()) {
                    if let (Ok(src_modified), Ok(bin_modified)) =
                        (src_meta.modified(), bin_meta.modified())
                    {
                        if bin_modified > src_modified {
                            info!("ZK binary {:?} is current, skipping", bin_path);
                            continue;
                        }
                    }
                }
            }

            info!("Building ZK binary {:?} from {:?}", bin_path, src);
            let output = std::process::Command::new(&zkas)
                .arg(src)
                .arg("-o")
                .arg(&bin_path)
                .output()
                .map_err(|e| PipelineError::BuildFailed(format!("Failed to run zkas: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(PipelineError::BuildFailed(stderr.to_string()));
            }

            info!("ZK binary {:?} built successfully", bin_path);
        }

        Ok(())
    }

    /// Check if genesis has been run
    pub async fn check_genesis(&self) -> std::result::Result<GenesisStatus, PipelineError> {
        if !self.state_file.exists() {
            return Ok(GenesisStatus::NotStarted);
        }

        match smol::fs::read_to_string(&self.state_file).await {
            Ok(content) => match serde_json::from_str::<GenesisState>(&content) {
                Ok(state) => Ok(GenesisStatus::Ready(state)),
                Err(_) => Ok(GenesisStatus::Invalid),
            },
            Err(_) => Ok(GenesisStatus::Invalid),
        }
    }

    /// Build WASM binary (with ZK binaries first) for this contract
    pub async fn build_contract(&self) -> std::result::Result<BinaryStatus, PipelineError> {
        // Build zkas compiler first
        self.build_zkas_if_needed().await?;

        // Build ZK binaries for this contract
        self.build_zk_bins().await?;

        // Build WASM
        let status = self.check_wasm();
        if matches!(status, BinaryStatus::Current) {
            info!("WASM binary is current at {:?}", self.wasm_path);
            return Ok(status);
        }

        info!("Building WASM binary for {}...", self.contract_name);

        // Run cargo build for this contract
        let output = std::process::Command::new("cargo")
            .args([
                "build",
                "--release",
                "-p",
                &format!("darkfi_{}_contract", self.contract_name),
            ])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .map_err(|e| PipelineError::BuildFailed(format!("Failed to run cargo build: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PipelineError::BuildFailed(stderr.to_string()));
        }

        info!("WASM binary built successfully");
        Ok(BinaryStatus::Current)
    }

    /// Run genesis if needed, store GenesisHarness internally
    pub async fn ensure_genesis(
        &mut self,
    ) -> std::result::Result<GenesisState, PipelineError> {
        // If genesis is already populated, just return the state
        if let Some(ref genesis) = self.genesis {
            info!("Genesis already initialized");
            let state = GenesisState {
                block_height: genesis.block_height().map_err(|e| PipelineError::GenesisFailed(e.to_string()))?,
                deployed_contracts: genesis.deployed_contracts.iter().map(|cid| cid.to_string()).collect(),
                timestamp_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            };
            return Ok(state);
        }

        // Check genesis status
        let status = self.check_genesis().await?;
        if matches!(status, GenesisStatus::Ready(_)) {
            info!("Genesis state file exists, creating fresh harness");
        }

        info!("Running genesis setup...");

        let mut genesis = GenesisHarness::new(self.harness_config.clone(), &self.ex)
            .await
            .map_err(|e| PipelineError::GenesisFailed(e.to_string()))?;

        // Generate genesis blocks
        genesis.generate_genesis_blocks(3).await
            .map_err(|e| PipelineError::GenesisFailed(e.to_string()))?;

        let state = GenesisState {
            block_height: genesis.block_height().map_err(|e| PipelineError::GenesisFailed(e.to_string()))?,
            deployed_contracts: genesis.deployed_contracts.iter().map(|cid| cid.to_string()).collect(),
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        // Save genesis state
        let json = serde_json::to_string_pretty(&state)
            .map_err(|e| PipelineError::GenesisFailed(e.to_string()))?;
        smol::fs::write(&self.state_file, json).await
            .map_err(|e| PipelineError::GenesisFailed(e.to_string()))?;

        // Store genesis harness for later use
        self.genesis = Some(genesis);

        info!("Genesis complete, state saved to {:?}", self.state_file);
        Ok(state)
    }

    /// Deploy a single contract by name using the stored GenesisHarness
    async fn deploy_single_contract(
        &mut self,
        contract_name: &str,
    ) -> std::result::Result<ContractId, PipelineError> {
        let wasm_path = ContractManifest::base_dir()
            .join(contract_name)
            .join(format!("darkfi_{}_contract.wasm", contract_name));

        // Make sure WASM exists
        if !wasm_path.exists() {
            return Err(PipelineError::WasmNotFound(wasm_path.to_string_lossy().to_string()));
        }

        // Use the stored genesis harness
        let genesis = self.genesis.as_mut().ok_or(PipelineError::GenesisNotInitialized)?;

        // Read WASM binary
        let wasm = smol::fs::read(&wasm_path).await
            .map_err(|e| PipelineError::DeploymentFailed(e.to_string()))?;

        // Deploy via genesis harness
        let contract_id = genesis.deploy_contract(wasm, contract_name).await
            .map_err(|e| PipelineError::DeploymentFailed(e.to_string()))?;

        info!("Deployed {} contract: {:?}", contract_name, contract_id);
        Ok(contract_id)
    }

    /// Deploy the contract and all its dependencies using stored GenesisHarness
    pub async fn deploy(&mut self) -> std::result::Result<ContractId, PipelineError> {
        // Load manifest for this contract
        let manifest = ContractManifest::load(&self.contract_name)?;

        // Resolve dependencies in deployment order
        let deployment_order = manifest.resolve_dependencies()?;

        info!(
            "Deploying {} with dependencies: {:?}",
            self.contract_name,
            deployment_order
        );

        // Deploy dependencies first
        for dep in &deployment_order {
            self.deploy_single_contract(dep).await?;
        }

        // Deploy this contract
        let contract_name = self.contract_name.clone();
        self.deploy_single_contract(&contract_name).await
    }

    /// One-shot: ensure everything is ready and deploy
    pub async fn ensure_ready_and_deploy(
        &mut self,
    ) -> std::result::Result<ContractId, PipelineError> {
        // Load manifest for this contract
        let manifest = ContractManifest::load(&self.contract_name)?;

        // Resolve dependencies
        let deployment_order = manifest.resolve_dependencies()?;

        // 1. Build all contracts if needed
        for contract_name in deployment_order.iter().chain(std::iter::once(&self.contract_name)) {
            let pipeline = ContractTestingPipeline::new(
                contract_name,
                self.harness_config.clone(),
                self.ex.clone(),
            ).await?;
            pipeline.build_contract().await?;
        }

        // 2. Run genesis if needed (populate self.genesis)
        self.ensure_genesis().await?;

        // 3. Deploy contract and dependencies
        self.deploy().await
    }

    /// Get a report of all statuses
    pub async fn status_report(&self) -> std::result::Result<PipelineStatusReport, PipelineError> {
        let wasm_status = self.check_wasm();
        let zkbin_status = self.check_zkbins();
        let genesis_status = self.check_genesis().await?;

        Ok(PipelineStatusReport {
            wasm_status,
            zkbin_status,
            genesis_status,
            contract_deployed: false, // TODO: track this properly
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

/// All deployable contracts (those with test harnesses)
const ALL_CONTRACTS: [&str; 24] = [
    "atomic_swap", "attestation", "auction", "baccarat", "block_height_prediction",
    "bridge", "dao_escrow", "darkbet_exchange", "darktoshi_dice", "dex",
    "escrow", "identity", "insurance_market", "labor_market", "lottery",
    "money_v3", "oracle", "pool_stake", "relayer_endowment", "slot",
    "stablecoin", "subscription", "tender", "native_token",
];

/// Test the pipeline - deploy any contract by CONTRACT_NAME env var
pub async fn test_pipeline_impl(ex: Arc<Executor<'static>>) -> std::result::Result<(), PipelineError> {
    let contract_name =
        std::env::var("CONTRACT_NAME").unwrap_or_else(|_| "dex".to_string());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(BigUint::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18550".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18551".to_string(),
    };

    info!("Creating pipeline for {}...", contract_name);
    let mut pipeline = ContractTestingPipeline::new(&contract_name, config, ex).await?;

    info!("Running genesis...");
    let state = pipeline.ensure_genesis().await?;
    info!("Genesis complete: block_height={}", state.block_height);

    info!("Deploying {}...", contract_name);
    let contract_id = pipeline.deploy().await?;
    info!("{} deployed: {:?}", contract_name, contract_id);

    // Lightweight verification: deployment success confirms:
    // - WASM binary is valid and loadable
    // - Contract exec function exists and is reachable
    // - Contract ID is derivable
    info!("✓ {} endpoint verification passed (deployment successful)", contract_name);

    info!("test_pipeline PASSED");
    Ok(())
}

#[test]
fn test_pipeline() -> std::result::Result<(), PipelineError> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_pipeline_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

/// Batch test: deploy all contracts via ContractTestingPipeline
pub async fn test_all_contracts_deploy_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), PipelineError> {
    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(BigUint::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18550".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18551".to_string(),
    };

    for contract_name in ALL_CONTRACTS {
        info!("[{}] Starting deployment test...", contract_name);

        let mut pipeline =
            ContractTestingPipeline::new(contract_name, config.clone(), ex.clone()).await?;

        pipeline.ensure_ready_and_deploy().await?;

        info!("[{}] ✓ Deployed successfully", contract_name);
    }

    info!("All {} contracts deployed successfully!", ALL_CONTRACTS.len());
    Ok(())
}

#[test]
fn test_all_contracts_deploy() -> std::result::Result<(), PipelineError> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_all_contracts_deploy_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}
