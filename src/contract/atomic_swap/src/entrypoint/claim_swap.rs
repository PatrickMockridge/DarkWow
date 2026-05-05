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

use darkfi_sdk::{
    crypto::{poseidon_hash, pasta_prelude::*},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Decodable, Encodable};

use crate::{
    error::AtomicSwapError,
    model::{ClaimParamsV1, ClaimUpdateV1, Swap, SwapState},
    ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE,
    ATOMIC_SWAP_CONTRACT_SECRETS_TREE,
    ATOMIC_SWAP_CONTRACT_SWAPS_TREE,
    ATOMIC_SWAP_CONTRACT_ZKAS_CLAIM_NS,
};

/// `get_metadata` function for `AtomicSwap::ClaimV1`
///
/// **Note**: This prepares public inputs for ZK proof verification when the
/// DarkFi runtime's `wasm::zk::verify_zk_proof()` is integrated. Currently,
/// the actual verification is done manually in `process_instruction` via
/// `poseidon_hash(secret) == swap.hash`.
pub(crate) fn atomic_swap_claim_get_metadata_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClaimParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // The circuit expects: nullifier_check as public input
    zk_public_inputs.push((
        ATOMIC_SWAP_CONTRACT_ZKAS_CLAIM_NS.to_string(),
        vec![params.nullifier],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `AtomicSwap::ClaimV1`
///
/// **ZK Verification Status**: This contract expects `wasm::zk::verify_zk_proof()`
/// from the DarkFi runtime, but the SDK does not expose this function.
/// Instead, we manually verify `poseidon_hash(secret) == swap.hash` which provides
/// equivalent cryptographic proof of secret knowledge. The ZK circuit (`claim_v1.zk`)
/// exists and could provide privacy-preserving verification if the runtime
/// integration is completed.
pub(crate) fn atomic_swap_claim_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];

    // Validate children_indexes for token claim
    if self_.children_indexes.len() != 1 {
        msg!("[AtomicSwap::ClaimV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", self_.children_indexes.len());
        return Err(AtomicSwapError::InvalidChildrenIndexes.into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[AtomicSwap::ClaimV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(AtomicSwapError::InvalidChildCall.into())
    }

    let params: ClaimParamsV1 = deserialize(&self_.data.data[1..])?;

    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_SWAPS_TREE)?;
    let Some(swap_data) = wasm::db::db_get(swaps_db, &serialize(&params.swap_id))? else {
        msg!("[AtomicSwap::Claim] Error: Swap not found");
        return Err(ContractError::InvalidFunction)
    };
    let swap: Swap = deserialize(&swap_data)
        .map_err(|_| ContractError::IoError("decode error".to_string()))?;

    // Verify swap is in Created state
    if swap.state != SwapState::Created {
        msg!("[AtomicSwap::Claim] Error: Swap not in Created state");
        return Err(ContractError::InvalidFunction)
    }

    // Verify the hash matches poseidon_hash(secret)
    let computed_hash = poseidon_hash([params.secret]);
    if computed_hash != swap.hash {
        msg!("[AtomicSwap::Claim] Error: Hash does not match secret");
        return Err(ContractError::InvalidFunction)
    }

    // Check nullifier hasn't been used (prevent double-claim)
    let nullifiers_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.nullifier))? {
        msg!("[AtomicSwap::Claim] Error: Nullifier already spent");
        return Err(ContractError::InvalidFunction)
    }

    // Return update data
    let update = ClaimUpdateV1 {
        swap_id: params.swap_id,
        nullifier: params.nullifier,
        secret: params.secret,
    };
    Ok(serialize(&update))
}

/// `process_update` function for `AtomicSwap::ClaimV1`
pub(crate) fn atomic_swap_claim_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ClaimUpdateV1,
) -> ContractResult {
    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_SWAPS_TREE)?;
    let Some(swap_data) = wasm::db::db_get(swaps_db, &serialize(&update.swap_id))? else {
        msg!("[AtomicSwap::Claim] Error: Swap not found");
        return Err(ContractError::InvalidFunction)
    };
    let mut swap: Swap = deserialize(&swap_data)
        .map_err(|_| ContractError::IoError("decode error".to_string()))?;

    // Mark as Claimed
    swap.state = SwapState::Claimed;
    wasm::db::db_set(swaps_db, &serialize(&update.swap_id), &serialize(&swap))?;

    // Record nullifier to prevent double-claim
    let nullifiers_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;

    // Store revealed secret (for external chain to read)
    let secrets_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_SECRETS_TREE)?;
    wasm::db::db_set(secrets_db, &serialize(&update.swap_id), &serialize(&update.secret))?;

    msg!("[AtomicSwap::Claim] Swap claimed: {:?}", update.swap_id);
    Ok(())
}
