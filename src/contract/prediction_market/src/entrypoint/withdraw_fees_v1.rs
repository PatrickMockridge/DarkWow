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

//! WithdrawFeesV1 Implementation
//!
//! Allows liquidity providers to withdraw their earned fees.

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{MarketState, WithdrawFeesParamsV1, WithdrawFeesUpdateV1};
use crate::{PREDICTION_CONTRACT_LIQUIDITY_TREE, PREDICTION_CONTRACT_MARKETS_TREE};

/// Process instruction for WithdrawFeesV1
pub fn prediction_market_withdraw_fees_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: WithdrawFeesParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::withdraw_fees] Withdrawing fees");
    msg!("  market_id: {:?}", params.market_id);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::Market = deserialize(&market_bytes)?;

    // Verify market is resolved
    if market.state != MarketState::Resolved {
        return Err(PredictionMarketError::MarketNotResolved.into())
    }

    // Look up LP share
    let liquidity_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_LIQUIDITY_TREE)?;

    if !wasm::db::db_contains_key(liquidity_db, &serialize(&params.provider))? {
        return Err(PredictionMarketError::LpShareNotFound.into())
    }

    let lp_bytes = wasm::db::db_get(liquidity_db, &serialize(&params.provider))?.unwrap();
    let lp: crate::model::LpShare = deserialize(&lp_bytes)?;

    // Verify LP share belongs to this market
    if lp.market_id != params.market_id {
        return Err(PredictionMarketError::LpShareNotFound.into())
    }

    if lp.earned_fees == 0 {
        return Err(PredictionMarketError::NoWinnings.into())
    }

    msg!("[prediction_market::withdraw_fees] Fees to withdraw: {}", lp.earned_fees);

    // Create the update
    let update = WithdrawFeesUpdateV1 {
        market_id: params.market_id,
        provider: params.provider,
        amount: lp.earned_fees,
    };

    msg!("[prediction_market::withdraw_fees] Fee withdrawal prepared");
    Ok(serialize(&update))
}

/// Process update for WithdrawFeesV1
pub fn prediction_market_withdraw_fees_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: WithdrawFeesUpdateV1,
) -> Result<(), ContractError> {
    let liquidity_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_LIQUIDITY_TREE)?;

    // Look up and update LP share
    let lp_bytes = wasm::db::db_get(liquidity_db, &serialize(&update.provider))?.unwrap();
    let mut lp: crate::model::LpShare = deserialize(&lp_bytes)?;

    lp.earned_fees = lp.earned_fees.saturating_sub(update.amount);

    wasm::db::db_set(liquidity_db, &serialize(&update.provider), &serialize(&lp))?;

    msg!(
        "[prediction_market::withdraw_fees::update] Remaining fees: {}",
        lp.earned_fees
    );

    Ok(())
}
