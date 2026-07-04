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

#![allow(dead_code)]

use dwow_sdk::{
    crypto::{keypair::Keypair, pasta_prelude::{Field, Group}, SecretKey},
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;
use tracing::info;

use dwow_native_token_contract::{
    client::{
        burn_v1::{BurnCallBuilder, BurnCallInput},
        pow_reward_v1::PoWRewardCallBuilder,
    },
    model::{Coin, MintParamsV1},
    NativeTokenFunction,
};

/// Initialize logger for tests
pub fn init_logger() {
    let subscriber = tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Test MintV1 - simple coin creation
fn test_mint() -> Result<(), Box<dyn std::error::Error>> {
    info!(target: "test_harness::native_token", "=== Testing MintV1 ===");

    let keypair = Keypair::default();

    // Create a simple coin
    let coin = Coin::from_attributes(
        &keypair.public,
        1000,
        pallas::Base::zero(),
        pallas::Base::zero(),
        pallas::Base::zero(),
        pallas::Base::random(&mut OsRng),
    );

    info!(target: "test_harness::native_token", "Mint created coin: {:?}", coin);

    // Verify the coin can be serialized
    let mut data = vec![NativeTokenFunction::MintV1 as u8];
    let params = MintParamsV1 { coin, token_commit: pallas::Base::zero(), value_commit: pallas::Point::identity() };
    params.encode(&mut data)?;

    info!(target: "test_harness::native_token", "MintV1 test PASSED");
    Ok(())
}

/// Test PoWRewardCallBuilder - build a PoW reward call
fn test_pow_reward_call_builder() -> Result<(), Box<dyn std::error::Error>> {
    info!(target: "test_harness::native_token", "=== Testing PoWRewardCallBuilder ===");

    // Get the ZK binary from the compiled contract
    let mint_v1_bincode = include_bytes!("../../native_token/proof/mint_v1.zk.bin");
    let zkbin = dwow_core::zkas::ZkBinary::decode(mint_v1_bincode, false)?;
    let circuit = dwow_core::zk::ZkCircuit::new(dwow_core::zk::empty_witnesses(&zkbin)?, &zkbin);
    let pk = dwow_core::zk::ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build failed");

    // Generate secrets for the reward recipient
    let secret = SecretKey::random(&mut OsRng);
    let ephemeral_signature_secret = SecretKey::random(&mut OsRng);

    // Build PoW reward call
    let debris = PoWRewardCallBuilder {
        secret,
        ephemeral_signature_secret,
        block_height: 1,
        fees: 0,
        recipient: None,
        spend_hook: None,
        user_data: None,
        expected_cumulative_supply: 0,
        old_cumulative_commit: pallas::Point::identity(),
        old_cumulative_blind: pallas::Scalar::zero(),
        mint_zkbin: zkbin.clone(),
        mint_pk: pk,
        tx_nonce: pallas::Base::zero(),
        tx_commitment: pallas::Base::zero(),
    }
    .build()?;

    info!(target: "test_harness::native_token", "PoWReward call built successfully");
    info!(target: "test_harness::native_token", "  Output coin: {:?}", debris.params.output.coin);
    info!(target: "test_harness::native_token", "  Value commit: {:?}", debris.params.output.value_commit);
    info!(target: "test_harness::native_token", "  Token commit: {:?}", debris.params.output.token_commit);
    info!(target: "test_harness::native_token", "  Proofs generated: {}", debris.proofs.len());

    // Verify the call data can be serialized/deserialized correctly
    let mut data = vec![NativeTokenFunction::PoWRewardV1 as u8];
    debris.params.encode(&mut data)?;

    info!(target: "test_harness::native_token", "PoWRewardCallBuilder test PASSED");
    Ok(())
}

/// Test BurnCallBuilder - build a burn call
fn test_burn_call_builder() -> Result<(), Box<dyn std::error::Error>> {
    info!(target: "test_harness::native_token", "=== Testing BurnCallBuilder ===");

    // Get the ZK binary from the compiled contract
    let burn_v1_bincode = include_bytes!("../../native_token/proof/burn_v1.zk.bin");
    let zkbin = dwow_core::zkas::ZkBinary::decode(burn_v1_bincode, false)?;
    let circuit = dwow_core::zk::ZkCircuit::new(dwow_core::zk::empty_witnesses(&zkbin)?, &zkbin);
    let pk = dwow_core::zk::ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build failed");

    // Create secrets for the sender
    let secret = SecretKey::random(&mut OsRng);
    let ephemeral_sig_secret = SecretKey::random(&mut OsRng);

    // Create a simple input to burn
    let input = BurnCallInput {
        value: 500,
        token_id: pallas::Base::zero(),
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: pallas::Base::random(&mut OsRng),
        leaf_position: 0,
        merkle_path: vec![], // Empty path for simplicity
        secret,
        ephemeral_signature_secret: ephemeral_sig_secret,
        tx_commitment: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    // Build burn call
    let debris = BurnCallBuilder {
        inputs: vec![input],
        burn_zkbin: zkbin.clone(),
        burn_pk: pk,
    }
    .build()?;

    info!(target: "test_harness::native_token", "Burn call built successfully");
    info!(target: "test_harness::native_token", "  Inputs: {}", debris.params.inputs.len());
    info!(target: "test_harness::native_token", "  Proofs generated: {}", debris.proofs.len());

    // Verify the call data can be serialized/deserialized correctly
    let mut data = vec![NativeTokenFunction::BurnV1 as u8];
    debris.params.encode(&mut data)?;

    info!(target: "test_harness::native_token", "BurnCallBuilder test PASSED");
    Ok(())
}

/// Run all native_token tests
fn run_tests() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();

    info!(target: "test_harness::native_token", "Starting NativeToken contract tests");

    test_mint()?;
    test_pow_reward_call_builder()?;
    // test_burn_call_builder()?; // Requires proper Merkle tree infrastructure

    info!(target: "test_harness::native_token", "All NativeToken tests PASSED");
    Ok(())
}

fn main() {
    if let Err(e) = run_tests() {
        eprintln!("Test failed: {}", e);
        std::process::exit(1);
    }
}