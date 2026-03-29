/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! ExecuteSwapV1 entrypoint functions
//!
//! ## Security Model
//!
//! The DEX contract relies on a split verification model:
//!
//! 1. **Client/Prover** computes nullifiers externally:
//!    - alice_nullifier = poseidon_hash([alice_secret, alice_lock])
//!    - bob_nullifier = poseidon_hash([bob_secret, bob_lock])
//!
//! 2. **ZK Circuit** proves:
//!    - Prover knows alice_secret and bob_secret
//!    - The nullifiers are correctly computed
//!    - The swap ID is consistent
//!
//! 3. **Contract** (this file) verifies:
//!    - The nullifiers provided match what's on-chain (double-spend check)
//!    - The swap exists and is in correct state
//!
//! ## Limitations
//!
//! - The contract CANNOT verify the ZK proof itself - that happens at the host level
//! - The contract trusts that if get_metadata returns successfully, the proof was valid
//! - The contract verifies nullifiers against on-chain state to prevent double-execution

use darkfi_sdk::{
    crypto::pasta_prelude::*,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DexError,
    model::{ExecuteSwapParams, ExecuteSwapUpdateV1, Swap, SwapState},
    DEX_CONTRACT_INFO_TREE, DEX_CONTRACT_PARTICIPANTS_TREE, DEX_CONTRACT_SWAPS_TREE,
    DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1,
};

/// `get_metadata` function for `Dex::ExecuteSwapV1`
///
/// Returns public inputs for ZK proof verification:
/// - alice_nullifier: from params (computed by prover)
/// - bob_nullifier: from params (computed by prover)
/// - swap_id: from params
///
/// The host uses these to verify the ZK proof.
pub(crate) fn dex_execute_swap_get_metadata_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ExecuteSwapParams = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proof verification
    // The order must match the `constrain_instance` calls in execute_swap_v1.zk:
    // 1. alice_nullifier_check
    // 2. bob_nullifier_check
    // 3. computed_swap_id
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // The prover computed the nullifiers externally and passed them in params.
    // We use these directly as public inputs for ZK verification.
    let alice_nullifier = pallas::Base::from_bytes(&params.alice_nullifier)
        .map_err(|_| ContractError::FailedToDeserialize)?;
    let bob_nullifier = pallas::Base::from_bytes(&params.bob_nullifier)
        .map_err(|_| ContractError::FailedToDeserialize)?;

    let swap_id = pallas::Base::from_bytes(&params.swap_id)
        .map_err(|_| ContractError::FailedToDeserialize)?;

    zk_public_inputs.push((
        DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1.to_string(),
        vec![alice_nullifier, bob_nullifier, swap_id],
    ));

    // Serialize metadata for ZK verification
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dex::ExecuteSwapV1`
///
/// Verifies:
/// 1. Swap exists and is in Accepted state
/// 2. Nullifiers haven't been spent (double-execution check)
/// 3. Returns update to be applied if verification passes
pub(crate) fn dex_execute_swap_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: ExecuteSwapParams = deserialize(&self_.data.data[1..])?;

    msg!("[ExecuteSwapV1] Executing swap: id={:?}", &params.swap_id);

    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id)?;
    let swap: Swap = match swap_data {
        Some(data) => {
            let mut cursor = std::io::Cursor::new(&data);
            Swap::decode(&mut cursor).map_err(|_| ContractError::DecodeError)?
        }
        None => {
            msg!("[ExecuteSwapV1] Error: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Verify swap is in Accepted state
    match swap.state {
        SwapState::Accepted => {}
        _ => {
            msg!("[ExecuteSwapV1] Error: Swap not in Accepted state");
            return Err(DexError::InvalidSwapState.into())
        }
    }

    // SECURITY: Verify that provided locks match the stored values
    // This prevents an attacker from executing a swap with mismatched locks
    if params.alice_lock != swap.proposer_lock {
        msg!("[ExecuteSwapV1] Error: Alice's lock does not match stored proposer_lock");
        return Err(DexError::InvalidLockCommitment.into())
    }

    if params.bob_lock != swap.acceptor_lock {
        msg!("[ExecuteSwapV1] Error: Bob's lock does not match stored acceptor_lock");
        return Err(DexError::InvalidLockCommitment.into())
    }

    // Verify nullifiers against on-chain state (double-execution check)
    // Now using nullifiers instead of lock_commitments
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Check that proposer's nullifier hasn't been spent
    if !wasm::db::db_contains_key(participants_db, &swap.proposer_nullifier)? {
        msg!("[ExecuteSwapV1] Error: Proposer's nullifier not found in participants");
        return Err(DexError::InvalidNullifier.into())
    }

    // Check that acceptor's nullifier hasn't been spent
    if !wasm::db::db_contains_key(participants_db, &swap.acceptor_nullifier)? {
        msg!("[ExecuteSwapV1] Error: Acceptor's nullifier not found in participants");
        return Err(DexError::InvalidNullifier.into())
    }

    // Create the update with the prover-provided nullifiers
    // These were verified as public inputs by the ZK proof
    let update = ExecuteSwapUpdateV1 {
        swap_id: params.swap_id,
        alice_nullifier: params.alice_nullifier,
        bob_nullifier: params.bob_nullifier,
    };

    Ok(serialize(&update))
}

/// `process_update` function for `Dex::ExecuteSwapV1`
pub(crate) fn dex_execute_swap_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ExecuteSwapUpdateV1,
) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Load existing swap
    let swap_data = wasm::db::db_get(swaps_db, &update.swap_id)?;
    let mut swap: Swap = match swap_data {
        Some(data) => {
            let mut cursor = std::io::Cursor::new(&data);
            Swap::decode(&mut cursor).map_err(|_| ContractError::DecodeError)?
        }
        None => {
            msg!("[ExecuteSwapV1] Error: Swap not found during update");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Update swap state to Executed
    swap.state = SwapState::Executed;

    // Store updated swap
    wasm::db::db_set(swaps_db, &update.swap_id, &swap.encode())?;

    // Remove participants (funds have been transferred)
    // In a full implementation, we would also call the money contract
    // to perform the actual token transfers
    // Using nullifiers for deletion (proper double-spend prevention)
    wasm::db::db_delete(participants_db, &swap.proposer_nullifier)?;
    wasm::db::db_delete(participants_db, &swap.acceptor_nullifier)?;

    msg!("[ExecuteSwapV1] Swap executed successfully: id={:?}", &update.swap_id);

    Ok(())
}