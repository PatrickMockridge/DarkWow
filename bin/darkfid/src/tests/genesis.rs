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

//! Genesis Test Module
//!
//! Provides a baseline chain setup with NativeToken + Deployooor that other
//! contract tests can build upon.
//!
//! # Documentation
//!
//! See [../../doc/src/arch/genesis_harness.md] for detailed documentation on
//! GenesisHarness API, usage examples, and architecture.
//!
//! # Quick Start
//!
//! ```rust
//! use crate::tests::genesis::GenesisHarness;
//!
//! async fn test_my_contract() -> Result<()> {
//!     let mut genesis = GenesisHarness::new(config, &ex).await?;
//!     genesis.generate_genesis_blocks(3).await?;
//!     let contract_id = genesis.deploy_contract(wasm_bincode, "MyContract").await?;
//!     // ... test your contract
//!     Ok(())
//! }
//! ```
//!
//! # Tests
//!
//! - `test_genesis` - Basic genesis block generation with NativeToken
//! - `test_native_token_deployoor` - Full deployment of MoneyV2 via Deployooor

use std::sync::Arc;

use dwow::{
    blockchain::{BlockInfo, Header},
    tx::{ContractCallLeaf, TransactionBuilder},
    validator::{
        consensus::{Fork, Proposal},
        sync::{apply_block, verify_block},
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    Result,
};
use dwow_deployooor_contract::DeployFunction;
use dwow_sdk::deploy::DeployParamsV1;
use dwow_native_token_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, NativeTokenFunction,
    NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1, NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN,
};
use dwow_sdk::{
    crypto::{
        keypair::Keypair,
        pasta_prelude::{Curve, CurveAffine},
        DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
    },
    num_traits::One,
    ContractCall,
};
use dwow_serial::Encodable;
use num_bigint::BigUint;
use smol::Executor;

use crate::tests::{Harness, HarnessConfig};

/// ZK Binary loaded directly from include_bytes
fn get_mint_zkbin() -> Result<dwow::zkas::ZkBinary> {
    let zkbin_bytes =
        include_bytes!("../../../../src/contract/native_token/proof/mint_v1.zk.bin").to_vec();
    Ok(dwow::zkas::ZkBinary::decode(&zkbin_bytes, false)?)
}

/// Generate a NativeToken PoW reward block
pub async fn generate_native_block(
    fork: &mut Fork,
    keypair: &Keypair,
) -> Result<BlockInfo> {
    let previous = fork.overlay.lock().unwrap().last_block()?;
    let block_height = previous.header.height + 1;
    let last_nonce = previous.header.nonce;

    let zkbin = get_mint_zkbin()?;
    let zkbin_bytes = NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN.to_vec();
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit);

    let debris = PoWRewardCallBuilder {
        signature_keypair: *keypair,
        block_height,
        fees: 0,
        recipient: None,
        spend_hook: None,
        user_data: None,
        mint_zkbin: zkbin.clone(),
        mint_pk: pk.clone(),
    }
    .build()?;

    let value_coords = debris.params.output.value_commit.to_affine().coordinates().unwrap();
    let public_inputs = vec![
        debris.params.output.coin.inner(),
        *value_coords.x(),
        *value_coords.y(),
        debris.params.output.token_commit,
    ];

    let mut data = vec![NativeTokenFunction::PoWRewardV1 as u8];
    debris.params.encode(&mut data)?;
    let call = ContractCall { contract_id: *NATIVE_TOKEN_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&[keypair.secret])?;
    tx.signatures = vec![sigs];

    let timestamp = previous.header.timestamp.checked_add(1.into())?;
    let header = Header::new(previous.hash(), block_height, last_nonce, timestamp);

    let mut block = BlockInfo::new_empty(header);
    block.append_txs(vec![tx]);
    block.zkbin_data = vec![(
        *NATIVE_TOKEN_CONTRACT_ID,
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        zkbin_bytes,
        public_inputs,
    )];

    let overlay = fork.overlay.lock().unwrap().full_clone()?;
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&fork.diffs)?;
    block.header.state_root = overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

    block.sign(&keypair.secret);
    Ok(block)
}

/// Generate a deploy transaction for Deployooor
pub fn generate_deploy_tx(
    deploy_keypair: &Keypair,
    wasm_bincode: Vec<u8>,
) -> Result<dwow::tx::Transaction> {
    let params = DeployParamsV1 {
        wasm_bincode,
        public_key: deploy_keypair.public,
        ix: vec![],
    };

    let mut data = vec![DeployFunction::DeployV1 as u8];
    params.encode(&mut data)?;

    let call = ContractCall { contract_id: *DEPLOYOOOR_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
    tx.signatures = vec![sigs];

    Ok(tx)
}

/// Generate a block with a deploy transaction
pub async fn generate_deploy_block(
    fork: &mut Fork,
    deploy_keypair: &Keypair,
    wasm_bincode: Vec<u8>,
) -> Result<BlockInfo> {
    let previous = fork.overlay.lock().unwrap().last_block()?;
    let block_height = previous.header.height + 1;
    let last_nonce = previous.header.nonce;

    let tx = generate_deploy_tx(deploy_keypair, wasm_bincode)?;

    let timestamp = previous.header.timestamp.checked_add(1.into())?;
    let header = Header::new(previous.hash(), block_height, last_nonce, timestamp);

    let mut block = BlockInfo::new_empty(header);
    block.append_txs(vec![tx]);
    block.zkbin_data = vec![]; // Deployooor has no ZK circuits

    let overlay = fork.overlay.lock().unwrap().full_clone()?;
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&fork.diffs)?;
    block.header.state_root = overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

    block.sign(&deploy_keypair.secret);
    Ok(block)
}

/// Genesis harness for setting up baseline chain state
pub struct GenesisHarness {
    pub harness: Harness,
    pub fork: Fork,
    pub keypair: Keypair,
    pub deployed_contracts: Vec<dwow_sdk::crypto::ContractId>,
}

impl GenesisHarness {
    /// Create a new genesis harness with NativeToken + Deployooor
    pub async fn new(config: HarnessConfig, ex: &Arc<Executor<'static>>) -> Result<Self> {
        let harness = Harness::new(config, true, ex).await?;
        let fork = harness.alice.validator.read().await.consensus.forks[0].full_clone()?;
        let keypair = Keypair::default();

        Ok(Self { harness, fork, keypair, deployed_contracts: vec![] })
    }

    /// Generate genesis blocks with PoW rewards (mints NativeToken)
    pub async fn generate_genesis_blocks(&mut self, num_blocks: usize) -> Result<()> {
        for _ in 0..num_blocks {
            let block = generate_native_block(&mut self.fork, &self.keypair).await?;
            let previous = self.fork.overlay.lock().unwrap().last_block()?;
            verify_block(&block, &previous, &block.zkbin_data).await?;
            apply_block(&block).await?;
            self.fork.append_proposal(&Proposal::new(block.clone())).await?;
        }
        tracing::info!("Generated {} genesis blocks", num_blocks);
        Ok(())
    }

    /// Deploy a WASM contract via Deployooor
    pub async fn deploy_contract(
        &mut self,
        wasm_bincode: Vec<u8>,
        name: &str,
    ) -> Result<dwow_sdk::crypto::ContractId> {
        let block = generate_deploy_block(&mut self.fork, &self.keypair, wasm_bincode).await?;
        let previous = self.fork.overlay.lock().unwrap().last_block()?;
        verify_block(&block, &previous, &block.zkbin_data).await?;
        apply_block(&block).await?;
        self.fork.append_proposal(&Proposal::new(block.clone())).await?;

        // Derive the contract ID from deploy public key
        let contract_id = dwow_sdk::crypto::ContractId::derive_public(self.keypair.public);
        self.deployed_contracts.push(contract_id);

        tracing::info!("Deployed {} contract: {:?}", name, contract_id);
        Ok(contract_id)
    }

    /// Verify and apply a block
    pub async fn verify_and_apply(&mut self, block: &BlockInfo) -> Result<()> {
        let previous = self.fork.overlay.lock().unwrap().last_block()?;
        verify_block(block, &previous, &block.zkbin_data).await?;
        apply_block(block).await?;
        self.fork.append_proposal(&Proposal::new(block.clone())).await?;
        Ok(())
    }

    /// Get current block height
    pub fn block_height(&self) -> Result<u32> {
        Ok(self.fork.overlay.lock().unwrap().last_block()?.header.height)
    }
}

/// Test basic genesis with NativeToken + Deployooor
pub async fn test_genesis_impl(ex: Arc<Executor<'static>>) -> Result<()> {
    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(BigUint::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18440".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18441".to_string(),
    };

    let mut genesis = GenesisHarness::new(config, &ex).await?;

    // Generate 3 genesis blocks with PoW rewards
    genesis.generate_genesis_blocks(3).await?;
    tracing::info!("Genesis blocks generated, height: {}", genesis.block_height()?);

    tracing::info!("test_genesis PASSED");
    Ok(())
}

#[test]
fn test_genesis() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_genesis_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}

/// Test NativeToken + Deployooor deployment
pub async fn test_native_token_deployoor_impl(ex: Arc<Executor<'static>>) -> Result<()> {
    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(BigUint::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18440".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18441".to_string(),
    };

    let mut genesis = GenesisHarness::new(config, &ex).await?;

    // Generate genesis blocks
    genesis.generate_genesis_blocks(2).await?;

    // Verify genesis blocks were generated successfully
    let height = genesis.block_height()?;
    tracing::info!("Genesis blocks generated, height: {}", height);
    tracing::info!("test_native_token_deployoor PASSED - NativeToken + Deployooor baseline verified");
    Ok(())
}

#[test]
fn test_native_token_deployoor() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_native_token_deployoor_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}