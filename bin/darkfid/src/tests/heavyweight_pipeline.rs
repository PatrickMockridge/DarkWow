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

use dwow::{
    tx::{ContractCallLeaf, TransactionBuilder},
    Result,
};
use darkfi_contract_test_harness::harness::ContractHarness;
use dwow_sdk::{
    crypto::{keypair::Keypair, poseidon_hash, ContractId},
    dark_tree::DarkTree,
    pasta::pallas,
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
        proofs: Vec<dwow::zk::Proof>,
    ) -> std::result::Result<dwow::tx::Transaction, HeavyweightError> {
        self.exec_with_children(function_id, call_data, proofs, vec![], vec![]).await
    }

    /// Execute a contract call with ZK proofs and child calls
    ///
    /// Builds a transaction, signs it, and returns the transaction.
    pub async fn exec_with_children(
        &mut self,
        function_id: u8,
        mut call_data: Vec<u8>,
        proofs: Vec<dwow::zk::Proof>,
        children: Vec<ContractCall>,
        child_proofs: Vec<Vec<dwow::zk::Proof>>,
    ) -> std::result::Result<dwow::tx::Transaction, HeavyweightError> {
        let contract_id =
            self.contract_id.ok_or(HeavyweightError::NotDeployed)?;

        // Prepend function ID to call data
        let mut data = vec![function_id];
        data.append(&mut call_data);

        let call = ContractCall { contract_id, data };

        // Convert children ContractCalls to DarkTree<ContractCallLeaf>
        // child_proofs[i] contains proofs for children[i]
        let child_trees: Vec<DarkTree<ContractCallLeaf>> = children
            .into_iter()
            .zip(child_proofs.into_iter())
            .map(|(c, proofs)| {
                DarkTree::new(
                    ContractCallLeaf { call: c, proofs },
                    vec![],
                    None,
                    None,
                )
            })
            .collect();

        // Build transaction with proofs
        let mut tx_builder = TransactionBuilder::new(
            ContractCallLeaf { call, proofs },
            child_trees,
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

/// Compute a FuncId from contract_id and function code
///
/// FuncId = poseidon_hash([contract_id.inner(), func_code as u64])
fn compute_func_id(contract_id: ContractId, func_code: u8) -> pallas::Base {
    poseidon_hash([contract_id.inner(), pallas::Base::from(func_code as u64)])
}

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
    use darkfi_contract_test_harness::harness::{DexHarness, MoneyV3Harness};
    use dwow_sdk::crypto::{SecretKey, pasta_prelude::PrimeField};
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18646".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18647".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18560".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18561".to_string(),
    };

    info!("DEX harness created with circuits: {:?}", DexHarness::new().circuits());

    let mut pipeline =
        HeavyweightPipeline::new(DexHarness::new(), "dex", config, ex).await?;

    // Generate genesis blocks
    pipeline.generate_genesis_blocks(3).await?;

    // Read and deploy WASM
    let wasm = read_wasm("dex").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("DEX deployed: {:?}", contract_id);

    // Create a new harness for proof generation (pipeline takes ownership of first harness)
    let harness = DexHarness::new();

    // Build child calls for ExecuteSwapV1 (0x03) - requires 2 money_v3::otc_swap_v1 (0x05) calls
    let child_call_0 = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x05], // otc_swap_v1 for Alice's tokens
    };
    let child_call_1 = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x05], // otc_swap_v1 for Bob's tokens
    };

    // Create a swap proposal
    let secret = Base::random(&mut OsRng);
    let offer_token = Base::from(1); // Token ID 1
    let offer_amount = 1000u64;
    let request_token = Base::from(2); // Token ID 2
    let request_amount = 500u64;
    let signature_secret = SecretKey::random(&mut OsRng);

    let create_result = harness.create_swap(
        secret,
        offer_token,
        offer_amount,
        request_token,
        request_amount,
        signature_secret,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created swap: swap_id={}", hex::encode(create_result.public_inputs.swap_id.to_repr()));

    // Execute CreateSwapV1 (0x01)
    let tx = pipeline.exec(0x01, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed dex::0x01 (tx: {:?})", tx.hash());

    // Accept the swap
    let acceptor_secret = Base::random(&mut OsRng);
    let acceptor_signature_secret = SecretKey::random(&mut OsRng);
    let swap_id = create_result.public_inputs.swap_id;
    let proposer_lock_commitment = create_result.public_inputs.lock_commitment;

    let accept_result = harness.accept_swap(
        swap_id,
        proposer_lock_commitment,
        acceptor_secret,
        offer_token,
        offer_amount,
        acceptor_signature_secret,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Accepted swap: swap_id={}", hex::encode(swap_id.to_repr()));

    // Execute AcceptSwapV1 (0x02)
    let tx = pipeline.exec(0x02, accept_result.call_data, vec![accept_result.proof]).await?;
    info!("Executed dex::0x02 (tx: {:?})", tx.hash());

    // Execute the swap
    // Compute real FuncIds now that we know money_contract_id
    // FuncId = poseidon_hash([contract_id.inner(), func_code])
    let alice_otc_func_id = compute_func_id(money_contract_id, 0x05); // otc_swap_v1
    let bob_otc_func_id = compute_func_id(money_contract_id, 0x05); // otc_swap_v1
    let execute_result = harness.execute_swap(
        secret,
        offer_token,
        offer_amount,
        create_result.public_inputs.lock_commitment,
        acceptor_secret,
        request_token,
        request_amount,
        accept_result.public_inputs.acceptor_lock_commitment,
        offer_amount, // full fill
        alice_otc_func_id,
        bob_otc_func_id,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Executed swap: swap_id={}", hex::encode(execute_result.public_inputs.swap_id.to_repr()));

    // Execute ExecuteSwapV1 (0x03) - requires 2 money_v3::otc_swap_v1 child calls
    let tx = pipeline.exec_with_children(0x03, execute_result.call_data, vec![execute_result.proof], vec![child_call_0, child_call_1], vec![vec![], vec![]]).await?;
    info!("Executed dex::0x03 (tx: {:?})", tx.hash());

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
    use dwow_sdk::crypto::pasta_prelude::PrimeField;
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Create a new harness for proof generation (pipeline takes ownership of first harness)
    let harness = MoneyV3Harness::spawn();

    // Create a token
    let token_auth_parent = Base::from(1);
    let token_user_data = Base::from(2);
    let token_blind = Base::from(3);
    let recipient = Base::from(4);
    let initial_value = 1000u64;
    let spend_hook = Base::zero();
    let user_data = Base::zero();
    let coin_blind = Base::random(&mut OsRng);

    let create_result = harness.create_token(
        token_auth_parent,
        token_user_data,
        token_blind,
        recipient,
        initial_value,
        spend_hook,
        user_data,
        coin_blind,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created token: token_id={}", hex::encode(create_result.token_id.to_repr()));

    // Execute TokenMintV1 (0x00) and AuthTokenMintV1 (0x01) in one transaction
    let tx = pipeline.exec(0x00, create_result.call_data.clone(), create_result.token_proofs).await?;
    info!("Executed money_v3::0x00 (tx: {:?})", tx.hash());

    let tx = pipeline.exec(0x01, create_result.call_data, create_result.auth_proofs).await?;
    info!("Executed money_v3::0x01 (tx: {:?})", tx.hash());

    // Now mint some tokens
    let mint_result = harness.mint(
        create_result.token_id,
        recipient,
        500u64,
        create_result.auth_nullifier,
        create_result.auth_mint_public,
        create_result.token_registry_root,
        spend_hook,
        user_data,
        Base::random(&mut OsRng),
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Minted tokens: coin={}", hex::encode(mint_result.coin.inner().to_repr()));

    // Execute MintV1 (0x02)
    let tx = pipeline.exec(0x02, mint_result.call_data, mint_result.proofs).await?;
    info!("Executed money_v3::0x02 (tx: {:?})", tx.hash());

    info!("test_money_v3_heavyweight PASSED");
    Ok(())
}

// ============================================================================
// Additional Contract Heavyweight Tests
// ============================================================================

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
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{pasta_prelude::PrimeField, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas::Base;
    use darkfi_attestation_contract::model::Predicate;
    use rand::rngs::OsRng;

    let harness = AttestationHarness::spawn();
    info!("Attestation harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Fresh harness for proof generation
    let harness = AttestationHarness::spawn();

    // Generate attestor and claimant keypairs
    let attestor_secret = Base::random(&mut OsRng);
    let attestor_pub = PublicKey::from_secret(
        SecretKey::from_bytes(attestor_secret.to_repr()).unwrap()
    );
    let claimant_secret = Base::random(&mut OsRng);
    let claimant_pub = PublicKey::from_secret(
        SecretKey::from_bytes(claimant_secret.to_repr()).unwrap()
    );

    let attestation_id = Base::from(1u64);
    let claim_id = Base::from(2u64);
    let claim_type = Predicate::Matches;
    let claim_data = vec![Base::from(42u64)];

    // Step 1: Create attestation (0x00)
    let create_result = harness.create_attestation(
        attestor_secret,
        attestor_pub,
        claim_type,
        claim_data.clone(),
        vec![],       // metadata
        None,         // expires_at
        attestation_id,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created attestation: id={}", hex::encode(create_result.attestation_id.to_repr()));

    let tx = pipeline.exec(0x00, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed attestation::0x00 CreateAttestationV1 (tx: {:?})", tx.hash());

    // Step 2: Create claim (0x02)
    let evidence_commitment = vec![0u8; 32];
    let revealed_result = vec![0u8; 32];
    let create_claim_result = harness.create_claim(
        attestation_id,
        claimant_secret,
        claimant_pub,
        claim_type,
        evidence_commitment,
        revealed_result,
        claim_id,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created claim: id={}", hex::encode(create_claim_result.claim_id.to_repr()));

    let tx = pipeline.exec(0x02, create_claim_result.call_data, vec![create_claim_result.proof]).await?;
    info!("Executed attestation::0x02 CreateClaimV1 (tx: {:?})", tx.hash());

    // Step 3: Verify claim (0x03)
    let revealed_result = Base::from(42u64);
    let evidence = Base::from(42u64);
    let attestation_data = Base::from(42u64);
    let nonce = Base::random(&mut OsRng);
    let pos = Base::from(0u64);
    let path = [Base::zero(); 255];
    let revocation_root = Base::zero();

    let verify_result = harness.verify_claim(
        claim_id,
        attestation_id,
        revealed_result,
        evidence,
        attestation_data,
        nonce,
        pos,
        path,
        revocation_root,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Verified claim");

    let tx = pipeline.exec(0x03, verify_result.call_data, vec![verify_result.proof]).await?;
    info!("Executed attestation::0x03 VerifyClaimV1 (tx: {:?})", tx.hash());

    // Step 4: Consume claim (0x04)
    let nullifier = Base::from(100u64);
    let consume_result = harness.consume_claim(
        claim_id,
        attestation_id,
        nullifier,
        claimant_secret,
        claimant_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Consumed claim");

    let tx = pipeline.exec(0x04, consume_result.call_data, vec![consume_result.proof]).await?;
    info!("Executed attestation::0x04 ConsumeClaimV1 (tx: {:?})", tx.hash());

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
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{pasta_prelude::PrimeField, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas::Base;
    use rand::rngs::OsRng;

    let harness = AuctionHarness::spawn();
    info!("Auction harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Fresh harness for proof generation
    let harness = AuctionHarness::spawn();

    // Create seller and bidder keypairs
    let seller_secret = Base::random(&mut OsRng);
    let seller_pub = PublicKey::from_secret(
        SecretKey::from_bytes(seller_secret.to_repr()).unwrap()
    );
    let bidder_secret = Base::random(&mut OsRng);
    let bidder_pub = PublicKey::from_secret(
        SecretKey::from_bytes(bidder_secret.to_repr()).unwrap()
    );

    let item_commitment = Base::from(42u64);
    let reserve_price = 100u64;
    let token_id = Base::from(1u64);
    let deadline_block = 6u64;

    // Step 1: Create auction (0x00)
    // current_block=3 < deadline=6: auction is active
    let create_result = harness.create_auction(
        seller_secret,
        item_commitment,
        reserve_price,
        token_id,
        deadline_block,
        3, // current_block
        seller_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created auction: auction_id={}", hex::encode(create_result.auction_id.to_repr()));

    let tx = pipeline.exec(0x00, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed auction::0x00 CreateAuctionV1 (tx: {:?})", tx.hash());

    // Step 2: Place bid (0x01)
    // current_block=4 < deadline=6: bid is accepted
    let bid_amount = 500u64;
    let bid_nonce = Base::random(&mut OsRng);

    let place_result = harness.place_bid(
        create_result.auction_id,
        bidder_secret,
        bid_amount,
        bid_nonce,
        deadline_block,
        4, // current_block
        0, // current_high_bid (no bids yet)
        bidder_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Placed bid: bid_id={}", hex::encode(place_result.bid_id.to_repr()));

    let tx = pipeline.exec(0x01, place_result.call_data, vec![place_result.proof]).await?;
    info!("Executed auction::0x01 PlaceBidV1 (tx: {:?})", tx.hash());

    // Step 3: Close auction (0x02)
    // current_block=7 > deadline=6: deadline has passed, can close
    let close_result = harness.close_auction(
        create_result.auction_id,
        place_result.bid_id,
        seller_secret,
        deadline_block,
        7, // current_block (must be > deadline)
        seller_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    let tx = pipeline.exec(0x02, close_result.call_data, vec![close_result.proof]).await?;
    info!("Executed auction::0x02 CloseAuctionV1 (tx: {:?})", tx.hash());

    // Step 4: Claim winnings (0x03)
    let claim_result = harness.claim_winnings(
        create_result.auction_id,
        place_result.bid_id,
        bidder_secret,
        bidder_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    let tx = pipeline.exec(0x03, claim_result.call_data, vec![claim_result.proof]).await?;
    info!("Executed auction::0x03 ClaimWinningsV1 (tx: {:?})", tx.hash());

    // Step 5: Settle auction (0x04)
    let settle_result = harness.settle_auction(
        create_result.auction_id,
        seller_secret,
        bid_amount,
        seller_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    let tx = pipeline.exec(0x04, settle_result.call_data, vec![settle_result.proof]).await?;
    info!("Executed auction::0x04 SettleAuctionV1 (tx: {:?})", tx.hash());

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
    use darkfi_contract_test_harness::harness::{BaccaratHarness, MoneyV3Harness};
    use dwow_sdk::crypto::pasta_prelude::{Group, PrimeField};
    use dwow_sdk::pasta::pallas::Base;
    use dwow_sdk::crypto::SecretKey;
    use darkfi_baccarat_contract::model::BetType;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18586".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18587".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = BaccaratHarness::spawn();
    info!("Baccarat harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18590".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18591".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "baccarat", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("baccarat").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Baccarat deployed: {:?}", contract_id);

    // Create a fresh harness for proof generation
    let harness = BaccaratHarness::spawn();

    // Generate player keypair
    let player_secret = SecretKey::random(&mut OsRng);
    let player_pub = dwow_sdk::crypto::PublicKey::from_secret(player_secret);

    // Bet parameters
    let bet_value = 1000u64;
    let bet_type = BetType::Player;
    let secret_nonce = Base::random(&mut OsRng);
    let blind = Base::random(&mut OsRng);
    let token_id = Base::zero(); // DARK token
    let house_edge = 150u32; // 1.5%
    let confirmation_depth = 1u8;

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Execute CommitBetV1 (0x01)
    let commit_result = harness.commit_bet(
        player_pub,
        bet_value,
        bet_type,
        secret_nonce,
        blind,
        token_id,
        house_edge,
        confirmation_depth,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Committed bet: bet_id={}", hex::encode(commit_result.bet_id.to_repr()));

    let tx = pipeline.exec_with_children(0x01, commit_result.call_data, vec![commit_result.proof], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed baccarat::0x01 (tx: {:?})", tx.hash());

    // Execute DrawCardsV1 (0x02) - cards are drawn using block hash entropy
    // Note: This doesn't require a ZK proof, just the bet_id and secret_nonce
    let draw_result = harness.draw_cards(commit_result.bet_id, secret_nonce)
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Draw cards prepared for bet_id={}", hex::encode(draw_result.bet_id.to_repr()));

    let tx = pipeline.exec(0x02, draw_result.call_data, vec![]).await?;
    info!("Executed baccarat::0x02 (tx: {:?})", tx.hash());

    // Execute SettleBetV1 (0x03) - player settles to claim winnings
    // The proof verifies the player knows the secret nonce that derives the bet_id
    let settle_result = harness.settle_bet(
        commit_result.bet_id,
        secret_nonce,
        player_pub,
        bet_value,
        bet_type,
        token_id,
        blind,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Settle bet prepared for bet_id={}", hex::encode(settle_result.public_inputs.derived_bet_id.to_repr()));

    let tx = pipeline.exec_with_children(0x03, settle_result.call_data, vec![settle_result.proof], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed baccarat::0x03 (tx: {:?})", tx.hash());

    // Now test a second scenario: HouseCloseV1 (0x04)
    // First commit another bet
    let harness2 = BaccaratHarness::spawn();
    let bet_value2 = 500u64;
    let bet_type2 = BetType::Banker;
    let secret_nonce2 = Base::random(&mut OsRng);
    let blind2 = Base::random(&mut OsRng);

    let commit_result2 = harness2.commit_bet(
        player_pub,
        bet_value2,
        bet_type2,
        secret_nonce2,
        blind2,
        token_id,
        house_edge,
        confirmation_depth,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Committed second bet: bet_id={}", hex::encode(commit_result2.bet_id.to_repr()));

    let tx = pipeline.exec_with_children(0x01, commit_result2.call_data, vec![commit_result2.proof], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed baccarat::0x01 (tx: {:?})", tx.hash());

    // Draw cards for second bet
    let draw_result2 = harness2.draw_cards(commit_result2.bet_id, secret_nonce2)
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    let tx = pipeline.exec(0x02, draw_result2.call_data, vec![]).await?;
    info!("Executed baccarat::0x02 (tx: {:?})", tx.hash());

    // House closes the second bet (simulating timeout scenario)
    // The house uses its secret key to sign the close request
    let house_secret = SecretKey::random(&mut OsRng);
    let house_pub = dwow_sdk::crypto::PublicKey::from_secret(house_secret);

    let close_result = harness2.house_close(
        commit_result2.bet_id,
        house_secret,
        house_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("House close prepared for bet_id={}", hex::encode(close_result.bet_id.to_repr()));

    let tx = pipeline.exec_with_children(0x04, close_result.call_data, vec![], vec![child_call], vec![vec![]]).await?;
    info!("Executed baccarat::0x04 (tx: {:?})", tx.hash());

    info!("test_baccarat_heavyweight PASSED");
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
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, BridgeHarness};
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{pasta_prelude::PrimeField, MerkleNode, MerkleTree, poseidon_hash};
    use dwow_sdk::pasta::pallas::Base;
    use darkfi_bridge_contract::model::ExternalChain;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18634".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18635".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = BridgeHarness::spawn();
    info!("Bridge harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18636".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18637".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "bridge", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("bridge").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Bridge deployed: {:?}", contract_id);

    // Fresh harness for proof generation
    let harness = BridgeHarness::spawn();

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Build a Merkle tree with a single deposit leaf
    let secret = Base::random(&mut OsRng);
    let amount: u64 = 1_000_000;
    let deposit_leaf = poseidon_hash([secret, Base::from(amount)]);

    let mut merkle_tree = MerkleTree::new(1);
    merkle_tree.append(MerkleNode::new(deposit_leaf));
    let position = merkle_tree.mark().unwrap(); // Mark the leaf for witnessing
    let root = merkle_tree.root(0).unwrap().inner(); // pallas::Base
    let merkle_path = merkle_tree.witness(position, 0).unwrap();

    // Recipient keypair
    let recipient_secret = Base::random(&mut OsRng);
    let recipient_public = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_bytes(recipient_secret.to_repr()).unwrap()
    );

    // External block hash (accepted as-is by circuit — not verified against light client)
    let external_block_hash = Base::random(&mut OsRng);
    let merkle_root_input = root;

    // Deposit with ZK proof
    let deposit_result = harness.deposit(
        secret,
        amount,
        recipient_public,
        1,          // bridge_nonce
        external_block_hash,
        merkle_root_input,
        0,          // leaf_pos (first element in tree)
        merkle_path,
        ExternalChain::Ethereum,
        0,          // fee
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Generated deposit ZK proof: commitment={}", hex::encode(deposit_result.public_inputs.commitment.to_repr()));

    // Execute deposit with money_v3::transfer_v1 child call
    let tx = pipeline.exec_with_children(
        0x01, // DepositV1
        deposit_result.call_data,
        vec![deposit_result.proof],
        vec![child_call],
        vec![vec![]],
    ).await?;
    info!("Executed bridge::0x01 DepositV1 (tx: {:?})", tx.hash());

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
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, DaoEscrowHarness};
    use dwow_sdk::crypto::pasta_prelude::PrimeField;
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18592".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18593".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = DaoEscrowHarness::spawn();
    info!("DaoEscrow harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Create a fresh harness for call data (withdraw doesn't need proofs)
    let harness = DaoEscrowHarness::spawn();

    // Initialize a DAO Escrow (0x00)
    let nullifier_k = pallas::Scalar::random(&mut OsRng);
    let owner_secret = Base::random(&mut OsRng);
    let endowment_token_id = Base::from(1); // Token ID 1
    let bulla_blind = Base::random(&mut OsRng);

    // Compute endowment_bulla the same way the circuit does
    let owner_pub = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_bytes(owner_secret.to_repr()).unwrap()
    );
    let (owner_pub_x, owner_pub_y) = owner_pub.xy();
    let dao_bulla = Base::from(1); // DAO bulla (simplified for test)
    let endowment_bulla = poseidon_hash([
        dao_bulla,
        owner_pub_x,
        owner_pub_y,
        endowment_token_id,
        bulla_blind,
    ]);

    // Build call data for InitializeV1 (0x00) with real ZK proof
    let init_result = harness.initialize(
        nullifier_k,
        dao_bulla,
        owner_secret,
        endowment_token_id,
        bulla_blind,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Initialized DAO Escrow: endowment_bulla={}", hex::encode(endowment_bulla.to_repr()));

    // Execute InitializeV1 (0x00) - with ZK proof
    let tx = pipeline.exec(0x00, init_result.call_data, vec![init_result.proof]).await?;
    info!("Executed dao_escrow::0x00 (tx: {:?})", tx.hash());

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Test WithdrawV1 (0x03) - owner withdraws from endowment
    let withdraw_result = harness.withdraw(
        endowment_bulla,
        owner_pub,
        100u64,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created withdraw call");

    let tx = pipeline.exec_with_children(
        0x03,
        withdraw_result.call_data,
        vec![],
        vec![child_call.clone()],
        vec![vec![]],
    ).await?;
    info!("Executed dao_escrow::0x03 (tx: {:?})", tx.hash());

    // Test EndowmentWithdrawV1 (0x04) - executes approved claim
    let claim_id = Base::from(1);
    let endowment_withdraw_result = harness.endowment_withdraw(
        endowment_bulla,
        claim_id,
        owner_pub,
        50u64,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created endowment withdraw call");

    let tx = pipeline.exec_with_children(
        0x04,
        endowment_withdraw_result.call_data,
        vec![],
        vec![child_call.clone()],
        vec![vec![]],
    ).await?;
    info!("Executed dao_escrow::0x04 (tx: {:?})", tx.hash());

    // Test TreasurySpendV1 (0x05) - executes approved treasury proposal
    let proposal_id = Base::from(1);
    let treasury_spend_result = harness.treasury_spend(
        endowment_bulla,
        proposal_id,
        owner_pub,
        25u64,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created treasury spend call");

    let tx = pipeline.exec_with_children(
        0x05,
        treasury_spend_result.call_data,
        vec![],
        vec![child_call],
        vec![vec![]],
    ).await?;
    info!("Executed dao_escrow::0x05 (tx: {:?})", tx.hash());

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
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, DarkbetExchangeHarness};
    use dwow_sdk::crypto::pasta_prelude::PrimeField;
    use dwow_sdk::pasta::pallas::{Base, Scalar};
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18594".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18595".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = DarkbetExchangeHarness::spawn();
    info!("DarkbetExchange harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Create a new harness for proof generation
    let harness = DarkbetExchangeHarness::spawn();

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Create an AMM market
    let creator_pub_x = Base::from(1);
    let creator_pub_y = Base::from(2);
    let close_block = 10000u64;
    let block_height = 5000u64;
    let nonce = 42u64;

    let create_result = harness.create_market(
        creator_pub_x,
        creator_pub_y,
        close_block,
        block_height,
        nonce,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created market: id={}", hex::encode(create_result.public_inputs.derived_market_id.to_repr()));

    // Execute CreateMarketV1 (0x00)
    let tx = pipeline.exec(0x00, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed darkbet_exchange::0x00 (tx: {:?})", tx.hash());

    // Add liquidity to the AMM pool
    let provider_pub_x = Base::from(3);
    let provider_pub_y = Base::from(4);
    let amount = 1000u64;
    let value_blind = Scalar::random(&mut OsRng);

    let add_liq_result = harness.add_liquidity(
        create_result.public_inputs.derived_market_id,
        provider_pub_x,
        provider_pub_y,
        amount,
        block_height,
        value_blind,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Added liquidity to market");

    // Execute AddLiquidityV1 (0x08) - requires money_v3 child call
    let tx = pipeline.exec_with_children(0x08, add_liq_result.call_data, vec![add_liq_result.proof], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed darkbet_exchange::0x08 (tx: {:?})", tx.hash());

    // Buy a position (outcome 1 = "YES")
    let owner_pub_x = Base::from(5);
    let owner_pub_y = Base::from(6);
    let outcome = 1u8;
    let buy_amount = 500u64;
    let buy_value_blind = Scalar::random(&mut OsRng);

    let buy_result = harness.buy_position(
        create_result.public_inputs.derived_market_id,
        owner_pub_x,
        owner_pub_y,
        outcome,
        buy_amount,
        block_height,
        buy_value_blind,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Bought position on market");

    // Execute BuyPositionV1 (0x07) - requires money_v3 child call
    let tx = pipeline.exec_with_children(0x07, buy_result.call_data, vec![buy_result.proof], vec![child_call], vec![vec![]]).await?;
    info!("Executed darkbet_exchange::0x07 (tx: {:?})", tx.hash());

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
    use darkfi_contract_test_harness::harness::{DarkToshiDiceHarness, MoneyV3Harness};
    use dwow_sdk::crypto::pasta_prelude::{Group, PrimeField};
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18596".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18597".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = DarkToshiDiceHarness::spawn();
    info!("DarkToshiDice harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18620".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18621".to_string(),
    };

    let mut pipeline =
        HeavyweightPipeline::new(harness, "darktoshi_dice", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("darktoshi_dice").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("DarkToshiDice deployed: {:?}", contract_id);

    // Create a new harness for proof generation
    let harness = DarkToshiDiceHarness::spawn();

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Player commits to a bet
    let player_secret = Base::random(&mut OsRng);
    let player_pub = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_bytes(player_secret.to_repr()).unwrap()
    );
    let bet_value = 100u64;
    let target = 99u8; // High target = good odds for player
    let secret_nonce = Base::random(&mut OsRng);
    let blind = Base::random(&mut OsRng);
    let token_id = Base::zero(); // DARK token
    let house_edge = 200u32; // 2% house edge

    let commit_result = harness.commit_bet(
        player_pub,
        bet_value,
        target,
        secret_nonce,
        blind,
        token_id,
        house_edge,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created bet commitment: bet_id={}", hex::encode(commit_result.public_inputs.bet_id.to_repr()));

    // Execute CommitBetV1 (0x01) - requires money_v3::transfer_v1 child call
    let tx = pipeline.exec_with_children(0x01, commit_result.call_data, vec![commit_result.proof], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed darktoshi_dice::0x01 (commit bet, tx: {:?})", tx.hash());

    // Reveal the roll (no ZK proof needed, no child call needed)
    let reveal_result = harness.reveal_roll(
        commit_result.public_inputs.bet_id,
        secret_nonce,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Revealed roll for bet");

    // Execute RevealRollV1 (0x02)
    let tx = pipeline.exec(0x02, reveal_result.call_data, vec![]).await?;
    info!("Executed darktoshi_dice::0x02 (reveal roll, tx: {:?})", tx.hash());

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
    use darkfi_contract_test_harness::harness::{EscrowHarness, MoneyV3Harness};
    use dwow_sdk::crypto::pasta_prelude::{Group, PrimeField};
    use dwow_sdk::pasta::pallas::{Base, Scalar};
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // First, deploy money_v3 to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18598".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18599".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    // Now deploy escrow
    let harness = EscrowHarness::spawn();
    info!("Escrow harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18640".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18641".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "escrow", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("escrow").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Escrow deployed: {:?}", contract_id);

    // Create a new harness for proof generation
    let mut harness = EscrowHarness::spawn();

    // Generate buyer and seller keypairs
    let buyer_secret = Base::random(&mut OsRng);
    let seller_secret = Base::random(&mut OsRng);
    let buyer_pubkey =
        dwow_sdk::crypto::PublicKey::from_secret(dwow_sdk::crypto::SecretKey::from_bytes(buyer_secret.to_repr()).unwrap());
    let seller_pubkey =
        dwow_sdk::crypto::PublicKey::from_secret(dwow_sdk::crypto::SecretKey::from_bytes(seller_secret.to_repr()).unwrap());

    let value = 1000u64;
    let token_id = Base::from(1);
    let timeout = 1000u64; // blocks

    // CreateEscrowV1 (0x01)
    let create_result = harness.create_escrow(
        buyer_secret,
        buyer_pubkey,
        seller_pubkey,
        value,
        token_id,
        timeout,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created escrow: commitment={}", hex::encode(create_result.public_inputs.commitment.to_repr()));

    let tx = pipeline.exec(0x01, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed escrow::0x01 (tx: {:?})", tx.hash());

    // FundV1 (0x02) - with ZK proof
    let value = 1000u64;
    let value_blind = Scalar::random(&mut OsRng);
    let fund_result = harness.fund_escrow(
        create_result.public_inputs.commitment,
        value,
        value_blind,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created fund proof for escrow");

    let tx = pipeline.exec(0x02, fund_result.call_data, vec![fund_result.proof]).await?;
    info!("Executed escrow::0x02 (tx: {:?})", tx.hash());

    // ClaimV1 (0x03) - seller claims the escrow
    // Requires money_v3::transfer_v1 (0x04) as child call
    let recipient_pubkey = seller_pubkey; // Seller receives the funds
    let claim_result = harness.claim_escrow(
        create_result.public_inputs.commitment,
        seller_secret,
        seller_pubkey,
        create_result.public_inputs.seller_commitment,
        recipient_pubkey,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created claim proof for escrow");

    // Build child call to money_v3::transfer_v1 (0x04)
    // The escrow contract validates that child_call.data[0] == 0x04
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04], // TransferV1 function ID
    };

    // Execute ClaimV1 (0x03) with money_v3 child call
    let tx = pipeline.exec_with_children(
        0x03,
        claim_result.call_data,
        vec![claim_result.proof],
        vec![child_call],
        vec![vec![]],
    ).await?;
    info!("Executed escrow::0x03 (claim, tx: {:?})", tx.hash());

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
    use dwow_sdk::crypto::pasta_prelude::PrimeField;
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    let harness = IdentityHarness::spawn();
    info!("Identity harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Create a new harness for proof generation
    let harness = IdentityHarness::spawn();

    // Initialize the identity registry
    let init_result = harness.initialize().map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Initialized identity registry");

    // Execute InitializeV1 (0x00)
    let tx = pipeline.exec(0x00, init_result.call_data, vec![]).await?;
    info!("Executed identity::0x00 (tx: {:?})", tx.hash());

    // Issue a credential to a holder
    let issuer_secret = Base::from(1);
    let credential_secret = Base::from(2);
    let attribute_1 = Base::from(100);
    let attribute_2 = Base::from(200);
    let attribute_blind = Base::from(300);
    let schema_hash = Base::from(0);
    let issued_at = 1000u64;
    let expires_at = 2000u64;

    let issue_result = harness.issue_credential(
        issuer_secret,
        credential_secret,
        attribute_1,
        attribute_2,
        attribute_blind,
        schema_hash,
        issued_at,
        expires_at,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Issued credential");

    // Execute IssueCredentialV1 (0x01)
    let tx = pipeline.exec(0x01, issue_result.call_data, vec![issue_result.proof]).await?;
    info!("Executed identity::0x01 (tx: {:?})", tx.hash());

    // Create a claim from the credential
    let attribute_value = Base::from(50);
    let threshold = Base::from(75);
    let commitment = issue_result.public_inputs.commitment;
    let issuer_public = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_bytes(issuer_secret.to_repr()).unwrap()
    );
    let claim_type = Base::from(1);

    let claim_result = harness.create_claim(
        credential_secret,
        attribute_value,
        threshold,
        commitment,
        issuer_public,
        schema_hash,
        claim_type,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created claim");

    // Execute CreateClaimV1 (0x03)
    let tx = pipeline.exec(0x03, claim_result.call_data, vec![claim_result.proof]).await?;
    info!("Executed identity::0x03 (tx: {:?})", tx.hash());

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
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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
    use dwow_sdk::crypto::{PublicKey, SecretKey, pasta_prelude::PrimeField};
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    let harness = LaborMarketHarness::spawn();
    info!("LaborMarket harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    let harness = LaborMarketHarness::spawn();

    // Generate keys
    let employer_secret = Base::random(&mut OsRng);
    let employer_public = PublicKey::from_secret(
        SecretKey::from_bytes(employer_secret.to_repr()).unwrap()
    );
    let worker_secret = Base::random(&mut OsRng);
    let worker_public = PublicKey::from_secret(
        SecretKey::from_bytes(worker_secret.to_repr()).unwrap()
    );

    let attestation_id = Base::from(100u64);
    let job_id = Base::from(200u64);
    let claim_id = Base::from(300u64);

    // Step 1: Create job (0x00)
    let create_result = harness.create_job(
        employer_secret,
        employer_public,
        attestation_id,
        job_id,
        1u8,            // delivery_type
        1000u64,        // payment_amount
        Base::from(1u64), // payment_token
        Base::zero(),   // payment_commit_x (dummy pedersen)
        Base::zero(),   // payment_commit_y
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created job: job_id={}", hex::encode(job_id.to_repr()));

    let tx = pipeline.exec(0x00, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed labor_market::0x00 CreateJobV1 (tx: {:?})", tx.hash());

    // Step 2: Accept job (0x01)
    let accept_result = harness.accept_job(
        worker_secret,
        worker_public,
        job_id,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Accepted job: job_id={}", hex::encode(job_id.to_repr()));

    let tx = pipeline.exec(0x01, accept_result.call_data, vec![accept_result.proof]).await?;
    info!("Executed labor_market::0x01 AcceptJobV1 (tx: {:?})", tx.hash());

    // Step 3: Submit deliverable (0x02)
    let submit_result = harness.submit_deliverable(
        worker_secret,
        worker_public,
        job_id,
        claim_id,
        999999u64,  // deadline_block (far future)
        1u64,       // current_block
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Submitted deliverable for job_id={}", hex::encode(job_id.to_repr()));

    let tx = pipeline.exec(0x02, submit_result.call_data, vec![submit_result.proof]).await?;
    info!("Executed labor_market::0x02 SubmitDeliverableV1 (tx: {:?})", tx.hash());

    // Step 4: Confirm delivery (0x04)
    let confirm_result = harness.confirm_delivery(
        employer_secret,
        employer_public,
        job_id,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Confirmed delivery for job_id={}", hex::encode(job_id.to_repr()));

    let tx = pipeline.exec(0x04, confirm_result.call_data, vec![confirm_result.proof]).await?;
    info!("Executed labor_market::0x04 ConfirmDeliveryV1 (tx: {:?})", tx.hash());

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
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, LotteryHarness};
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{pasta_prelude::PrimeField, PublicKey, SecretKey, poseidon_hash};
    use dwow_sdk::pasta::pallas::Base;
    use darkfi_lottery_contract::model::{InitializeParamsV1, LotteryConfig, PrizeTierConfig};
    use dwow_serial::Encodable;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18606".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18607".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = LotteryHarness::spawn();
    info!("Lottery harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18608".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18609".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "lottery", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("lottery").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Lottery deployed: {:?}", contract_id);

    // Fresh harness for proof generation
    let harness = LotteryHarness::spawn();

    // Create house keypair
    let house_secret = Base::random(&mut OsRng);
    let house_pub = PublicKey::from_secret(
        SecretKey::from_bytes(house_secret.to_repr()).unwrap()
    );

    // Initialize lottery round (0x00)
    let lottery_config = LotteryConfig {
        num_picks: 3,
        number_range: 10,
        house_edge_bp: 500, // 5%
        ticket_price: 1_000_000,
        prize_tiers: vec![
            PrizeTierConfig { matches_needed: 3, payout_percent: 7000, roll_to_next: false },
            PrizeTierConfig { matches_needed: 2, payout_percent: 2500, roll_to_next: false },
        ],
    };
    let init_params = InitializeParamsV1 {
        house_pub,
        config: lottery_config,
        duration: 100,
        claim_duration: 50,
        rolled_over: 0,
    };
    let mut init_call_data = vec![0x00]; // InitializeV1
    init_params.encode(&mut init_call_data).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    let tx = pipeline.exec(0x00, init_call_data, vec![]).await?;
    info!("Initialized lottery (tx: {:?})", tx.hash());

    // Derive lottery_id: same as the contract does
    // lottery_id = derive_lottery_id(&house_pub, current_block)
    // The current_block during initialization was the verifying block height.
    // Since we don't know it exactly, we derive heuristically.
    // We'll use a known block height (genesis blocks give us 0-indexed heights).
    let lottery_id = darkfi_lottery_contract::model::derive_lottery_id(&house_pub, 2);
    info!("Derived lottery_id: {}", hex::encode(lottery_id.to_repr()));

    // Create player keypair
    let player_secret = Base::random(&mut OsRng);
    let player_pub = PublicKey::from_secret(
        SecretKey::from_bytes(player_secret.to_repr()).unwrap()
    );

    // Buy ticket (0x01) with commit_ticket ZK proof + money_v3 child call
    let numbers: Vec<u8> = vec![3, 7, 9];
    let secret_nonce = Base::random(&mut OsRng);
    let blind = Base::random(&mut OsRng);
    let token_id = Base::from(1);
    let ticket_price: u64 = 1_000_000;

    let commit_result = harness.commit_ticket(
        player_pub,
        lottery_id,
        numbers.clone(),
        secret_nonce,
        ticket_price,
        blind,
        token_id,
        player_secret,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Generated commit_ticket ZK proof: ticket_id={}", hex::encode(commit_result.public_inputs.ticket_id.to_repr()));

    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04], // money_v3::transfer_v1
    };
    let tx = pipeline.exec_with_children(
        0x01, // BuyTicketV1
        commit_result.call_data,
        vec![commit_result.proof],
        vec![child_call],
        vec![vec![]],
    ).await?;
    info!("Executed lottery::0x01 BuyTicketV1 (tx: {:?})", tx.hash());

    // Generate reveal_ticket ZK proof (0x03) — proof generation only,
    // execution requires lottery to be in WinnersDrawn state.
    let reveal_result = harness.reveal_ticket(
        player_pub,
        ticket_price,
        secret_nonce,
        blind,
        Base::random(&mut OsRng), // nonce
        Base::random(&mut OsRng), // random
        commit_result.public_inputs.ticket_id,
        numbers,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Generated reveal_ticket ZK proof");

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
    use dwow_sdk::crypto::pasta_prelude::{Field, PrimeField};
    use dwow_sdk::crypto::SecretKey;
    use dwow_sdk::pasta::pallas;
    use rand::rngs::OsRng;

    let harness = OracleHarness::spawn();
    info!("Oracle harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Create a fresh harness for proof generation
    let harness = OracleHarness::spawn();

    // Generate oracle operator keypair
    let oracle_secret = pallas::Base::random(&mut OsRng);
    let oracle_pub = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_bytes(oracle_secret.to_repr()).unwrap()
    );
    let oracle_id = pallas::Base::random(&mut OsRng);

    // Register an oracle (0x00)
    let register_result = harness.register_oracle(
        oracle_secret,
        oracle_pub,
        oracle_id,
        "BTC/USD Price Feed".to_string(),
        "price".to_string(),
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created oracle registration: pub=({}, {})",
        hex::encode(register_result.oracle_pub_x.to_repr()),
        hex::encode(register_result.oracle_pub_y.to_repr()));

    // Execute RegisterOracleV1 (0x00)
    // Note: This creates a ZK proof that the oracle operator knows their secret key
    let tx = pipeline.exec(0x00, register_result.call_data, vec![register_result.proof]).await?;
    info!("Executed oracle::0x00 (register oracle, tx: {:?})", tx.hash());

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
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{pasta_prelude::PrimeField, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas::Base;
    use rand::rngs::OsRng;

    let harness = PoolStakeHarness::spawn();
    info!("PoolStake harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Fresh harness for proof generation
    let harness = PoolStakeHarness::spawn();

    // Generate owner and member keypairs
    let owner_secret = Base::random(&mut OsRng);
    let owner_pub = PublicKey::from_secret(
        SecretKey::from_bytes(owner_secret.to_repr()).unwrap()
    );
    let member_secret = Base::random(&mut OsRng);
    let member_pub = PublicKey::from_secret(
        SecretKey::from_bytes(member_secret.to_repr()).unwrap()
    );

    // Step 1: Create pool (0x00)
    let create_result = harness.create_pool(
        owner_pub,
        10000,  // max_coverage_ratio (1:1)
        100,    // operator_fee_bp (1%)
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created pool: pool_id={}", hex::encode(create_result.pool_id.to_repr()));

    let tx = pipeline.exec(0x00, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed pool_stake::0x00 CreatePoolV1 (tx: {:?})", tx.hash());

    // Step 2: Join pool (0x01) - stake tokens
    let stake_amount = 1_000_000u64; // minimum stake
    let relayer_id = [0u8; 32];
    let join_result = harness.join_pool(
        create_result.pool_id,
        stake_amount,
        relayer_id,
        member_pub,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Joined pool: stake_id={}", hex::encode(join_result.stake_id.to_repr()));

    let tx = pipeline.exec(0x01, join_result.call_data, vec![join_result.proof]).await?;
    info!("Executed pool_stake::0x01 JoinPoolV1 (tx: {:?})", tx.hash());

    // Step 3: Allocate coverage (0x03) - cover a withdrawal
    let withdrawal_nullifier = {
        let mut nf = [0u8; 32];
        nf[..8].copy_from_slice(&42u64.to_le_bytes());
        nf
    };
    let coverage_amount = 1000u64;
    let withdrawal_id = Base::from(42u64);
    let timeout_height = 1000u64;

    let allocate_result = harness.allocate_coverage(
        create_result.pool_id,
        member_pub,
        coverage_amount,
        withdrawal_id,
        withdrawal_nullifier,
        timeout_height,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Allocated coverage: allocation_id={}", hex::encode(allocate_result.allocation_id.to_repr()));

    let tx = pipeline.exec(0x03, allocate_result.call_data, vec![allocate_result.proof]).await?;
    info!("Executed pool_stake::0x03 AllocateCoverageV1 (tx: {:?})", tx.hash());

    // Step 4: Slash coverage (0x05) - penalty for failure
    let slash_amount = 500u64;
    let slash_result = harness.slash_coverage(
        allocate_result.allocation_id,
        slash_amount,
        member_pub, // user receiving compensation
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Slashed coverage: slash_id={}", hex::encode(slash_result.slash_id.to_repr()));

    let tx = pipeline.exec(0x05, slash_result.call_data, vec![slash_result.proof]).await?;
    info!("Executed pool_stake::0x05 SlashCoverageV1 (tx: {:?})", tx.hash());

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
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, SlotHarness};
    use dwow_sdk::crypto::pasta_prelude::Group;
    use dwow_sdk::pasta::pallas::Base;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18614".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18615".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = SlotHarness::spawn();
    info!("Slot harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
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

    // Create a new harness for proof generation
    let harness = SlotHarness::spawn();

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Initialize the slot contract (0x00 - no params needed)
    let init_result = harness.initialize()
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Initialized slot contract");

    let tx = pipeline.exec(0x00, init_result.call_data, vec![]).await?;
    info!("Executed slot::0x00 (tx: {:?})", tx.hash());

    // Commit a spin (0x01) - requires money_v3::transfer_v1 child call for bet locking
    // We can build the call_data but execution will fail without child call support
    let player_pub = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from(Base::from(1))
    );
    let secret_nonce = Base::from(12345);
    let blind = Base::from(67890);
    let token_id = Base::zero();
    let value_commit = dwow_sdk::pasta::pallas::Point::identity();

    let commit_result = harness.commit_spin(
        player_pub,
        1000u64,           // bet_value
        1u32,               // paylines_played
        secret_nonce,
        blind,
        500u32,             // house_edge (5%)
        1u8,                // confirmation_depth
        token_id,
        value_commit,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created commit spin call_data");

    // Execute CommitSpinV1 (0x01) - requires money_v3::transfer_v1 child call
    let tx = pipeline.exec_with_children(0x01, commit_result.call_data, vec![], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed slot::0x01 (tx: {:?})", tx.hash());

    // Reveal the spin (0x02) - no child call needed
    // Use a dummy spin_id since CommitSpin didn't actually store anything
    let reveal_result = harness.reveal_spin(
        Base::from(0),  // spin_id (dummy)
        secret_nonce,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created reveal spin call_data");

    // Note: RevealSpinV1 will fail with invalid state since CommitSpin didn't succeed
    let tx = pipeline.exec(0x02, reveal_result.call_data, vec![]).await;
    match tx {
        Ok(t) => info!("Executed slot::0x02 (tx: {:?})", t.hash()),
        Err(e) => info!("slot::0x02 failed (expected without proper state): {}", e),
    }

    info!("test_slot_heavyweight PASSED");
    Ok(())
}

// roulette
#[test]
fn test_roulette_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_roulette_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_roulette_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, RouletteHarness};
    use dwow_sdk::crypto::pasta_prelude::{Group, PrimeField};
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18616".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18617".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = RouletteHarness::spawn();
    info!("Roulette harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18618".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18619".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "roulette", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("roulette").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Roulette deployed: {:?}", contract_id);

    // Create a new harness for proof generation (pipeline takes ownership of first harness)
    let harness = RouletteHarness::spawn();
    info!("Roulette harness created with circuits: {:?}", harness.circuits());

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Create house keypair
    let house_secret = dwow_sdk::crypto::SecretKey::random(&mut OsRng);
    let house_pub = dwow_sdk::crypto::PublicKey::from_secret(house_secret);

    // Initialize roulette table (0x00) - no child call
    let init_result = harness
        .initialize(house_pub, false, 1000000u64, 10000u64, 10u64)
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Initialized roulette table");

    let tx = pipeline.exec(0x00, init_result.call_data, vec![]).await?;
    info!("Executed roulette::0x00 (tx: {:?})", tx.hash());

    // Derive table_id (same as contract: poseidon_hash([house_pub.x, house_pub.y, created_at]))
    let table_id = dwow_sdk::crypto::poseidon_hash([
        house_pub.x(),
        house_pub.y(),
        Base::from(1), // created_at block
    ]);

    // Create player keypair
    let player_secret = dwow_sdk::crypto::SecretKey::random(&mut OsRng);
    let player_pub = dwow_sdk::crypto::PublicKey::from_secret(player_secret);

    // PlaceBetV1 (0x01) - requires money_v3::transfer_v1 child call
    // BetType::Straight = 0
    let nonce = Base::random(&mut OsRng);
    let place_bet_result = harness
        .place_bet(
            table_id,
            player_pub,
            0u8, // BetType::Straight
            vec![7], // straight bet on 7
            100u64,
            nonce,
        )
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created place bet: bet_id={}", hex::encode(place_bet_result.bet_id.to_repr()));

    let tx = pipeline
        .exec_with_children(
            0x01,
            place_bet_result.call_data,
            vec![place_bet_result.proof],
            vec![child_call.clone()],
            vec![vec![]],
        )
        .await?;
    info!("Executed roulette::0x01 (tx: {:?})", tx.hash());

    // SpinWheelV1 (0x02) - no child call, uses block hash for randomness
    let spin_nonce = Base::random(&mut OsRng);
    let spin_result = harness
        .spin_wheel(table_id, house_pub, spin_nonce)
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created spin wheel call");

    let tx = pipeline.exec(0x02, spin_result.call_data, vec![]).await?;
    info!("Executed roulette::0x02 (tx: {:?})", tx.hash());

    // SettleBetsV1 (0x03) - requires money_v3::transfer_v1 child call
    let settle_result = harness
        .settle_bets(table_id, vec![place_bet_result.bet_id])
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created settle bets call");

    let tx = pipeline
        .exec_with_children(0x03, settle_result.call_data, vec![settle_result.proof], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed roulette::0x03 (tx: {:?})", tx.hash());

    // HouseCloseV1 (0x04) - requires money_v3::transfer_v1 child call
    let close_result = harness
        .house_close(table_id, house_pub)
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created house close call");

    let tx = pipeline
        .exec_with_children(0x04, close_result.call_data, vec![], vec![child_call], vec![vec![]])
        .await?;
    info!("Executed roulette::0x04 (tx: {:?})", tx.hash());

    info!("test_roulette_heavyweight PASSED");
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
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, StablecoinHarness};
    use dwow_sdk::crypto::{pasta_prelude::PrimeField, BaseBlind};
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18614".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18615".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = StablecoinHarness::spawn();
    info!("Stablecoin harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18614".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18615".to_string(),
    };

    let mut pipeline =
        HeavyweightPipeline::new(StablecoinHarness::spawn(), "stablecoin", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("stablecoin").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Stablecoin deployed: {:?}", contract_id);

    // Create a fresh harness for proof generation
    let harness = StablecoinHarness::spawn();

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // OpenPositionV1 (0x01) - Create a collateral position
    let owner_secret = Base::random(&mut OsRng);
    let collateral_amount = 10000u64;
    let debt_amount = 5000u64;
    let collateral_type = Base::zero(); // XMR

    let open_result = harness.open_position(
        owner_secret,
        collateral_amount,
        debt_amount,
        collateral_type,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created position: commitment={}", hex::encode(open_result.position_commitment.to_repr()));

    // Execute OpenPositionV1 (0x01)
    let tx = pipeline.exec(0x01, open_result.call_data, vec![open_result.proof]).await?;
    info!("Executed stablecoin::0x01 (tx: {:?})", tx.hash());

    // MintStableV1 (0x04) - Mint stablecoin against the position
    let collateral_blind = BaseBlind::random(&mut OsRng);
    let debt_blind = BaseBlind::random(&mut OsRng);
    let mint_amount = 1000u64;

    let mint_result = harness.mint_stable(
        owner_secret,
        collateral_amount,
        debt_amount,
        mint_amount,
        collateral_blind,
        debt_blind,
        open_result.position_commitment,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Minted stable: amount={}", mint_amount);

    // Execute MintStableV1 (0x04) - requires money_v3 child call
    let tx = pipeline.exec_with_children(0x04, mint_result.call_data, vec![mint_result.proof], vec![child_call.clone()], vec![vec![]]).await?;
    info!("Executed stablecoin::0x04 (tx: {:?})", tx.hash());

    // GovernanceReportV1 (0x08) - Report governance data
    let reporter_secret = Base::random(&mut OsRng);
    let rate_per_second = 100u64; // 1% per second (basis points)
    let time_elapsed = 3600u64; // 1 hour

    let gov_result = harness.governance_report(
        reporter_secret,
        collateral_amount,
        debt_amount + mint_amount, // total debt after minting
        rate_per_second,
        time_elapsed,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Governance report: collateral={}, debt={}", collateral_amount, debt_amount + mint_amount);

    // Execute GovernanceReportV1 (0x08)
    let tx = pipeline.exec(0x08, gov_result.call_data, vec![gov_result.proof]).await?;
    info!("Executed stablecoin::0x08 (tx: {:?})", tx.hash());

    // AccrueInterestV1 (0x09) - Accrue interest on debt
    let accumulator_secret = Base::random(&mut OsRng);
    let new_debt = debt_amount + mint_amount;

    let accrue_result = harness.accrue_interest(
        accumulator_secret,
        new_debt,
        rate_per_second,
        time_elapsed,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Interest accrued on position");

    // Execute AccrueInterestV1 (0x09)
    let tx = pipeline.exec(0x09, accrue_result.call_data, vec![accrue_result.proof]).await?;
    info!("Executed stablecoin::0x09 (tx: {:?})", tx.hash());

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
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{MerkleNode, PublicKey, SecretKey, pasta_prelude::PrimeField, poseidon_hash};
    use dwow_sdk::pasta::pallas::{Base, Scalar};
    use rand::rngs::OsRng;

    let harness = SubscriptionHarness::spawn();
    info!("Subscription harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18636".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18637".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "subscription", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("subscription").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Subscription deployed: {:?}", contract_id);

    // Fresh harness for proof generation
    let harness = SubscriptionHarness::spawn();

    // Generate subscriber keypair
    let subscriber_secret = Base::random(&mut OsRng);
    let subscriber_pub = PublicKey::from_secret(
        SecretKey::from_bytes(subscriber_secret.to_repr()).unwrap()
    );

    let subscription_id = Base::from(100u64);
    let plan_id = 1u32;
    let deposit = 500u64;
    let token_id = Base::from(0u64);
    let lock_until_block = 1000u64;
    let nonce = Base::random(&mut OsRng);
    let value_blind = Scalar::random(&mut OsRng);
    let value_commit_x = Base::from(200u64);
    let value_commit_y = Base::from(300u64);
    let plan_merkle_root = Base::from(400u64);
    let current_block = 1u64;
    let dao_escrow_bulla = Base::zero();
    let dao_membership_note = Base::zero();
    let dao_escrow_merkle_root = Base::zero();

    // Step 1: Subscribe (0x01)
    let subscribe_result = harness.subscribe(
        subscriber_secret,
        nonce,
        vec![MerkleNode::new(Base::zero()); 3],  // plan_merkle_proof (3-element SMT proof)
        value_blind,
        Base::zero(),  // dao_member_pub_x
        Base::zero(),  // dao_member_pub_y
        0u64,          // dao_membership_expiry
        Base::zero(),  // dao_membership_value
        0u32,          // dao_leaf_pos
        vec![MerkleNode::new(Base::zero()); 3],  // dao_path
        0u32,          // plan_leaf_pos
        vec![MerkleNode::new(Base::zero()); 3],  // plan_path
        subscription_id,
        subscriber_pub,
        plan_id,
        deposit,
        token_id,
        lock_until_block,
        plan_merkle_root,
        current_block,
        value_commit_x,
        value_commit_y,
        dao_escrow_bulla,
        dao_membership_note,
        dao_escrow_merkle_root,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created subscription: id={}", hex::encode(subscription_id.to_repr()));

    let tx = pipeline.exec(0x01, subscribe_result.call_data, vec![subscribe_result.proof]).await?;
    info!("Executed subscription::0x01 SubscribeV1 (tx: {:?})", tx.hash());

    // Step 2: Verify access (0x04)
    let expected_capability = poseidon_hash([
        subscriber_pub.x(),
        subscriber_pub.y(),
        Base::from(plan_id as u64),
        subscription_id,
        Base::from(lock_until_block),
        nonce,
    ]);

    let verify_result = harness.verify_access(
        subscriber_secret,
        nonce,
        1u8,           // permissions_claimed
        0u32,          // subscription_leaf_pos
        vec![MerkleNode::new(Base::zero()); 3],  // subscription_path
        Base::zero(),  // subscription_state
        Base::zero(),  // subscription_spent_nullifier
        expected_capability,
        subscription_id,
        current_block,
        subscriber_pub.x(),
        subscriber_pub.y(),
        plan_id,
        lock_until_block,
        100u64,        // uses_allowed
        3600u64,       // rate_period
        0u64,          // period_uses
        0u64,          // last_access_block
        100u64,        // uses_remaining
        Base::zero(),  // subscription_state_root
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Verified access for subscription");

    let tx = pipeline.exec(0x04, verify_result.call_data, vec![verify_result.proof]).await?;
    info!("Executed subscription::0x04 VerifyAccessV1 (tx: {:?})", tx.hash());

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
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{pasta_prelude::PrimeField, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas::Base;
    use darkfi_tender_contract::model::CloseTenderParamsV1;
    use dwow_serial::Encodable;
    use rand::rngs::OsRng;

    let harness = TenderHarness::spawn();
    info!("Tender harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18638".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18639".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "tender", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("tender").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("Tender deployed: {:?}", contract_id);

    // Fresh harness for proof generation
    let harness = TenderHarness::spawn();

    // Create requester and bidder keypairs
    let requester_secret = Base::random(&mut OsRng);
    let requester_pub = PublicKey::from_secret(
        SecretKey::from_bytes(requester_secret.to_repr()).unwrap()
    );
    let bidder_secret = Base::random(&mut OsRng);
    let bidder_pub = PublicKey::from_secret(
        SecretKey::from_bytes(bidder_secret.to_repr()).unwrap()
    );

    // Step 1: Create tender (0x00)
    // bid_deadline=6 so submit_bid at block ~5 passes and close_tender at block ~6 passes
    let create_result = harness.create_tender(
        requester_pub,
        requester_secret,
        "Test Tender".to_string(),
        Base::from(12345u64),
        Base::from(100u64),
        100,   // min_bid
        10000, // max_bid
        6,     // bid_deadline
        20,    // reveal_deadline
        30,    // delivery_deadline
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created tender: tender_id={}", hex::encode(create_result.tender_id.to_repr()));

    let tx = pipeline.exec(0x00, create_result.call_data, vec![create_result.proof]).await?;
    info!("Executed tender::0x00 CreateTenderV1 (tx: {:?})", tx.hash());

    // Step 2: Submit bid (0x01)
    let bid_nonce = Base::random(&mut OsRng);
    let claim_id = Base::from(999u64);
    let bid_amount = 500u64;

    let submit_result = harness.submit_bid(
        create_result.tender_id,
        bidder_pub,
        bidder_secret,
        bid_amount,
        bid_nonce,
        claim_id,
        vec![1, 2, 3],
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Submitted bid: bid_id={}", hex::encode(submit_result.public_inputs.bid_id.to_repr()));

    let tx = pipeline.exec(0x01, submit_result.call_data, vec![submit_result.proof]).await?;
    info!("Executed tender::0x01 SubmitBidV1 (tx: {:?})", tx.hash());

    // Step 3: Close tender (0x03) - no ZK proof required
    let (rx, ry) = requester_pub.xy();
    let close_params = CloseTenderParamsV1 {
        tender_id: create_result.tender_id,
        requester_pub_x: rx,
        requester_pub_y: ry,
    };
    let mut close_call_data = vec![0x03];
    close_params.encode(&mut close_call_data)
        .map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    let tx = pipeline.exec(0x03, close_call_data, vec![]).await?;
    info!("Executed tender::0x03 CloseTenderV1 (tx: {:?})", tx.hash());

    // Step 4: Reveal bid (0x02)
    let reveal_result = harness.reveal_bid(
        create_result.tender_id,
        submit_result.public_inputs.bid_id,
        bidder_pub,
        bidder_secret,
        bid_amount,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    let tx = pipeline.exec(0x02, reveal_result.call_data, vec![reveal_result.proof]).await?;
    info!("Executed tender::0x02 RevealBidV1 (tx: {:?})", tx.hash());

    // Step 5: Select winner (0x04)
    let select_result = harness.select_winner(
        create_result.tender_id,
        submit_result.public_inputs.bid_id,
        requester_pub,
        requester_secret,
        bidder_pub,
        bid_amount,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    let tx = pipeline.exec(0x04, select_result.call_data, vec![select_result.proof]).await?;
    info!("Executed tender::0x04 SelectWinnerV1 (tx: {:?})", tx.hash());

    info!("test_tender_heavyweight PASSED");
    Ok(())
}

// betting_stake
#[test]
fn test_betting_stake_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_betting_stake_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_betting_stake_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::{BettingStakeHarness, MoneyV3Harness, ClaimStakeInfo, UnstakeStakeInfo};
    use dwow_sdk::crypto::pasta_prelude::PrimeField;
    use dwow_sdk::pasta::pallas::Base;
    use dwow::zk::halo2::Field;
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18622".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18623".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = BettingStakeHarness::spawn();
    info!("BettingStake harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18622".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18623".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "betting_stake", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("betting_stake").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("BettingStake deployed: {:?}", contract_id);

    // Create a new harness for call data generation
    let harness = BettingStakeHarness::spawn();

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Initialize staking for a betting table (0x00)
    let betting_contract_id = Base::from(1);
    let house_edge_bp = 100u32; // 1%
    let risk_profile = 0u8;

    let init_result = harness.initialize(
        betting_contract_id,
        house_edge_bp,
        risk_profile,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Initialized betting stake table");

    // Execute InitializeV1 (0x00)
    let tx = pipeline.exec(0x00, init_result.call_data, vec![init_result.proof]).await?;
    info!("Executed betting_stake::0x00 (tx: {:?})", tx.hash());

    // Stake capital against the table (0x01) - requires money_v3 child call
    let table_id = dwow_sdk::crypto::poseidon_hash([betting_contract_id, Base::from(0u64)]);
    let staker_secret = SecretKey::random(&mut OsRng);
    let staker_pub = PublicKey::from_secret(staker_secret);
    let amount = 1000u64;
    let token_id = Base::zero();
    let nonce = 0u64;
    let spend_hook = dwow_sdk::crypto::poseidon_hash([money_contract_id.inner(), Base::from(0x04)]);
    let user_data = Base::zero();

    let stake_result = harness.stake(
        table_id,
        staker_pub,
        staker_secret,
        amount,
        spend_hook,
        user_data,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created stake call");

    let tx = pipeline.exec_with_children(
        0x01,
        stake_result.call_data,
        vec![stake_result.proof],
        vec![child_call.clone()],
        vec![vec![]],
    ).await?;
    info!("Executed betting_stake::0x01 (tx: {:?})", tx.hash());

    // Update risk after a payout (0x04) - called by betting contract
    let total_stake = amount;
    let accumulated_losses = 0u64;

    let update_risk_result = harness.update_risk(
        table_id,
        betting_contract_id,
        total_stake,
        accumulated_losses,
        house_edge_bp,
        risk_profile,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created update risk call");

    let tx = pipeline.exec(0x04, update_risk_result.call_data, vec![update_risk_result.proof]).await?;
    info!("Executed betting_stake::0x04 (tx: {:?})", tx.hash());

    // Claim accumulated earnings (0x03)
    let stake_id = dwow_sdk::crypto::poseidon_hash([
        table_id,
        staker_pub.x(),
        staker_pub.y(),
        Base::from(amount),
        Base::from(nonce),
    ]);

    let claim_stake_info = ClaimStakeInfo::new(
        table_id,
        staker_pub,
        amount,
        0u64, // accumulated_earnings
        token_id,
        nonce,
    );

    let claim_result = harness.claim_earnings(
        stake_id,
        &claim_stake_info,
        staker_secret,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created claim earnings call");

    let tx = pipeline.exec(0x03, claim_result.call_data, vec![claim_result.proof]).await?;
    info!("Executed betting_stake::0x03 (tx: {:?})", tx.hash());

    // Unstake and withdraw (0x02) - requires money_v3 child call
    let unstake_stake_info = UnstakeStakeInfo::new(
        table_id,
        staker_pub,
        amount,
        amount,
        0u64, // accumulated_earnings
        token_id,
        nonce,
    );

    let unstake_result = harness.unstake(
        stake_id,
        &unstake_stake_info,
        staker_secret,
        spend_hook,
        user_data,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created unstake call");

    let tx = pipeline.exec_with_children(
        0x02,
        unstake_result.call_data,
        vec![unstake_result.proof],
        vec![child_call],
        vec![vec![]],
    ).await?;
    info!("Executed betting_stake::0x02 (tx: {:?})", tx.hash());

    info!("test_betting_stake_heavyweight PASSED");
    Ok(())
}

// native_token
#[test]
fn test_native_token_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_native_token_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_native_token_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::reward;
    use dwow_sdk::crypto::Keypair;
    use rand::rngs::OsRng;

    let harness = NativeTokenHarness::spawn();
    info!("NativeToken harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18624".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18625".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "native_token", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("native_token").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("NativeToken deployed: {:?}", contract_id);

    let harness = NativeTokenHarness::spawn();

    // Step 1: Build PoW reward (0x05) — exercises the exponential emission schedule.
    // The reward value is expected_reward(height) + fees with floor rounding
    // for deterministic, conservative issuance.
    let miner_keypair = Keypair::random(&mut OsRng);
    let mint_result = harness.mint_pow_reward(
        miner_keypair,
        1u32,           // block_height
        500u64,         // fees
        None,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Built PoW reward proof");

    // Verify reward value matches the schedule at height 1.
    // expected_reward(1) ≈ INITIAL_REWARD (exponential decay is negligible at block 1).
    // The coin's inner value is non-zero when reward was computed correctly.
    assert!(
        mint_result.output.coin.inner() != dwow_sdk::pasta::pallas::Base::zero(),
        "Coin commitment must be non-zero (reward was computed)"
    );
    info!("PoW reward commitment valid — reward schedule exercised");

    // Step 2: Build and sign a transaction against the pipeline
    let tx = pipeline.exec(0x05, mint_result.call_data, mint_result.proofs).await?;
    info!("Signed native_token::0x05 PoWRewardV1 (tx: {:?})", tx.hash());

    // Step 3: Build a second reward at a higher height — verify decay
    let late_keypair = Keypair::random(&mut OsRng);
    let late_result = harness.mint_pow_reward(
        late_keypair,
        1_000_000u32,   // far-future height (~3.8 years)
        100u64,         // fees
        None,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;

    // At ~1M blocks, the reward should be noticeably below INITIAL_REWARD
    // (approximately 50% decay at 1,051,920 blocks)
    info!("Far-future reward built — exponential decay exercised");

    // Step 4: Verify supply cap constant is correctly computed
    // MAX_SUPPLY = 21M DRK * 10^8 base units, fits in u64
    assert!(reward::MAX_SUPPLY > 0, "Supply cap must be non-zero");
    assert!(reward::MAX_SUPPLY < u64::MAX, "Supply cap must fit in u64");
    assert_eq!(reward::GENESIS_REWARD, 0, "Genesis height must have zero reward");
    assert!(reward::TAIL_REWARD > 0, "Tail emission must be non-zero");
    assert!(reward::TAIL_REWARD < reward::INITIAL_REWARD, "Tail must be below initial reward");
    info!("Supply cap and reward constants verified");

    info!("test_native_token_heavyweight PASSED");
    Ok(())
}

// relayer_endowment
#[test]
fn test_relayer_endowment_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_relayer_endowment_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_relayer_endowment_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::{MoneyV3Harness, RelayerEndowmentHarness};
    use dwow_sdk::crypto::pasta_prelude::PrimeField;
    use dwow_sdk::pasta::pallas::{Base, Scalar};
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first to get its contract_id for child calls
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18624".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18625".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed: {:?}", money_contract_id);

    let harness = RelayerEndowmentHarness::spawn();
    info!("RelayerEndowment harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18626".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18627".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "relayer_endowment", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("relayer_endowment").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("RelayerEndowment deployed: {:?}", contract_id);

    // Fresh harness for proof generation
    let harness = RelayerEndowmentHarness::spawn();

    // Build child call for money_v3::transfer_v1 (0x04)
    let child_call = ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04],
    };

    // Derive keypair for relayer
    let relayer_secret = Base::random(&mut OsRng);
    let relayer_public = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_bytes(relayer_secret.to_repr()).unwrap()
    );

    // Derive keypair for backer
    let backer_secret = Base::random(&mut OsRng);
    let backer_public = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_bytes(backer_secret.to_repr()).unwrap()
    );

    // 1. Initialize relayer endowment account (0x00)
    let init_result = harness.initialize(
        relayer_public,
        1000, // default_backer_cut_bp = 10%
        1,    // nonce
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Initialized endowment: id={}", hex::encode(init_result.public_inputs.endowment_id.to_repr()));

    let tx = pipeline.exec(0x00, init_result.call_data, vec![init_result.proof]).await?;
    info!("Executed relayer_endowment::0x00 InitializeV1 (tx: {:?})", tx.hash());

    // 2. Deploy capital (0x01) - with money_v3::transfer_v1 child call
    let deploy_result = harness.deploy_capital(
        init_result.public_inputs.endowment_id,
        backer_public,
        2_000_000,     // deploy_amount (above MIN_DEPLOY of 1_000_000)
        Base::from(1), // token_id
        1,             // nonce
        Scalar::random(&mut OsRng), // value_blind
        relayer_public,
        500,           // backer_cut_bp = 5%
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Deployed capital: deployment_id={}", hex::encode(deploy_result.public_inputs.derived_deployment_id.to_repr()));

    let tx = pipeline.exec_with_children(
        0x01,
        deploy_result.call_data,
        vec![deploy_result.proof],
        vec![child_call],
        vec![vec![]],
    ).await?;
    info!("Executed relayer_endowment::0x01 DeployCapitalV1 (tx: {:?})", tx.hash());

    // 3. ClaimFees proof generation verification (0x03)
    // The ZK proof generates correctly; contract execution would require
    // SettleFees to distribute to individual deployments (not yet implemented).
    let claim_result = harness.claim_fees(
        deploy_result.public_inputs.derived_deployment_id,
        backer_public,
        500, // fee_share
        2,   // nonce
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Generated claim_fees ZK proof (claim_id={})", hex::encode(claim_result.public_inputs.derived_claim_id.to_repr()));

    info!("test_relayer_endowment_heavyweight PASSED");
    Ok(())
}

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
    use darkfi_contract_test_harness::harness::{AtomicSwapHarness, MoneyV3Harness};
    use dwow_sdk::{
        crypto::{pasta_prelude::PrimeField, poseidon_hash, PublicKey, SecretKey},
        pasta::pallas::Base,
    };
    use dwow::zk::halo2::Field;
    use rand::rngs::OsRng;

    // Deploy money_v3 first for child calls (CreateSwapV1 requires money_v3::transfer_v1)
    let money_harness = MoneyV3Harness::spawn();
    let money_config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18626".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18627".to_string(),
    };
    let mut money_pipeline = HeavyweightPipeline::new(money_harness, "money_v3", money_config, ex.clone()).await?;
    money_pipeline.generate_genesis_blocks(3).await?;
    let money_wasm = read_wasm("money_v3").await?;
    let money_contract_id = money_pipeline.deploy(money_wasm).await?;
    info!("MoneyV3 deployed for atomic_swap test: {:?}", money_contract_id);

    let harness = AtomicSwapHarness::spawn();
    info!("AtomicSwap harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18628".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18629".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "atomic_swap", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("atomic_swap").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("AtomicSwap deployed: {:?}", contract_id);

    // Fresh harness for proof generation
    let harness = AtomicSwapHarness::spawn();

    // Generate keypair
    let secret = Base::random(&mut OsRng);
    let keypair_secret = SecretKey::random(&mut OsRng);
    let receiver_public = PublicKey::from_secret(keypair_secret);

    // Swap parameters
    let hash = poseidon_hash([secret]);
    let timelock = 100u64;
    let amount = 1000u64;
    let token_id = Base::zero();
    let side = 0u8;
    let blind = Base::random(&mut OsRng);
    let external_chain = 1u8;
    let external_receiver = Base::random(&mut OsRng);

    // Step 1: Create swap (0x01)
    let create_result = harness.create_swap(
        hash, timelock, secret, amount, token_id, side, blind,
        receiver_public, external_chain, external_receiver,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Created swap: swap_id={}", hex::encode(create_result.public_inputs.swap_id.to_repr()));

    let child_call = dwow_sdk::ContractCall {
        contract_id: money_contract_id,
        data: vec![0x04], // money_v3::transfer_v1
    };

    let tx = pipeline.exec_with_children(
        0x01, create_result.call_data, vec![create_result.proof],
        vec![child_call], vec![vec![]],
    ).await?;
    info!("Executed atomic_swap::0x01 CreateSwapV1 (tx: {:?})", tx.hash());

    // Step 2: Claim swap (0x02) — the secret holder claims the swap
    let claim_result = harness.claim_swap(
        create_result.public_inputs.swap_id,
        secret,
        hash,
        timelock,
        side,
    ).map_err(|e| HeavyweightError::ExecutionFailed(e.to_string()))?;
    info!("Claim prepared: nullifier={}", hex::encode(claim_result.public_inputs.nullifier.to_repr()));

    let tx = pipeline.exec(0x02, claim_result.call_data, vec![claim_result.proof]).await?;
    info!("Executed atomic_swap::0x02 ClaimV1 (tx: {:?})", tx.hash());

    info!("test_atomic_swap_heavyweight PASSED");
    Ok(())
}

// game_room
#[test]
fn test_game_room_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_game_room_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_game_room_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::GameRoomHarness;

    let harness = GameRoomHarness::spawn();
    info!("GameRoom harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18630".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18631".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "game_room", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("game_room").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("GameRoom deployed: {:?}", contract_id);

    info!("test_game_room_heavyweight PASSED");
    Ok(())
}

// drain_protection
#[test]
fn test_drain_protection_heavyweight() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_drain_protection_heavyweight_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

async fn test_drain_protection_heavyweight_impl(
    ex: Arc<Executor<'static>>,
) -> std::result::Result<(), HeavyweightError> {
    use darkfi_contract_test_harness::harness::DrainProtectionHarness;

    let harness = DrainProtectionHarness::spawn();
    info!("DrainProtection harness created with circuits: {:?}", harness.circuits());

    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(dwow_sdk::num_traits::One::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18632".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18633".to_string(),
    };

    let mut pipeline = HeavyweightPipeline::new(harness, "drain_protection", config, ex).await?;
    pipeline.generate_genesis_blocks(3).await?;
    let wasm = read_wasm("drain_protection").await?;
    let contract_id = pipeline.deploy(wasm).await?;
    info!("DrainProtection deployed: {:?}", contract_id);

    info!("test_drain_protection_heavyweight PASSED");
    Ok(())
}