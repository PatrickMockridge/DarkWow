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
use crate::model::{Market, MarketState, CancelMarketParamsV1, CancelMarketUpdateV1};
use crate::BLOCK_HEIGHT_PREDICTION_MARKETS_TREE;

/// Process instruction for CancelMarketV1
pub fn block_height_prediction_cancel_market_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CancelMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[block_height_prediction::cancel_market] Cancelling market");
    msg!("  market_id: {:?}", params.market_id);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?
        .ok_or(BlockHeightPredictionError::MarketNotFound)?;

    let mut market: Market = deserialize(&market_bytes)?;

    // Verify market is active
    if market.state != MarketState::Active {
        return Err(BlockHeightPredictionError::MarketNotActive.into())
    }

    // Update market state
    market.state = MarketState::Cancelled;

    // Store updated market
    wasm::db::db_set(markets_db, &serialize(&params.market_id), &serialize(&market))?;

    // Create the update (refund amounts would be calculated by client)
    let update = CancelMarketUpdateV1 {
        market_id: params.market_id,
        state: MarketState::Cancelled,
        refund_amounts: vec![],
    };

    msg!("[block_height_prediction::cancel_market] Market cancelled successfully");
    Ok(serialize(&update))
}

/// Process update for CancelMarketV1
pub fn block_height_prediction_cancel_market_process_update(
    _cid: ContractId,
    _update: CancelMarketUpdateV1,
) -> Result<(), ContractError> {
    // Cancellation already processed

    msg!("[block_height_prediction::cancel_market::update] Cancellation confirmed");
    Ok(())
}
