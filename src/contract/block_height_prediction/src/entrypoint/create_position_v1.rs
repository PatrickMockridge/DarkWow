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

//! CreatePositionV1 Implementation

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
    derive_position_id, validate_amount, validate_tolerance, Market, MarketState, Position,
    PositionType, CreatePositionParamsV1, CreatePositionUpdateV1,
};
use crate::{
    BLOCK_HEIGHT_PREDICTION_MARKETS_TREE, BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE,
};

/// Process instruction for CreatePositionV1
pub fn block_height_prediction_create_position_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CreatePositionParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[block_height_prediction::create_position] Processing bet");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  predicted_height: {}", params.predicted_height);
    msg!("  position_type: {}", params.position_type);
    msg!("  amount: {}", params.amount);

    // Validate position type
    let position_type = PositionType::try_from(params.position_type)?;

    // Validate amount
    validate_amount(params.amount)?;

    // Validate tolerance
    validate_tolerance(params.tolerance)?;

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?
        .ok_or(BlockHeightPredictionError::MarketNotFound)?;

    let mut market: Market = deserialize(&market_bytes)?;

    // Verify market is active
    if market.state != MarketState::Active {
        return Err(BlockHeightPredictionError::MarketNotActive.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Calculate settle block (market can be resolved after this)
    let settle_block = market.target_time / 120 + market.confirmation_depth as u64;
    if (current_block as u64) < settle_block {
        return Err(BlockHeightPredictionError::BettingClosed.into())
    }

    // Derive position ID
    let position_id = derive_position_id(
        params.market_id,
        &params.owner,
        params.predicted_height,
        position_type,
        params.amount,
        params.signature,
    );

    msg!("[block_height_prediction::create_position] Derived position_id");

    // Check if position already exists
    let positions_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE)?;
    if wasm::db::db_contains_key(positions_db, &serialize(&position_id))? {
        return Err(BlockHeightPredictionError::PositionAlreadyExists.into())
    }

    // Update market pools
    market.total_pool = market.total_pool.checked_add(params.amount)
        .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?;

    match position_type {
        PositionType::Below => {
            market.below_pool = market.below_pool.checked_add(params.amount)
                .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?
        }
        PositionType::Above => {
            market.above_pool = market.above_pool.checked_add(params.amount)
                .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?
        }
        PositionType::Exact => {
            market.exact_pool = market.exact_pool.checked_add(params.amount)
                .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?
        }
    }

    market.position_count = market.position_count.checked_add(1)
        .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?;

    // Store updated market
    wasm::db::db_set(markets_db, &serialize(&params.market_id), &serialize(&market))?;

    // Create the update
    let update = CreatePositionUpdateV1 {
        position_id,
        market_id: params.market_id,
        owner: params.owner,
        predicted_height: params.predicted_height,
        tolerance: params.tolerance,
        position_type,
        amount: params.amount,
        created_at: current_block as u64,
    };

    msg!("[block_height_prediction::create_position] Position created successfully");
    Ok(serialize(&update))
}

/// Process update for CreatePositionV1
pub fn block_height_prediction_create_position_process_update(
    cid: ContractId,
    update: CreatePositionUpdateV1,
) -> Result<(), ContractError> {
    let positions_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE)?;

    // Create position state
    let position = Position {
        id: update.position_id,
        market_id: update.market_id,
        owner: update.owner,
        predicted_height: update.predicted_height,
        tolerance: update.tolerance,
        position_type: update.position_type,
        amount: update.amount,
        potential_payout: 0,
        claimed: false,
        created_at: update.created_at,
    };

    // Store position
    wasm::db::db_set(positions_db, &serialize(&update.position_id), &serialize(&position))?;
    msg!("[block_height_prediction::create_position::update] Position stored in database");

    Ok(())
}
