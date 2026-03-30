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
//!
//! Allows position owners to claim their winnings after market resolution.

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{calculate_payout, MarketState, ClaimWinningsParamsV1, ClaimWinningsUpdateV1};
use crate::{PREDICTION_CONTRACT_CLAIMS_TREE, PREDICTION_CONTRACT_MARKETS_TREE, PREDICTION_CONTRACT_POSITIONS_TREE};

/// Process instruction for ClaimWinningsV1
pub fn prediction_market_claim_winnings_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClaimWinningsParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::claim_winnings] Processing winnings claim");
    msg!("  position_id: {:?}", params.position_id);
    msg!("  market_id: {:?}", params.market_id);

    // Look up the position
    let positions_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_POSITIONS_TREE)?;
    let position_bytes = wasm::db::db_get(positions_db, &serialize(&params.position_id))?.unwrap();
    let position: crate::model::Position = deserialize(&position_bytes)?;

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::Market = deserialize(&market_bytes)?;

    // Verify market is resolved
    if market.state != MarketState::Resolved {
        return Err(PredictionMarketError::MarketNotResolved.into())
    }

    // Verify position matches market
    if position.market_id != params.market_id {
        return Err(PredictionMarketError::PositionNotFound.into())
    }

    // Check if already claimed
    if position.claimed {
        return Err(PredictionMarketError::AlreadyClaimed.into())
    }

    // Get winning outcome
    let winning_outcome = market.resolved_outcome.ok_or(PredictionMarketError::InvalidOracleAttestation)?;

    // Calculate payout
    let payout = if position.outcome == winning_outcome {
        let winning_pool = market.outcome_pools[winning_outcome as usize];
        calculate_payout(
            position.amount,
            winning_pool,
            market.total_pool,
            market.protocol_fee,
            market.lp_fee,
        )
    } else {
        0
    };

    if payout == 0 {
        return Err(PredictionMarketError::NoWinnings.into())
    }

    msg!("[prediction_market::claim_winnings] Calculated payout: {}", payout);

    // Create the update
    let update = ClaimWinningsUpdateV1 {
        position_id: params.position_id,
        payout,
        claimed: true,
    };

    msg!("[prediction_market::claim_winnings] Claim prepared successfully");
    Ok(serialize(&update))
}

/// Process update for ClaimWinningsV1
pub fn prediction_market_claim_winnings_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ClaimWinningsUpdateV1,
) -> Result<(), ContractError> {
    let positions_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_POSITIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_CLAIMS_TREE)?;

    // Look up and update the position
    let position_bytes = wasm::db::db_get(positions_db, &serialize(&update.position_id))?.unwrap();
    let mut position: crate::model::Position = deserialize(&position_bytes)?;

    // Update position
    position.claimed = true;
    wasm::db::db_set(
        positions_db,
        &serialize(&update.position_id),
        &serialize(&position),
    )?;
    msg!("[prediction_market::claim_winnings::update] Position marked as claimed");

    // Store claim record (for double-claim prevention)
    wasm::db::db_set(
        claims_db,
        &serialize(&update.position_id),
        &serialize(&update),
    )?;
    msg!("[prediction_market::claim_winnings::update] Claim record stored");

    Ok(())
}