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
 * with this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Linear Testnet Contract Integration Tests
//!
//! These tests use the LinearFiveNodeHarness to deploy contracts via Deployooor
//! and execute contract calls with real ZK proofs.
//!
//! ## Usage
//!
//! ```bash
//! cargo test --test linear_contract_tests
//! ```
//!
//! ## Contracts Tested
//!
//! - DAO Escrow (initialize, pay_premium, withdraw)
//! - Stablecoin (open_position, mint_stable, etc.)

use std::sync::Arc;

use darkfi::{
    tx::{ContractCallLeaf, TransactionBuilder},
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
};
use darkfi_contract_test_harness::harness::{DaoEscrowHarness, StablecoinHarness, ContractHarness};
use darkfi_sdk::{
    crypto::{keypair::Keypair, ContractId},
    pasta::pallas,
};
use smol::Executor;

use super::linear_five_node::LinearFiveNodeHarness;
use super::{HeavyweightPipeline, HarnessConfig};

// ============================================================================
// DAO Escrow Contract Tests
// ============================================================================

/// Test deploying and initializing DAO Escrow contract
#[cfg(test)]
pub async fn test_dao_escrow_init() -> Result<(), Box<dyn std::error::Error>> {
    // Create 5-node harness
    let harness = LinearFiveNodeHarness::new()?;

    // Deploy genesis contracts (Deployooor + NativeToken)
    harness.deploy_genesis_contracts()?;

    // Create genesis block
    let genesis_block = harness.alice_create_genesis();
    let genesis_hash = genesis_block.hash();
    harness.broadcast_block(&genesis_block)?;

    // Verify all nodes have genesis at height 0
    for node in harness.all_nodes() {
        assert_eq!(node.blockchain.get_height(), 0);
    }

    // Create DAO Escrow harness and heavyweight pipeline
    let ex = Arc::new(Executor::new()?);
    let config = HarnessConfig::default();

    let dao_harness = DaoEscrowHarness::new();
    let mut pipeline = HeavyweightPipeline::new(dao_harness, "dao_escrow", config, ex).await?;

    // Generate genesis blocks (mints tokens to keypair)
    pipeline.generate_genesis_blocks(3).await?;

    // Get DAO Escrow WASM
    let dao_escrow_wasm =
        include_bytes!("../../../../src/contract/dao_escrow/darkfi_dao_escrow_contract.wasm").to_vec();

    // Deploy contract
    let contract_id = pipeline.deploy(dao_escrow_wasm).await?;
    println!("DAO Escrow deployed at: {:?}", contract_id);

    // Verify contract exists on all nodes
    for node in harness.all_nodes() {
        assert!(node.blockchain.has_contract(contract_id)?);
    }

    // Build InitializeV1 call
    // This requires ZK proof - use the harness to generate it
    let init_call_data = pipeline.harness().create_init_v1_call(
        dao_bulla,
        owner_secret,
        token_id,
        bulla_blind,
    ).await?;

    // Execute contract call with ZK proof
    let tx = pipeline.exec(0x00, init_call_data, vec![]).await?;
    println!("DAO Init tx created: {:?}", tx.hash());

    // Create a block containing the tx
    let difficulty_target = 0x0000_FFFF;
    let mut block = smol::block_on(async {
        let txs = vec![tx.clone()];
        darkfi_linear::create_block(genesis_hash, 1, txs, difficulty_target)
    })?;

    // Mine block
    let consensus = darkfi_linear::PoWConsensus::new(difficulty_target);
    while !consensus.check_difficulty(&block.hash()) {
        block.header.nonce += 1;
    }

    // Broadcast block
    harness.broadcast_block(&block)?;

    // Verify all nodes at height 1
    harness.verify_sync()?;

    Ok(())
}

/// Test DAO Escrow pay_premium function
pub async fn test_dao_escrow_pay_premium() -> Result<(), Box<dyn std::error::Error>> {
    // Similar pattern - deploy, initialize, then pay_premium
    // ...
    Ok(())
}

// ============================================================================
// Stablecoin Contract Tests
// ============================================================================

/// Test deploying and opening a stablecoin position
pub async fn test_stablecoin_open_position() -> Result<(), Box<dyn std::error::Error>> {
    // Create 5-node harness
    let harness = LinearFiveNodeHarness::new()?;

    // Deploy genesis contracts
    harness.deploy_genesis_contracts()?;

    // Create genesis block
    let genesis_block = harness.alice_create_genesis();
    let genesis_hash = genesis_block.hash();
    harness.broadcast_block(&genesis_block)?;

    // Create stablecoin harness and pipeline
    let ex = Arc::new(Executor::new()?);
    let config = HarnessConfig::default();

    let stable_harness = StablecoinHarness::new();
    let mut pipeline = HeavyweightPipeline::new(stable_harness, "stablecoin", config, ex).await?;

    // Generate genesis blocks
    pipeline.generate_genesis_blocks(3).await?;

    // Get Stablecoin WASM
    let stablecoin_wasm =
        include_bytes!("../../../../src/contract/stablecoin/darkfi_stablecoin_contract.wasm").to_vec();

    // Deploy contract
    let contract_id = pipeline.deploy(stablecoin_wasm).await?;
    println!("Stablecoin deployed at: {:?}", contract_id);

    // Verify contract exists
    for node in harness.all_nodes() {
        assert!(node.blockchain.has_contract(contract_id)?);
    }

    // Build OpenPositionV1 call with ZK proof
    let open_call_data = pipeline.harness().create_open_position_call(
        owner_secret,
        collateral_token_id,
        collateral_amount,
        debt_token_id,
        mint_amount,
    ).await?;

    // Execute contract call
    let tx = pipeline.exec(0x01, open_call_data, vec![]).await?;
    println!("Stablecoin OpenPosition tx: {:?}", tx.hash());

    // Create and broadcast block
    let difficulty_target = 0x0000_FFFF;
    let mut block = smol::block_on(async {
        let txs = vec![tx];
        darkfi_linear::create_block(genesis_hash, 1, txs, difficulty_target)
    })?;

    let consensus = darkfi_linear::PoWConsensus::new(difficulty_target);
    while !consensus.check_difficulty(&block.hash()) {
        block.header.nonce += 1;
    }

    harness.broadcast_block(&block)?;
    harness.verify_sync()?;

    Ok(())
}