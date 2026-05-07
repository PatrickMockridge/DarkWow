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

use dwow_sdk::{
    crypto::pasta_prelude::*,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Decodable, Encodable};

use crate::{
    error::AtomicSwapError,
    model::{RefundParamsV1, RefundUpdateV1, Swap, SwapState},
    ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE,
    ATOMIC_SWAP_CONTRACT_SWAPS_TREE,
    ATOMIC_SWAP_CONTRACT_ZKAS_REFUND_NS,
};

/// `get_metadata` function for `AtomicSwap::RefundV1`
pub(crate) fn atomic_swap_refund_get_metadata_v1(
    _cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: RefundParamsV1 = deserialize(&self_.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // The circuit expects: nullifier_check as public input
    zk_public_inputs.push((
        ATOMIC_SWAP_CONTRACT_ZKAS_REFUND_NS.to_string(),
        vec![params.nullifier],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `AtomicSwap::RefundV1`
pub(crate) fn atomic_swap_refund_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];

    // Validate children_indexes for token refund
    if self_.children_indexes.len() != 1 {
        msg!("[AtomicSwap::RefundV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", self_.children_indexes.len());
        return Err(AtomicSwapError::InvalidChildrenIndexes.into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[AtomicSwap::RefundV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(AtomicSwapError::InvalidChildCall.into())
    }

    let params: RefundParamsV1 = deserialize(&self_.data.data[1..])?;

    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_SWAPS_TREE)?;
    let Some(swap_data) = wasm::db::db_get(swaps_db, &serialize(&params.swap_id))? else {
        msg!("[AtomicSwap::Refund] Error: Swap not found");
        return Err(ContractError::InvalidFunction)
    };
    let swap: Swap = deserialize(&swap_data)
        .map_err(|_| ContractError::IoError("decode error".to_string()))?;

    // Verify swap is in Created state
    if swap.state != SwapState::Created {
        msg!("[AtomicSwap::Refund] Error: Swap not in Created state");
        return Err(ContractError::InvalidFunction)
    }

    // Verify timelock has passed
    if params.current_block < swap.timelock {
        msg!("[AtomicSwap::Refund] Error: Timelock not expired");
        return Err(ContractError::InvalidFunction)
    }

    // Check nullifier hasn't been used (prevent double-refund)
    let nullifiers_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.nullifier))? {
        msg!("[AtomicSwap::Refund] Error: Nullifier already spent");
        return Err(ContractError::InvalidFunction)
    }

    // Return update data
    let update = RefundUpdateV1 {
        swap_id: params.swap_id,
        nullifier: params.nullifier,
    };
    Ok(serialize(&update))
}

/// `process_update` function for `AtomicSwap::RefundV1`
pub(crate) fn atomic_swap_refund_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: RefundUpdateV1,
) -> ContractResult {
    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_SWAPS_TREE)?;
    let Some(swap_data) = wasm::db::db_get(swaps_db, &serialize(&update.swap_id))? else {
        msg!("[AtomicSwap::Refund] Error: Swap not found");
        return Err(ContractError::InvalidFunction)
    };
    let mut swap: Swap = deserialize(&swap_data)
        .map_err(|_| ContractError::IoError("decode error".to_string()))?;

    // Mark as Refunded
    swap.state = SwapState::Refunded;
    wasm::db::db_set(swaps_db, &serialize(&update.swap_id), &serialize(&swap))?;

    // Record nullifier to prevent double-refund
    let nullifiers_db = wasm::db::db_lookup(cid, ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;

    msg!("[AtomicSwap::Refund] Swap refunded: {:?}", update.swap_id);
    Ok(())
}
