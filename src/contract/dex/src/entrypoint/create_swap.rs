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

//! CreateSwapV1 entrypoint functions
//!
//! ## Security Model
//!
//! The prover MUST compute the nullifier externally before calling this function:
//! - nullifier = poseidon_hash([secret, lock_commitment])
//!
//! This nullifier is passed in CreateSwapParams and is used for:
//! 1. ZK proof verification (public input)
//! 2. Double-spend prevention (stored in participants_db)
//!
//! ## Trusted Setup for lock_proof
//!
//! The lock_proof is verified against a trusted Merkle root that was set during
//! contract initialization. This is a TEMPORARY WORKAROUND due to lack of proper
//! cross-contract ZK composition opcodes.
//!
//! See module-level documentation in lib.rs for full details.

use dwow_sdk::{
    crypto::{poseidon_hash, pasta_prelude::PrimeField, IntentCommitment, IntentNullifier},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use dwow_serial::{Decodable, Encodable};

use crate::{
    error::DexError,
    model::{CreateSwapParams, CreateSwapUpdateV1, Swap, SwapState},
    DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_INFO_TREE, DEX_CONTRACT_PARTICIPANTS_TREE,
    DEX_CONTRACT_SWAPS_TREE, DEX_CONTRACT_TIMEOUT, DEX_CONTRACT_TRUSTED_MONEY_MERKLE_ROOT_KEY,
    DEX_CONTRACT_ZKAS_CREATE_SWAP_NS_V2,
};

/// `get_metadata` function for `Dex::CreateSwapV1`
///
/// Returns public inputs for ZK proof verification:
/// - lock_commitment: from params
/// - swap_id: from params
/// - nullifier: from params (prover-computed)
/// - signature_public_x: X coordinate of proposer's signature public key
/// - signature_public_y: Y coordinate of proposer's signature public key
///
/// The host uses these to verify the ZK proof.
pub(crate) fn dex_create_swap_get_metadata_v1(
    _cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= CreateSwapParams::decode(&self_.data[1..])?;

    // Public inputs for the ZK proof verification
    // The order must match the `constrain_instance` calls in create_swap_v1.zk:
    // 1. lock_commitment (computed_lock)
    // 2. swap_id (computed_swap_id)
    // 3. nullifier
    // 4. signature_public_x
    // 5. signature_public_y
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // The prover computed the nullifier externally and passed it in params.
    // We use these directly as public inputs for ZK verification.
    let lock_commitment = params.lock_commitment.inner();
    let swap_id = match pallas::Base::from_repr(params.swap_id).into_option() {
        Some(v) => v,
        None => return Err(ContractError::IoError("Invalid swap_id".to_string()).into()),
    };
    let nullifier = params.nullifier.inner();

    // Extract signature public key coordinates
    let (sig_x, sig_y) = params.signature_public.xy().expect("pk not identity");

    zk_public_inputs.push((
        DEX_CONTRACT_ZKAS_CREATE_SWAP_NS_V2.to_string(),
        vec![lock_commitment, swap_id, nullifier, sig_x, sig_y, pallas::Base::zero(), pallas::Base::zero()],
    ));

    // Serialize metadata for ZK verification
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dex::CreateSwapV1`
///
/// Verifies:
/// 1. Swap doesn't already exist
/// 2. lock_proof is valid against trusted Merkle root (TRUSTED SETUP)
/// 3. Returns update with nullifier for storage
pub(crate) fn dex_create_swap_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params= CreateSwapParams::decode(&self_.data.data[1..])?;

    msg!("[CreateSwapV1] Creating swap: id={:?}", &params.swap_id);

    // Verify swap doesn't already exist
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    if wasm::db::db_contains_key(swaps_db, &params.swap_id)? {
        msg!("[CreateSwapV1] Error: Swap already exists");
        return Err(DexError::SwapAlreadyExists.into())
    }

    // FALLBACK — Host-level signature verification (NOT co-equal with in-contract)
    //   Reason: Cross-contract ZK composition not yet implemented (TRUSTED SETUP workaround).
    //   See accept_swap_v1.rs for full DEGRADATION RISK and CONSTRAINT documentation.
    let config_db = wasm::db::db_lookup(cid, DEX_CONTRACT_CONFIG_TREE)?;
    verify_lock_proof(config_db, &params.lock_commitment.to_bytes(), &params.lock_proof)?;

    // Get current timestamp and timeout
    let info_db = wasm::db::db_lookup(cid, DEX_CONTRACT_INFO_TREE)?;
    let current_time = get_current_timestamp(info_db)?;
    let timeout = get_swap_timeout(config_db)?;

    // Extract proposer's public key from signature_public
    // The signature_public is provided by the client and will be verified
    // by the ZK circuit once signature derivation is added to the circuit.
    // For now, we trust that the host has verified the signature.
    let (proposer_pub_x, proposer_pub_y) = params.signature_public.xy().expect("pk not identity");

    // Create the update struct with nullifier from params
    let update = CreateSwapUpdateV1 {
        swap_id: params.swap_id,
        proposer_pub_x: proposer_pub_x.to_repr(),
        proposer_pub_y: proposer_pub_y.to_repr(),
        offer_token: params.offer_token,
        offer_amount: params.offer_amount,
        request_token: params.request_token,
        request_amount: params.request_amount,
        proposer_lock: params.lock_commitment,
        proposer_nullifier: params.nullifier,
        created_at: current_time,
        expires_at: current_time + timeout as u64,
        open_execution: params.open_execution,
    };

    Ok(update.encode())
}

/// `process_update` function for `Dex::CreateSwapV1`
pub(crate) fn dex_create_swap_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: CreateSwapUpdateV1,
) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Create swap record with nullifiers
    let swap = Swap {
        swap_id: update.swap_id,
        proposer_pub_x: update.proposer_pub_x,
        proposer_pub_y: update.proposer_pub_y,
        acceptor_pub_x: [0u8; 32],
        acceptor_pub_y: [0u8; 32],
        offer_token: update.offer_token,
        offer_amount: update.offer_amount,
        request_token: update.request_token,
        request_amount: update.request_amount,
        proposer_lock: update.proposer_lock,
        proposer_nullifier: update.proposer_nullifier,
        acceptor_lock: IntentCommitment::from_bytes([0u8; 32]).unwrap(),
        acceptor_nullifier: IntentNullifier::from_bytes([0u8; 32]).unwrap(),
        state: SwapState::Created,
        created_at: update.created_at,
        expires_at: update.expires_at,
        open_execution: update.open_execution,
    };

    // Store the swap
    wasm::db::db_set(swaps_db, &update.swap_id, &swap.encode())?;

    // Store proposer's nullifier to prevent double-spending
    // Using nullifier as key instead of lock_commitment
    wasm::db::db_mark_spent(participants_db, &update.proposer_nullifier.to_bytes())?;

    msg!("[CreateSwapV1] Swap created successfully: id={:?}", &update.swap_id);

    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Get current block timestamp from info_db
fn get_current_timestamp(info_db: u32) -> Result<u64, ContractError> {
    let data = wasm::db::db_get(info_db, b"current_timestamp")?;
    match data {
        Some(d) => {
            let mut cursor = std::io::Cursor::new(&d);
            u64::decode(&mut cursor).map_err(|_| ContractError::IoError("decode error".to_string()))
        }
        None => Ok(0),
    }
}

/// Get swap timeout from config_db
fn get_swap_timeout(config_db: u32) -> Result<u32, ContractError> {
    let data = wasm::db::db_get(config_db, DEX_CONTRACT_TIMEOUT)?;
    match data {
        Some(d) => {
            let mut cursor = std::io::Cursor::new(&d);
            u32::decode(&mut cursor).map_err(|_| ContractError::IoError("decode error".to_string()))
        }
        None => Ok(100), // Default 100 blocks
    }
}

/// Verify lock_proof against trusted Merkle root
///
/// # Trusted Setup Warning
///
/// This function implements a TEMPORARY WORKAROUND for verifying lock_proofs
/// due to the absence of proper cross-contract ZK composition opcodes.
///
/// The verification is only as secure as the trusted Merkle root provided
/// during contract initialization. If the trusted root is incorrect or
/// outdated, invalid lock_proofs may be accepted.
///
/// # Proper Solution Requires
///
/// - Cross-contract ZK proof composition opcodes
/// - On-chain Merkle root verification
/// - Event-based state synchronization
fn verify_lock_proof(
    config_db: u32,
    lock_commitment: &[u8; 32],
    lock_proof: &[[u8; 32]],
) -> Result<(), ContractError> {
    // Get trusted Merkle root from config
    let trusted_root_data = wasm::db::db_get(config_db, DEX_CONTRACT_TRUSTED_MONEY_MERKLE_ROOT_KEY)
        .map_err(|_| ContractError::IoError("Db error".to_string()))?;

    let trusted_root = match trusted_root_data {
        Some(data) => {
            let mut cursor = std::io::Cursor::new(&data);
            <[u8; 32]>::decode(&mut cursor)
                .map_err(|_| ContractError::IoError("Decode error".to_string()))?
        }
        None => {
            msg!("[CreateSwapV1] ERROR: Trusted Merkle root not set during initialization");
            msg!("[CreateSwapV1] ERROR: This DEX was not properly initialized with a trusted root");
            msg!("[CreateSwapV1] ERROR: lock_proof cannot be verified - rejecting swap");
            return Err(DexError::InvalidMerkleProof.into())
        }
    };

    // Basic validation: lock_proof should not be empty
    // A valid Merkle proof has at least one sibling
    if lock_proof.is_empty() {
        msg!("[CreateSwapV1] ERROR: lock_proof is empty");
        return Err(DexError::InvalidMerkleProof.into())
    }

    // Convert lock_commitment to pallas::Base
    let leaf = match pallas::Base::from_repr(*lock_commitment).into_option() {
        Some(v) => v,
        None => return Err(ContractError::IoError("Invalid lock commitment".to_string()).into()),
    };

    // Compute Merkle root by hashing upward
    // The lock_proof contains siblings at each level
    let mut current = leaf;
    for sibling_bytes in lock_proof.iter() {
        let sibling = match pallas::Base::from_repr(*sibling_bytes).into_option() {
            Some(v) => v,
            None => return Err(ContractError::IoError("Invalid sibling".to_string()).into()),
        };
        // Hash pair - order matters for some trees, we assume fixed ordering
        current = poseidon_hash([current, sibling]);
    }

    // Compare computed root with trusted root
    let computed_root: [u8; 32] = current.to_repr();

    if computed_root != trusted_root {
        msg!("[CreateSwapV1] ERROR: lock_proof verification failed");
        msg!("[CreateSwapV1] ERROR: Computed root does not match trusted root");
        msg!("[CreateSwapV1] ERROR: This may indicate an invalid lock_proof or stale trusted root");
        return Err(DexError::InvalidMerkleProof.into())
    }

    msg!("[CreateSwapV1] lock_proof verified against trusted Merkle root");
    Ok(())
}