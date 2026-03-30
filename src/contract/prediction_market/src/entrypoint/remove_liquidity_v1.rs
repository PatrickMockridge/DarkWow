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

//! RemoveLiquidityV1 Implementation
//!
//! Allows liquidity providers to remove their funds and earned fees.

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{calculate_liquidity_payout, MarketState, RemoveLiquidityParamsV1, RemoveLiquidityUpdateV1};
use crate::{PREDICTION_CONTRACT_LIQUIDITY_TREE, PREDICTION_CONTRACT_MARKETS_TREE};

/// Process instruction for RemoveLiquidityV1
pub fn prediction_market_remove_liquidity_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: RemoveLiquidityParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::remove_liquidity] Removing liquidity");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  shares: {}", params.shares);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::Market = deserialize(&market_bytes)?;

    // Verify market is resolved (can only withdraw after resolution)
    if market.state != MarketState::Resolved && market.state != MarketState::Cancelled {
        return Err(PredictionMarketError::InvalidMarketState.into())
    }

    // Look up LP share
    let liquidity_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_LIQUIDITY_TREE)?;
    let lp_bytes = wasm::db::db_get(liquidity_db, &serialize(&params.provider))?.unwrap();
    let lp: crate::model::LpShare = deserialize(&lp_bytes)?;

    // Verify LP share belongs to this market
    if lp.market_id != params.market_id {
        return Err(PredictionMarketError::LpShareNotFound.into())
    }

    // Verify enough shares
    if lp.shares < params.shares {
        return Err(PredictionMarketError::InsufficientLiquidity.into())
    }

    // Calculate payout using total LP shares (not individual)
    let payout = calculate_liquidity_payout(params.shares, market.total_pool, market.total_lp_shares)?;
    let fees_withdrawn = (params.shares * lp.earned_fees) / lp.shares.max(1);

    msg!(
        "[prediction_market::remove_liquidity] Payout: {}, Fees: {}",
        payout,
        fees_withdrawn
    );

    // Create the update
    let update = RemoveLiquidityUpdateV1 {
        market_id: params.market_id,
        provider: params.provider,
        shares_burned: params.shares,
        payout,
        fees_withdrawn,
    };

    msg!("[prediction_market::remove_liquidity] Liquidity removal prepared");
    Ok(serialize(&update))
}

/// Process update for RemoveLiquidityV1
pub fn prediction_market_remove_liquidity_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: RemoveLiquidityUpdateV1,
) -> Result<(), ContractError> {
    let liquidity_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_LIQUIDITY_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;

    // Look up and update LP share
    let lp_bytes = wasm::db::db_get(liquidity_db, &serialize(&update.provider))?.unwrap();
    let mut lp: crate::model::LpShare = deserialize(&lp_bytes)?;

    lp.shares = lp.shares.saturating_sub(update.shares_burned);
    lp.earned_fees = lp.earned_fees.saturating_sub(update.fees_withdrawn);

    wasm::db::db_set(liquidity_db, &serialize(&update.provider), &serialize(&lp))?;

    // Update market total LP shares
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: crate::model::Market = deserialize(&market_bytes)?;
    market.total_lp_shares = market.total_lp_shares.saturating_sub(update.shares_burned);
    wasm::db::db_set(markets_db, &serialize(&update.market_id), &serialize(&market))?;

    msg!(
        "[prediction_market::remove_liquidity::update] Remaining shares: {}, Fees: {}, Total LP shares: {}",
        lp.shares,
        lp.earned_fees,
        market.total_lp_shares
    );

    Ok(())
}
