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

//! CancelMarketV1 Implementation
//!
//! Cancels a market before resolution, refunding all participants.

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{MarketState, CancelMarketParamsV1, CancelMarketUpdateV1};
use crate::PREDICTION_CONTRACT_MARKETS_TREE;

/// Process instruction for CancelMarketV1
pub fn prediction_market_cancel_market_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CancelMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::cancel_market] Cancelling market");
    msg!("  market_id: {:?}", params.market_id);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::Market = deserialize(&market_bytes)?;

    // Verify market is active
    if market.state != MarketState::Active {
        return Err(PredictionMarketError::MarketNotActive.into())
    }

    // Verify canceller is the creator
    if market.creator != params.canceller {
        return Err(PredictionMarketError::UnauthorizedCaller.into())
    }

    // Create the update
    let update = CancelMarketUpdateV1 {
        market_id: params.market_id,
        state: MarketState::Cancelled,
        refund_amounts: vec![], // Refunds processed via claim mechanism
    };

    msg!("[prediction_market::cancel_market] Market cancellation prepared");
    Ok(serialize(&update))
}

/// Process update for CancelMarketV1
pub fn prediction_market_cancel_market_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: CancelMarketUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;

    // Look up and update the market
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: crate::model::Market = deserialize(&market_bytes)?;

    // Update market state
    market.state = update.state;

    wasm::db::db_set(markets_db, &serialize(&update.market_id), &serialize(&market))?;

    msg!("[prediction_market::cancel_market::update] Market state updated to Cancelled");

    Ok(())
}
