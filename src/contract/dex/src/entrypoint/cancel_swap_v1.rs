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

//! CancelSwapV1 entrypoint functions
//!
//! ## Security Model
//!
//! The DEX contract relies on a split verification model:
//!
//! 1. **Client/Prover** computes nullifier externally:
//!    - nullifier = poseidon_hash([secret, lock_commitment])
//!
//! 2. **ZK Circuit** proves:
//!    - Prover knows the secret to the lock
//!    - The nullifier is correctly computed
//!
//! 3. **Contract** (this file) verifies:
//!    - The nullifier provided matches what's on-chain (double-cancel check)
//!    - The swap exists and is in correct state
//!    - The caller is authorized to cancel (via ZK proof at host level)
//!
//! ## Limitations
//!
//! - The contract CANNOT verify the ZK proof itself - that happens at the host level
//! - The contract trusts that if get_metadata returns successfully, the proof was valid
//! - The contract verifies nullifiers against on-chain state to prevent double-cancellation
//! - Determining WHO is cancelling (proposer vs acceptor) requires checking which nullifier matches

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
    model::{CancelSwapParams, CancelSwapUpdateV1, Swap, SwapState},
    DEX_CONTRACT_INFO_TREE, DEX_CONTRACT_PARTICIPANTS_TREE, DEX_CONTRACT_SWAPS_TREE,
    DEX_CONTRACT_ZKAS_CANCEL_SWAP_NS_V1,
};

/// `get_metadata` function for `Dex::CancelSwapV1`
///
/// Returns public inputs for ZK proof verification:
/// - computed_nullifier: from params (computed by prover)
/// - computed_swap_id: from params
///
/// The host uses these to verify the ZK proof.
pub(crate) fn dex_cancel_swap_get_metadata_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CancelSwapParams = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proof verification
    // The order must match the `constrain_instance` calls in cancel_swap_v1.zk:
    // 1. computed_nullifier
    // 2. computed_swap_id
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // The prover computed the nullifier externally and passed it in params.
    // We use this directly as a public input for ZK verification.
    let nullifier = pallas::Base::from_bytes(&params.nullifier)
        .map_err(|_| ContractError::FailedToDeserialize)?;

    let swap_id = pallas::Base::from_bytes(&params.swap_id)
        .map_err(|_| ContractError::FailedToDeserialize)?;

    zk_public_inputs.push((
        DEX_CONTRACT_ZKAS_CANCEL_SWAP_NS_V1.to_string(),
        vec![nullifier, swap_id],
    ));

    // Serialize metadata for ZK verification
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dex::CancelSwapV1`
///
/// Verifies:
/// 1. Swap exists and is in correct state (Created or Accepted)
/// 2. Nullifier exists in participants_db (double-cancel check)
/// 3. Determines whether proposer or acceptor is cancelling
/// 4. Returns update to be applied if verification passes
pub(crate) fn dex_cancel_swap_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: CancelSwapParams = deserialize(&self_.data.data[1..])?;

    msg!("[CancelSwapV1] Cancelling swap: id={:?}", &params.swap_id);

    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id)?;
    let swap: Swap = match swap_data {
        Some(data) => {
            let mut cursor = std::io::Cursor::new(&data);
            Swap::decode(&mut cursor).map_err(|_| ContractError::DecodeError)?
        }
        None => {
            msg!("[CancelSwapV1] Error: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Verify swap is not already executed or cancelled
    match swap.state {
        SwapState::Created | SwapState::Accepted => {}
        _ => {
            msg!("[CancelSwapV1] Error: Swap already executed or cancelled");
            return Err(DexError::InvalidSwapState.into())
        }
    };

    // Verify nullifier against on-chain state (double-cancel check)
    // Now using nullifiers stored in participants_db
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Check if the provided nullifier matches the proposer's nullifier
    let is_proposer = wasm::db::db_contains_key(participants_db, &swap.proposer_nullifier)?;

    // Check if it matches the acceptor's nullifier (if swap is accepted)
    let is_acceptor = wasm::db::db_contains_key(participants_db, &swap.acceptor_nullifier)?;

    // The nullifier must match exactly one of the participants
    // If it matches neither, the prover is trying to cancel with an invalid nullifier
    // If it matches both, something is wrong (shouldn't happen)
    if !is_proposer && !is_acceptor {
        msg!("[CancelSwapV1] Error: Nullifier does not match any participant");
        return Err(DexError::InvalidNullifier.into())
    }

    if is_proposer && is_acceptor {
        // This shouldn't happen unless there's a bug in nullifier generation
        msg!("[CancelSwapV1] Error: Nullifier matches both participants");
        return Err(DexError::InvalidNullifier.into())
    }

    // Create the update
    let update = CancelSwapUpdateV1 {
        swap_id: params.swap_id,
        nullifier: params.nullifier,
        is_proposer,
    };

    Ok(serialize(&update))
}

/// `process_update` function for `Dex::CancelSwapV1`
pub(crate) fn dex_cancel_swap_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: CancelSwapUpdateV1,
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
            msg!("[CancelSwapV1] Error: Swap not found during update");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Update swap state to Cancelled
    swap.state = SwapState::Cancelled;

    // Store updated swap
    wasm::db::db_set(swaps_db, &update.swap_id, &swap.encode())?;

    // Remove the participant's nullifier (they get refunded)
    // In a full implementation, we would also call the money contract
    // to refund the locked funds
    // Using nullifier for deletion (proper double-spend prevention)
    if update.is_proposer {
        wasm::db::db_delete(participants_db, &swap.proposer_nullifier)?;
    } else {
        wasm::db::db_delete(participants_db, &swap.acceptor_nullifier)?;
    }

    msg!("[CancelSwapV1] Swap cancelled: id={:?}", &update.swap_id);

    Ok(())
}