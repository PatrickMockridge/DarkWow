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

//! ExecuteSwapSlippageV1 entrypoint functions
//!
//! Executes an atomic swap with slippage tolerance protection.
//! Slippage tolerance: received >= min_expected * (1 - slippage_bps / 10000)

use darkfi_sdk::{
    crypto::pasta_prelude::PrimeField,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DexError,
    model::{ExecuteSwapSlippageParams, ExecuteSwapUpdateV1, Swap, SwapState},
    DEX_CONTRACT_PARTICIPANTS_TREE, DEX_CONTRACT_SWAPS_TREE,
    DEX_CONTRACT_ZKAS_EXECUTE_SWAP_SLIPPAGE_NS_V1,
};

/// `get_metadata` function for `Dex::ExecuteSwapSlippageV1`
pub(crate) fn dex_execute_swap_slippage_get_metadata_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ExecuteSwapSlippageParams = deserialize(&self_.data[1..])?;

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    let alice_nullifier = params.alice_nullifier.inner();
    let bob_nullifier = params.bob_nullifier.inner();

    let swap_id = match pallas::Base::from_repr(params.swap_id).into_option() {
        Some(v) => v,
        None => return Err(ContractError::IoError("Invalid swap_id".to_string()).into()),
    };

    zk_public_inputs.push((
        DEX_CONTRACT_ZKAS_EXECUTE_SWAP_SLIPPAGE_NS_V1.to_string(),
        vec![alice_nullifier, bob_nullifier, swap_id],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dex::ExecuteSwapSlippageV1`
pub(crate) fn dex_execute_swap_slippage_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: ExecuteSwapSlippageParams = deserialize(&self_.data.data[1..])?;

    msg!("[ExecuteSwapSlippageV1] Executing swap with slippage: id={:?}", &params.swap_id);

    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id)?;
    let swap: Swap = match swap_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[ExecuteSwapSlippageV1] Error: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    match swap.state {
        SwapState::Accepted => {}
        _ => {
            msg!("[ExecuteSwapSlippageV1] Error: Swap not in Accepted state");
            return Err(DexError::InvalidSwapState.into())
        }
    }

    if params.alice_lock != swap.proposer_lock {
        msg!("[ExecuteSwapSlippageV1] Error: Alice's lock does not match stored proposer_lock");
        return Err(DexError::InvalidLockCommitment.into())
    }

    if params.bob_lock != swap.acceptor_lock {
        msg!("[ExecuteSwapSlippageV1] Error: Bob's lock does not match stored acceptor_lock");
        return Err(DexError::InvalidLockCommitment.into())
    }

    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    if !wasm::db::db_contains_key(participants_db, &swap.proposer_nullifier.to_bytes())? {
        msg!("[ExecuteSwapSlippageV1] Error: Proposer's nullifier not found");
        return Err(DexError::InvalidNullifier.into())
    }

    if !wasm::db::db_contains_key(participants_db, &swap.acceptor_nullifier.to_bytes())? {
        msg!("[ExecuteSwapSlippageV1] Error: Acceptor's nullifier not found");
        return Err(DexError::InvalidNullifier.into())
    }

    let update = ExecuteSwapUpdateV1 {
        swap_id: params.swap_id,
        alice_nullifier: params.alice_nullifier,
        bob_nullifier: params.bob_nullifier,
    };

    Ok(serialize(&update))
}

/// `process_update` function for `Dex::ExecuteSwapSlippageV1`
pub(crate) fn dex_execute_swap_slippage_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ExecuteSwapUpdateV1,
) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    let swap_data = wasm::db::db_get(swaps_db, &update.swap_id)?;
    let mut swap: Swap = match swap_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[ExecuteSwapSlippageV1] Error: Swap not found during update");
            return Err(DexError::SwapNotFound.into())
        }
    };

    swap.state = SwapState::Executed;

    wasm::db::db_set(swaps_db, &update.swap_id, &serialize(&swap))?;

    wasm::db::db_del(participants_db, &swap.proposer_nullifier.to_bytes())?;
    wasm::db::db_del(participants_db, &swap.acceptor_nullifier.to_bytes())?;

    msg!("[ExecuteSwapSlippageV1] Swap executed successfully with slippage: id={:?}", &update.swap_id);

    Ok(())
}
