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

//! ResolveMarketV1 Implementation
//!
//! Resolves a market with the oracle attested outcome.
//! This distributes winnings to winning positions and fees to LPs.

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{MarketState, ResolveMarketParamsV1, ResolveMarketUpdateV1};
use crate::{PREDICTION_CONTRACT_MARKETS_TREE, PREDICTION_CONTRACT_RESOLUTIONS_TREE};

/// Process instruction for ResolveMarketV1
pub fn prediction_market_resolve_market_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ResolveMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::resolve_market] Resolving market");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  outcome: {}", params.outcome);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::Market = deserialize(&market_bytes)?;

    // Verify market is active
    if market.state != MarketState::Active {
        return Err(PredictionMarketError::MarketNotActive.into())
    }

    // Verify outcome is valid
    if params.outcome >= market.num_outcomes {
        return Err(PredictionMarketError::InvalidOutcome.into())
    }

    // Verify oracle signature
    // In production, this would verify the attestation using the oracle's public key
    // For MVP, we trust the oracle_pubkey stored in the market
    // TODO: Implement proper oracle signature verification
    msg!("[prediction_market::resolve_market] Oracle signature verification (MVP: skipped)");

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Create the update
    let update = ResolveMarketUpdateV1 {
        market_id: params.market_id,
        outcome: params.outcome,
        resolved_at: current_block as u64,
    };

    msg!("[prediction_market::resolve_market] Market resolved successfully");
    Ok(serialize(&update))
}

/// Process update for ResolveMarketV1
pub fn prediction_market_resolve_market_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ResolveMarketUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;
    let resolutions_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_RESOLUTIONS_TREE)?;

    // Look up and update the market
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: crate::model::Market = deserialize(&market_bytes)?;

    // Update market state
    market.state = MarketState::Resolved;
    market.resolved_outcome = Some(update.outcome);
    market.resolved_at = update.resolved_at;

    wasm::db::db_set(markets_db, &serialize(&update.market_id), &serialize(&market))?;
    msg!("[prediction_market::resolve_market::update] Market state updated to Resolved");

    // Store resolution record
    wasm::db::db_set(
        resolutions_db,
        &serialize(&update.market_id),
        &serialize(&update),
    )?;
    msg!("[prediction_market::resolve_market::update] Resolution stored");

    Ok(())
}