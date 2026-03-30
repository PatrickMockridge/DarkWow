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

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{
    validate_num_outcomes, validate_protocol_fee, CreateMarketParamsV1, CreateMarketUpdateV1,
};
use crate::{PREDICTION_CONTRACT_INFO_TREE, PREDICTION_CONTRACT_MARKETS_TREE};

/// Process instruction for CreateMarketV1
pub fn prediction_market_create_market_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CreateMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::create_market] Creating new market");
    msg!("  question: {:?}", String::from_utf8_lossy(&params.question));
    msg!("  num_outcomes: {}", params.num_outcomes);
    msg!("  resolve_time: {}", params.resolve_time);

    // Validate inputs
    validate_num_outcomes(params.num_outcomes)?;
    if params.question.is_empty() || params.question.len() > 512 {
        return Err(PredictionMarketError::QuestionTooLong.into())
    }

    // Get default fees if not specified
    let info_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_INFO_TREE)?;
    let default_protocol_fee: u32 = if params.protocol_fee == 0 {
        deserialize(&wasm::db::db_get(info_db, crate::PREDICTION_CONTRACT_PROTOCOL_FEE)?.unwrap())?
    } else {
        params.protocol_fee
    };
    validate_protocol_fee(default_protocol_fee)?;

    let default_lp_fee: u32 = if params.lp_fee == 0 {
        deserialize(&wasm::db::db_get(info_db, crate::PREDICTION_CONTRACT_LP_FEE)?.unwrap())?
    } else {
        params.lp_fee
    };

    // Derive market ID
    let market_id = crate::model::derive_market_id(
        &params.oracle_pubkey,
        &params.question,
        params.resolve_time,
        params.token_id,
        &params.oracle_pubkey,
    );

    msg!("[prediction_market::create_market] Derived market_id");

    // Check if market already exists
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    if wasm::db::db_contains_key(markets_db, &serialize(&market_id))? {
        return Err(PredictionMarketError::MarketAlreadyExists.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Create the update
    let update = CreateMarketUpdateV1 {
        market_id,
        creator: params.oracle_pubkey, // For MVP, oracle_pubkey is also creator
        question: params.question.clone(),
        resolve_time: params.resolve_time,
        betting_closes: if params.betting_closes == 0 {
            params.resolve_time
        } else {
            params.betting_closes
        },
        num_outcomes: params.num_outcomes,
        protocol_fee: default_protocol_fee,
        lp_fee: default_lp_fee,
        token_id: params.token_id,
        oracle_pubkey: params.oracle_pubkey,
        created_at: current_block as u64,
    };

    msg!("[prediction_market::create_market] Market created successfully");
    Ok(serialize(&update))
}

/// Process update for CreateMarketV1
pub fn prediction_market_create_market_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: CreateMarketUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;

    // Create market state
    let market = crate::model::Market {
        id: update.market_id,
        creator: update.creator,
        question: update.question.clone(),
        resolve_time: update.resolve_time,
        betting_closes: update.betting_closes,
        num_outcomes: update.num_outcomes,
        total_pool: 0,
        total_lp_shares: 0,
        outcome_pools: vec![0u64; update.num_outcomes as usize],
        state: crate::model::MarketState::Active,
        resolved_outcome: None,
        protocol_fee: update.protocol_fee,
        lp_fee: update.lp_fee,
        token_id: update.token_id,
        oracle_pubkey: update.oracle_pubkey,
        created_at: update.created_at,
        resolved_at: 0,
    };

    // Store market
    wasm::db::db_set(markets_db, &serialize(&update.market_id), &serialize(&market))?;
    msg!("[prediction_market::create_market::update] Market stored in database");

    Ok(())
}
