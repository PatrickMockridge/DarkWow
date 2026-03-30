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

//! Prediction Market Contract Entrypoint

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractResult,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{
    CancelMarketUpdateV1, ClaimWinningsUpdateV1, CreateMarketUpdateV1, CreatePositionUpdateV1,
    AddLiquidityUpdateV1, RemoveLiquidityUpdateV1, ResolveMarketUpdateV1, WithdrawFeesUpdateV1,
};
use crate::PredictionMarketFunction;

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Embed zkas circuits
    // Note: For MVP, we use simplified circuits. Production would include:
    // - position_v1.zk: Proves valid position creation
    // - resolve_market_v1.zk: Proves oracle resolution
    // - claim_winnings_v1.zk: Proves winning position ownership

    // Initialize database trees
    wasm::db::db_init(cid, crate::PREDICTION_CONTRACT_MARKETS_TREE)?;
    wasm::db::db_init(cid, crate::PREDICTION_CONTRACT_POSITIONS_TREE)?;
    wasm::db::db_init(cid, crate::PREDICTION_CONTRACT_LIQUIDITY_TREE)?;
    wasm::db::db_init(cid, crate::PREDICTION_CONTRACT_INFO_TREE)?;
    wasm::db::db_init(cid, crate::PREDICTION_CONTRACT_RESOLUTIONS_TREE)?;
    wasm::db::db_init(cid, crate::PREDICTION_CONTRACT_PENDING_TREE)?;
    wasm::db::db_init(cid, crate::PREDICTION_CONTRACT_CLAIMS_TREE)?;

    // Initialize default settings
    let info_db = wasm::db::db_lookup(cid, crate::PREDICTION_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(
        info_db,
        crate::PREDICTION_CONTRACT_PROTOCOL_FEE,
        &serialize(&crate::DEFAULT_PROTOCOL_FEE),
    )?;
    wasm::db::db_set(
        info_db,
        crate::PREDICTION_CONTRACT_LP_FEE,
        &serialize(&crate::DEFAULT_LP_FEE),
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
    let func = PredictionMarketFunction::try_from(self_.data[0])?;

    let update_data = match func {
        PredictionMarketFunction::InitializeV1 => {
            return Err(PredictionMarketError::InvalidMarketState.into())
        }
        PredictionMarketFunction::CreateMarketV1 => {
            prediction_market_create_market_process_instruction_v1(cid, call_idx, calls)?
        }
        PredictionMarketFunction::CreatePositionV1 => {
            prediction_market_create_position_process_instruction_v1(cid, call_idx, calls)?
        }
        PredictionMarketFunction::AddLiquidityV1 => {
            prediction_market_add_liquidity_process_instruction_v1(cid, call_idx, calls)?
        }
        PredictionMarketFunction::RemoveLiquidityV1 => {
            prediction_market_remove_liquidity_process_instruction_v1(cid, call_idx, calls)?
        }
        PredictionMarketFunction::ResolveMarketV1 => {
            prediction_market_resolve_market_process_instruction_v1(cid, call_idx, calls)?
        }
        PredictionMarketFunction::CancelMarketV1 => {
            prediction_market_cancel_market_process_instruction_v1(cid, call_idx, calls)?
        }
        PredictionMarketFunction::ClaimWinningsV1 => {
            prediction_market_claim_winnings_process_instruction_v1(cid, call_idx, calls)?
        }
        PredictionMarketFunction::WithdrawFeesV1 => {
            prediction_market_withdraw_fees_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match PredictionMarketFunction::try_from(update_data[0])? {
        PredictionMarketFunction::InitializeV1 => {
            Err(PredictionMarketError::InvalidMarketState.into())
        }
        PredictionMarketFunction::CreateMarketV1 => {
            let update: CreateMarketUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_create_market_process_update_v1(cid, update)
        }
        PredictionMarketFunction::CreatePositionV1 => {
            let update: CreatePositionUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_create_position_process_update_v1(cid, update)
        }
        PredictionMarketFunction::AddLiquidityV1 => {
            let update: AddLiquidityUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_add_liquidity_process_update_v1(cid, update)
        }
        PredictionMarketFunction::RemoveLiquidityV1 => {
            let update: RemoveLiquidityUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_remove_liquidity_process_update_v1(cid, update)
        }
        PredictionMarketFunction::ResolveMarketV1 => {
            let update: ResolveMarketUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_resolve_market_process_update_v1(cid, update)
        }
        PredictionMarketFunction::CancelMarketV1 => {
            let update: CancelMarketUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_cancel_market_process_update_v1(cid, update)
        }
        PredictionMarketFunction::ClaimWinningsV1 => {
            let update: ClaimWinningsUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_claim_winnings_process_update_v1(cid, update)
        }
        PredictionMarketFunction::WithdrawFeesV1 => {
            let update: WithdrawFeesUpdateV1 = deserialize(&update_data[1..])?;
            prediction_market_withdraw_fees_process_update_v1(cid, update)
        }
    }
}

// Modules for function implementations
mod create_market_v1;
mod create_position_v1;
mod add_liquidity_v1;
mod remove_liquidity_v1;
mod resolve_market_v1;
mod cancel_market_v1;
mod claim_winnings_v1;
mod withdraw_fees_v1;

use create_market_v1::*;
use create_position_v1::*;
use add_liquidity_v1::*;
use remove_liquidity_v1::*;
use resolve_market_v1::*;
use cancel_market_v1::*;
use claim_winnings_v1::*;
use withdraw_fees_v1::*;
