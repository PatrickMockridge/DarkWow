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
    crypto::pasta_prelude::*,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::AtomicSwapError,
    model::{CreateSwapParamsV1, CreateSwapUpdateV1, Swap, SwapState},
    ATOMIC_SWAP_CONTRACT_SWAPS_TREE,
    ATOMIC_SWAP_CONTRACT_ZKAS_CREATE_NS,
};

/// `get_metadata` function for `AtomicSwap::CreateSwapV1`
pub(crate) fn atomic_swap_create_get_metadata_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CreateSwapParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // Derive the swap_id from params (same as circuit)
    let swap_id = Swap::derive_id(
        params.hash,
        params.timelock,
        &params.darkfi_receiver,
        params.amount,
        params.token_id,
        params.side,
        params.blind,
    );

    // The circuit expects: derived_swap_id as public input (constrain_instance)
    zk_public_inputs.push((
        ATOMIC_SWAP_CONTRACT_ZKAS_CREATE_NS.to_string(),
        vec![swap_id],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `AtomicSwap::CreateSwapV1`
pub(crate) fn atomic_swap_create_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];

    // Validate children_indexes for token lock
    if self_.children_indexes.len() != 1 {
        msg!("[AtomicSwap::CreateSwapV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", self_.children_indexes.len());
        return Err(AtomicSwapError::InvalidChildrenIndexes.into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[AtomicSwap::CreateSwapV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(AtomicSwapError::InvalidChildCall.into())
    }

    let params: CreateSwapParamsV1 = deserialize(&self_.data.data[1..])?;

    // Verify swap doesn't already exist
    let swaps_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_SWAPS_TREE)?;
    if wasm::db::db_contains_key(swaps_db, &serialize(&params.commitment))? {
        msg!("[AtomicSwap::Create] Error: Swap already exists");
        return Err(ContractError::InvalidFunction)
    }

    // Verify the hash is not zero
    if params.hash == pallas::Base::ZERO {
        msg!("[AtomicSwap::Create] Error: Hash cannot be zero");
        return Err(ContractError::InvalidFunction)
    }

    // Return update data with all info needed to store the swap
    let update = CreateSwapUpdateV1 {
        swap_id: params.commitment,
        hash: params.hash,
        timelock: params.timelock,
        side: params.side,
        external_chain: params.external_chain,
        external_receiver: params.external_receiver,
        darkfi_receiver: params.darkfi_receiver,
        amount: params.amount,
        token_id: params.token_id,
        blind: params.blind,
        created_at: wasm::util::get_verifying_block_height()?.into(),
    };
    Ok(serialize(&update))
}

/// `process_update` function for `AtomicSwap::CreateSwapV1`
pub(crate) fn atomic_swap_create_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: CreateSwapUpdateV1,
) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_SWAPS_TREE)?;

    // Create the swap object
    let swap = Swap {
        id: update.swap_id,
        hash: update.hash,
        timelock: update.timelock,
        state: SwapState::Created,
        side: update.side,
        external_chain: update.external_chain,
        external_receiver: update.external_receiver,
        darkfi_receiver: update.darkfi_receiver,
        amount: update.amount,
        token_id: update.token_id,
        blind: update.blind,
        created_at: update.created_at,
    };

    // Store the swap
    wasm::db::db_set(swaps_db, &serialize(&update.swap_id), &serialize(&swap))?;

    msg!("[AtomicSwap::Create] Swap created: {:?}", update.swap_id);
    Ok(())
}
