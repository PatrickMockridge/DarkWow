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

//! CreateMarketV1 Implementation

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractError,
    msg,
    wasm,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::BlockHeightPredictionError;
use crate::model::{
    derive_market_id, validate_confirmation_depth, Market, MarketState,
    CreateMarketParamsV1, CreateMarketUpdateV1,
};
use crate::{
    BLOCK_HEIGHT_PREDICTION_INFO_TREE, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE,
    BLOCK_HEIGHT_PREDICTION_PROTOCOL_FEE, DEFAULT_PROTOCOL_FEE,
};

/// Process instruction for CreateMarketV1
pub fn block_height_prediction_create_market_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CreateMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[block_height_prediction::create_market] Creating new market");
    msg!("  target_time: {}", params.target_time);
    msg!("  confirmation_depth: {}", params.confirmation_depth);

    // Validate confirmation depth
    validate_confirmation_depth(params.confirmation_depth)?;

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Look up protocol fee (use default if not set)
    let info_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_INFO_TREE)?;
    let protocol_fee = if params.protocol_fee == 0 {
        let stored_fee_bytes = wasm::db::db_get(info_db, BLOCK_HEIGHT_PREDICTION_PROTOCOL_FEE)?;
        if let Some(bytes) = stored_fee_bytes {
            deserialize::<u32>(&bytes)?
        } else {
            DEFAULT_PROTOCOL_FEE
        }
    } else {
        params.protocol_fee
    };

    // Get base block height from current state
    // This is used as reference for expected block calculations
    let base_block_height = current_block as u64;

    msg!("  current_block: {}", current_block);
    msg!("  base_block_height: {}", base_block_height);

    // Derive market ID
    let market_id = derive_market_id(
        &params.creator,
        params.target_time,
        params.token_id,
        params.confirmation_depth,
    );

    msg!("[block_height_prediction::create_market] Derived market_id");

    // Check if market already exists
    let markets_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE)?;
    if wasm::db::db_contains_key(markets_db, &serialize(&market_id))? {
        return Err(BlockHeightPredictionError::MarketAlreadyExists.into())
    }

    // Create the update
    let update = CreateMarketUpdateV1 {
        market_id,
        creator: params.creator,
        target_time: params.target_time,
        base_block_height,
        confirmation_depth: params.confirmation_depth,
        protocol_fee,
        token_id: params.token_id,
        created_at: current_block as u64,
    };

    msg!("[block_height_prediction::create_market] Market created successfully");
    Ok(serialize(&update))
}

/// Process update for CreateMarketV1
pub fn block_height_prediction_create_market_process_update(
    cid: ContractId,
    update: CreateMarketUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE)?;

    // Create market state
    let market = Market {
        id: update.market_id,
        creator: update.creator,
        target_time: update.target_time,
        base_block_height: update.base_block_height,
        created_at: update.created_at,
        total_pool: 0,
        below_pool: 0,
        above_pool: 0,
        exact_pool: 0,
        state: MarketState::Active,
        resolved_height: None,
        resolution_block: 0,
        confirmation_depth: update.confirmation_depth,
        protocol_fee: update.protocol_fee,
        token_id: update.token_id,
        position_count: 0,
    };

    // Store market
    wasm::db::db_set(markets_db, &serialize(&update.market_id), &serialize(&market))?;
    msg!("[block_height_prediction::create_market::update] Market stored in database");

    Ok(())
}
