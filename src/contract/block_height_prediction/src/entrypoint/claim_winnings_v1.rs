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

//! ClaimWinningsV1 Implementation

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
    calculate_payout, position_wins, Market, MarketState, Position, PositionOutcome,
    PositionType, ClaimWinningsParamsV1, ClaimWinningsUpdateV1,
};
use crate::{
    BLOCK_HEIGHT_PREDICTION_CLAIMS_TREE, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE,
    BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE,
};

/// Process instruction for ClaimWinningsV1
pub fn block_height_prediction_claim_winnings_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClaimWinningsParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[block_height_prediction::claim_winnings] Claiming winnings");
    msg!("  position_id: {:?}", params.position_id);
    msg!("  market_id: {:?}", params.market_id);

    // Look up the position
    let positions_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE)?;
    let position_bytes = wasm::db::db_get(positions_db, &serialize(&params.position_id))?
        .ok_or(BlockHeightPredictionError::PositionNotFound)?;

    let mut position: Position = deserialize(&position_bytes)?;

    // Verify ownership
    if position.owner != params.owner {
        return Err(BlockHeightPredictionError::Unauthorized.into())
    }

    // Check if already claimed
    let claims_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_CLAIMS_TREE)?;
    if wasm::db::db_contains_key(claims_db, &serialize(&params.position_id))? {
        return Err(BlockHeightPredictionError::WinningsClaimed.into())
    }

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?
        .ok_or(BlockHeightPredictionError::MarketNotFound)?;

    let market: Market = deserialize(&market_bytes)?;

    // Verify market is resolved
    if market.state != MarketState::Resolved {
        return Err(BlockHeightPredictionError::MarketNotActive.into())
    }

    let resolved_height =
        market.resolved_height.ok_or(BlockHeightPredictionError::InvalidBlockHeight)?;

    // Determine if position wins
    let outcome = position_wins(&position, resolved_height);

    msg!("  resolved_height: {}", resolved_height);
    msg!("  position predicted: {}", position.predicted_height);
    msg!("  outcome: {:?}", outcome);

    // Calculate payout based on outcome
    let payout = match outcome {
        PositionOutcome::Won => {
            // Get the winning pool based on position type
            let winning_pool = match position.position_type {
                PositionType::Below => market.below_pool,
                PositionType::Above => market.above_pool,
                PositionType::Exact => market.exact_pool,
            };

            if winning_pool == 0 {
                0
            } else {
                calculate_payout(
                    position.amount,
                    winning_pool,
                    market.total_pool,
                    market.protocol_fee,
                )?
            }
        }
        PositionOutcome::Close => {
            // Close is worth half the full payout
            let winning_pool = match position.position_type {
                PositionType::Below => market.below_pool,
                PositionType::Above => market.above_pool,
                PositionType::Exact => market.exact_pool,
            };

            if winning_pool == 0 {
                0
            } else {
                calculate_payout(
                    position.amount,
                    winning_pool,
                    market.total_pool,
                    market.protocol_fee,
                )? / 2
            }
        }
        PositionOutcome::Exact => {
            // Exact prediction gets 3x jackpot bonus
            let winning_pool = market.exact_pool.max(1);
            let base_payout = calculate_payout(
                position.amount,
                winning_pool,
                market.total_pool,
                market.protocol_fee,
            )?;
            base_payout.saturating_mul(3)
        }
        PositionOutcome::Lost => 0,
    };

    msg!("  payout: {}", payout);

    // Mark position as claimed
    position.claimed = true;
    position.potential_payout = payout;

    // Store updated position
    wasm::db::db_set(positions_db, &serialize(&params.position_id), &serialize(&position))?;

    // Mark claim in claims tree to prevent double-claim
    wasm::db::db_set(claims_db, &serialize(&params.position_id), &serialize(&payout))?;

    // Create the update
    let update = ClaimWinningsUpdateV1 { position_id: params.position_id, payout, claimed: true };

    msg!("[block_height_prediction::claim_winnings] Winnings claimed successfully");
    Ok(serialize(&update))
}

/// Process update for ClaimWinningsV1
pub fn block_height_prediction_claim_winnings_process_update(
    _cid: ContractId,
    _update: ClaimWinningsUpdateV1,
) -> Result<(), ContractError> {
    // Claim already processed, this is for event emission if needed

    msg!("[block_height_prediction::claim_winnings::update] Claim confirmed");
    Ok(())
}
