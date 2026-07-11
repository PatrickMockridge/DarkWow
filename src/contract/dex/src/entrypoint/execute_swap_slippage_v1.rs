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

//! ExecuteSwapSlippageV1 entrypoint functions
//!
//! Executes an atomic swap with slippage tolerance protection.
//! Slippage tolerance: received >= min_expected * (1 - slippage_bps / 10000)

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use dwow_promissory_note_contract::validation::validate_child_contract_id;
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DexError,
    model::{ExecuteSwapSlippageParams, ExecuteSwapUpdateV1, Swap, SwapState},
    DEX_CONTRACT_INFO_TREE, DEX_CONTRACT_PARTICIPANTS_TREE, DEX_CONTRACT_SWAPS_TREE,
    DEX_CONTRACT_ZKAS_EXECUTE_SWAP_SLIPPAGE_NS_V1, PROMISSORY_NOTE_CONTRACT_ID_KEY,
};

/// `get_metadata` function for `Dex::ExecuteSwapSlippageV1`
pub(crate) fn dex_execute_swap_slippage_get_metadata_v1(
    _cid: dwow_sdk::crypto::ContractId,
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
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: ExecuteSwapSlippageParams = deserialize(&self_.data.data[1..])?;

    msg!("[ExecuteSwapSlippageV1] Executing swap with slippage: id={:?}", &params.swap_id);

    // Validate children_indexes for promissory_note::otc_swap_v1 calls
    if self_.children_indexes.len() != 2 {
        msg!("[ExecuteSwapSlippageV1] Error: Expected 2 child calls (promissory_note::otc_swap_v1), got {}",
             self_.children_indexes.len());
        return Err(DexError::InvalidChildrenIndexes.into())
    }
    for &child_idx in self_.children_indexes.iter() {
        let child_call = &calls[child_idx].data;
        if child_call.data[0] != 0x05 {
            msg!("[ExecuteSwapSlippageV1] Error: Expected promissory_note::otc_swap_v1 (0x05), got 0x{:02x}",
                 child_call.data[0]);
            return Err(DexError::InvalidChildCall.into())
        }
    }

    // Validate child calls target promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DEX_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DexError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Promissory note contract ID must be configured at initialization
    if promissory_note_cid == ContractId::ZERO {
        msg!("[ExecuteSwapSlippageV1] Error: Promissory Note contract ID not configured");
        return Err(DexError::InvalidChildCall.into())
    }
    for &child_idx in self_.children_indexes.iter() {
        let child_call = &calls[child_idx].data;
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

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
    cid: dwow_sdk::crypto::ContractId,
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
