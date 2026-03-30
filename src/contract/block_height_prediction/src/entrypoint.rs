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

//! Block Height Prediction Contract Entrypoint

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractResult,
    wasm,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::BlockHeightPredictionError;
use crate::model::{
    CancelMarketUpdateV1, ClaimWinningsUpdateV1, CreateMarketUpdateV1, CreatePositionUpdateV1,
    ResolveMarketUpdateV1,
};
use crate::BlockHeightPredictionFunction;

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize database trees
    wasm::db::db_init(cid, crate::BLOCK_HEIGHT_PREDICTION_MARKETS_TREE)?;
    wasm::db::db_init(cid, crate::BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE)?;
    wasm::db::db_init(cid, crate::BLOCK_HEIGHT_PREDICTION_INFO_TREE)?;
    wasm::db::db_init(cid, crate::BLOCK_HEIGHT_PREDICTION_CLAIMS_TREE)?;

    // Initialize default settings
    let info_db = wasm::db::db_lookup(cid, crate::BLOCK_HEIGHT_PREDICTION_INFO_TREE)?;
    wasm::db::db_set(
        info_db,
        crate::BLOCK_HEIGHT_PREDICTION_PROTOCOL_FEE,
        &serialize(&crate::DEFAULT_PROTOCOL_FEE),
    )?;

    Ok(())
}

/// Get metadata for verification
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BlockHeightPredictionFunction::try_from(self_.data[0])?;

    let update_data = match func {
        BlockHeightPredictionFunction::InitializeV1 => {
            return Err(BlockHeightPredictionError::InvalidStateTransition.into())
        }
        BlockHeightPredictionFunction::CreateMarketV1 => {
            create_market_v1::block_height_prediction_create_market_process_instruction(
                cid, call_idx, calls,
            )?
        }
        BlockHeightPredictionFunction::CreatePositionV1 => {
            create_position_v1::block_height_prediction_create_position_process_instruction(
                cid, call_idx, calls,
            )?
        }
        BlockHeightPredictionFunction::ResolveMarketV1 => {
            resolve_market_v1::block_height_prediction_resolve_market_process_instruction(
                cid, call_idx, calls,
            )?
        }
        BlockHeightPredictionFunction::ClaimWinningsV1 => {
            claim_winnings_v1::block_height_prediction_claim_winnings_process_instruction(
                cid, call_idx, calls,
            )?
        }
        BlockHeightPredictionFunction::CancelMarketV1 => {
            cancel_market_v1::block_height_prediction_cancel_market_process_instruction(
                cid, call_idx, calls,
            )?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match BlockHeightPredictionFunction::try_from(update_data[0])? {
        BlockHeightPredictionFunction::InitializeV1 => {
            Err(BlockHeightPredictionError::InvalidStateTransition.into())
        }
        BlockHeightPredictionFunction::CreateMarketV1 => {
            let update: CreateMarketUpdateV1 = deserialize(&update_data[1..])?;
            create_market_v1::block_height_prediction_create_market_process_update(cid, update)
        }
        BlockHeightPredictionFunction::CreatePositionV1 => {
            let update: CreatePositionUpdateV1 = deserialize(&update_data[1..])?;
            create_position_v1::block_height_prediction_create_position_process_update(cid, update)
        }
        BlockHeightPredictionFunction::ResolveMarketV1 => {
            let update: ResolveMarketUpdateV1 = deserialize(&update_data[1..])?;
            resolve_market_v1::block_height_prediction_resolve_market_process_update(cid, update)
        }
        BlockHeightPredictionFunction::ClaimWinningsV1 => {
            let update: ClaimWinningsUpdateV1 = deserialize(&update_data[1..])?;
            claim_winnings_v1::block_height_prediction_claim_winnings_process_update(cid, update)
        }
        BlockHeightPredictionFunction::CancelMarketV1 => {
            let update: CancelMarketUpdateV1 = deserialize(&update_data[1..])?;
            cancel_market_v1::block_height_prediction_cancel_market_process_update(cid, update)
        }
    }
}

// Modules for function implementations
mod cancel_market_v1;
mod claim_winnings_v1;
mod create_market_v1;
mod create_position_v1;
mod resolve_market_v1;

use cancel_market_v1::*;
use claim_winnings_v1::*;
use create_market_v1::*;
use create_position_v1::*;
use resolve_market_v1::*;
