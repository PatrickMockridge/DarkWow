/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Sync test with WASM - VK derived from bytes, NOT from sled

use std::sync::Arc;

use darkfi::{
    blockchain::{BlockInfo, Header},
    tx::{ContractCallLeaf, TransactionBuilder},
    validator::{
        consensus::{Fork, Proposal},
        sync::{apply_block, verify_block},
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    Result,
};
use darkfi_native_token_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, NativeTokenFunction,
};
use darkfi_sdk::{
    crypto::{
        keypair::Keypair,
        MerkleTree, NATIVE_TOKEN_CONTRACT_ID,
    },
    num_traits::One,
    ContractCall,
};
use darkfi_serial::Encodable;
use num_bigint::BigUint;
use smol::Executor;

use crate::tests::HarnessConfig;

/// ZK Binary loaded directly from include_bytes - no sled lookup needed
fn get_mint_zkbin() -> Result<darkfi::zkas::ZkBinary> {
    let zkbin_bytes =
        include_bytes!("../../../../src/contract/native_token/proof/mint_v1.zk.bin").to_vec();
    Ok(darkfi::zkas::ZkBinary::decode(&zkbin_bytes, false)?)
}

/// Generate a native token block
/// VK is derived from include_bytes, NOT from sled
async fn generate_native_block(
    fork: &mut Fork,
    keypair: &Keypair,
) -> Result<BlockInfo> {
    // Grab fork last block
    let previous = fork.overlay.lock().unwrap().last_block()?;
    let block_height = previous.header.height + 1;
    let last_nonce = previous.header.nonce;

    // Get zkbin directly from include_bytes - no sled lookup needed!
    let zkbin = get_mint_zkbin()?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit);

    // Build the transaction debris using zkbin we already have
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

    // Generate and sign the actual transaction
    let mut data = vec![NativeTokenFunction::PoWRewardV1 as u8];
    debris.params.encode(&mut data)?;
    let call = ContractCall { contract_id: *NATIVE_TOKEN_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&[keypair.secret])?;
    tx.signatures = vec![sigs];

    // Timestamp must be > previous
    let timestamp = previous.header.timestamp.checked_add(1.into())?;

    // Generate header
    let header = Header::new(previous.hash(), block_height, last_nonce, timestamp);

    // Generate the block
    let mut block = BlockInfo::new_empty(header);
    block.append_txs(vec![tx]);

    // Compute state root - but we skip apply_producer_transaction
    // since we're not executing WASM for state, just building the block
    let overlay = fork.overlay.lock().unwrap().full_clone()?;
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&fork.diffs)?;
    block.header.state_root = overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

    // Attach signature
    block.sign(&keypair.secret);

    Ok(block)
}

/// Test that uses harness to initialize node, generates a block with WASM zkbin,
/// and verifies using sync module without reading VK from sled
async fn test_sync_native_impl(ex: Arc<Executor<'static>>) -> Result<()> {
    // Initialize harness with native contracts deployed
    let config = HarnessConfig {
        pow_target: 20,
        pow_fixed_difficulty: Some(BigUint::one()),
        confirmation_threshold: 1,
        max_forks: 8,
        alice_url: "tcp+tls://127.0.0.1:18440".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18441".to_string(),
    };
    let th = crate::tests::Harness::new(config, true, &ex).await?;

    // Generate a fork from Alice's consensus
    let mut fork = th.alice.validator.read().await.consensus.forks[0].full_clone()?;

    // Use keypair for signing
    let keypair = Keypair::default();

    // Generate block using zkbin from include_bytes (not from sled)
    let block = generate_native_block(&mut fork, &keypair).await?;
    tracing::info!("Generated block: {:?}", block.hash());

    // Get zkbin bytes for verification
    let zkbin_bytes =
        include_bytes!("../../../../src/contract/native_token/proof/mint_v1.zk.bin").to_vec();

    // Get previous block
    let previous = fork.overlay.lock().unwrap().last_block()?;

    // Verify using sync module - VK derived from zkbin_bytes
    verify_block(&block, &previous, &zkbin_bytes).await?;

    // Apply using sync module
    apply_block(&block).await?;

    // Append to fork using consensus
    fork.append_proposal(&Proposal::new(block.clone())).await?;

    tracing::info!("test_sync_native PASSED");
    Ok(())
}

#[test]
fn test_sync_native() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_sync_native_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}