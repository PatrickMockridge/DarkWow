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
//!
//! This function is called as a child of Money::Burn to create a position
//! when the user locks their bet value.

use darkfi_sdk::{
    crypto::{pasta_prelude::{Curve, CurveAffine}, poseidon_hash},
    error::ContractError,
    msg,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{validate_amount, CreatePositionParamsV1, CreatePositionUpdateV1, MarketState};
use crate::{PREDICTION_CONTRACT_MARKETS_TREE, PREDICTION_CONTRACT_POSITIONS_TREE};

/// Process instruction for CreatePositionV1
pub fn prediction_market_create_position_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CreatePositionParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::create_position] Creating new position");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  outcome: {}", params.outcome);
    msg!("  amount: {}", params.amount);

    // Validate amount
    validate_amount(params.amount)?;

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::Market = deserialize(&market_bytes)?;

    // Verify market is active
    if market.state != MarketState::Active {
        return Err(PredictionMarketError::MarketNotActive.into())
    }

    // Verify betting hasn't closed
    let current_block = wasm::util::get_verifying_block_height()?;
    if current_block as u64 >= market.betting_closes {
        return Err(PredictionMarketError::MarketNotActive.into())
    }

    // Verify outcome is valid
    if params.outcome >= market.num_outcomes {
        return Err(PredictionMarketError::InvalidOutcome.into())
    }

    // Check if position already exists (prevent double-betting)
    // Use a hash of value_commit coordinates as a pseudo-random nonce
    let vc_coords = params.value_commit.to_affine().coordinates();
    let nonce = if vc_coords.is_some().into() {
        let coords = vc_coords.unwrap();
        poseidon_hash([*coords.x(), *coords.y()])
    } else {
        darkfi_sdk::pasta::pallas::Base::zero()
    };
    let position_id = crate::model::derive_position_id(
        params.market_id,
        &params.owner,
        params.outcome,
        params.amount,
        nonce,
    );

    let positions_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_POSITIONS_TREE)?;
    if wasm::db::db_contains_key(positions_db, &serialize(&position_id))? {
        return Err(PredictionMarketError::PositionAlreadyExists.into())
    }

    // Create the update
    let update = CreatePositionUpdateV1 {
        position_id,
        market_id: params.market_id,
        owner: params.owner,
        outcome: params.outcome,
        amount: params.amount,
        created_at: current_block as u64,
    };

    msg!("[prediction_market::create_position] Position created successfully");
    Ok(serialize(&update))
}

/// Process update for CreatePositionV1
pub fn prediction_market_create_position_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: CreatePositionUpdateV1,
) -> Result<(), ContractError> {
    let positions_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_POSITIONS_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;

    // Create position state
    let position = crate::model::Position {
        id: update.position_id,
        market_id: update.market_id,
        owner: update.owner,
        outcome: update.outcome,
        amount: update.amount,
        potential_payout: 0, // Calculated at resolution
        claimed: false,
        created_at: update.created_at,
    };

    // Store position
    wasm::db::db_set(
        positions_db,
        &serialize(&update.position_id),
        &serialize(&position),
    )?;
    msg!("[prediction_market::create_position::update] Position stored");

    // Update market pool
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: crate::model::Market = deserialize(&market_bytes)?;
    market.total_pool += update.amount;
    market.outcome_pools[update.outcome as usize] += update.amount;
    wasm::db::db_set(markets_db, &serialize(&update.market_id), &serialize(&market))?;
    msg!("[prediction_market::create_position::update] Market pool updated");

    Ok(())
}
