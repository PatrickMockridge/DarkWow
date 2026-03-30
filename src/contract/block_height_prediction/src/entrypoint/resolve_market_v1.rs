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
//! Resolution uses DarkFi's PoW block hashes to derive cumulative entropy for
//! determining the resolved block height. This leverages the full PoW consensus
//! mechanism rather than relying solely on tx_hash.
//!
//! The `wasm::util::get_block_hash(height)` function is used to retrieve K consecutive
//! block hashes (where K = confirmation_depth), which are then combined using poseidon
//! hashing for cumulative entropy.

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractError,
    msg,
    wasm,
    ContractCall,
};
use darkfi_sdk::pasta::pallas;
use darkfi_serial::{deserialize, serialize};

use crate::error::BlockHeightPredictionError;
use crate::model::{
    calculate_resolution_hash, derive_height_from_entropy, Market, MarketState,
    ResolveMarketParamsV1, ResolveMarketUpdateV1,
};
use crate::BLOCK_HEIGHT_PREDICTION_MARKETS_TREE;

/// Process instruction for ResolveMarketV1
pub fn block_height_prediction_resolve_market_process_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ResolveMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[block_height_prediction::resolve_market] Resolving market");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  observed_height: {}", params.observed_height);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, BLOCK_HEIGHT_PREDICTION_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?
        .ok_or(BlockHeightPredictionError::MarketNotFound)?;

    let mut market: Market = deserialize(&market_bytes)?;

    // Verify market is active
    if market.state != MarketState::Active {
        return Err(BlockHeightPredictionError::MarketAlreadyResolved.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Calculate the earliest block we can resolve at
    // This is based on target_time + confirmation_depth
    let target_block_estimate = market.target_time / 120;
    let resolve_block = target_block_estimate.saturating_add(market.confirmation_depth as u64);

    msg!("  target_block_estimate: {}", target_block_estimate);
    msg!("  resolve_block (target + depth): {}", resolve_block);
    msg!("  current_block: {}", current_block);

    // Verify we have enough confirmations
    if (current_block as u64) < resolve_block {
        return Err(BlockHeightPredictionError::TargetTimeNotReached.into())
    }

    // Get PoW block hashes for cumulative entropy
    // We use K consecutive blocks where K = confirmation_depth
    // Each block hash is influenced by the RandomX PoW output
    let confirmation_depth = market.confirmation_depth as usize;
    let mut entropy = pallas::Base::zero();

    for i in 0..confirmation_depth {
        let block_height = current_block.saturating_sub(i as u32);
        let block_hash = wasm::util::get_block_hash(block_height)?;

        // Convert block_hash bytes to pallas::Base for entropy
        let hash_bytes = block_hash.0;
        let a = u64::from_le_bytes(hash_bytes[0..8].try_into().unwrap());
        let b = u64::from_le_bytes(hash_bytes[8..16].try_into().unwrap());
        let c = u64::from_le_bytes(hash_bytes[16..24].try_into().unwrap());
        let d = u64::from_le_bytes(hash_bytes[24..32].try_into().unwrap());

        let block_entropy = pallas::Base::from(a);
        let block_entropy2 = pallas::Base::from(b);
        let block_entropy3 = pallas::Base::from(c);
        let block_entropy4 = pallas::Base::from(d);

        // Combine entropy from this block
        let block_entropy_combined = calculate_resolution_hash(&[
            block_entropy,
            block_entropy2,
            block_entropy3,
            block_entropy4,
        ]);

        // Cumulative entropy: combine with previous blocks
        entropy = calculate_resolution_hash(&[entropy, block_entropy_combined]);
    }

    // Calculate expected blocks since creation
    let time_elapsed = market.target_time.saturating_sub(market.created_at * 120);
    let expected_blocks = time_elapsed / 120;

    // Derive resolved height from entropy
    let resolved_height = derive_height_from_entropy(
        entropy,
        market.base_block_height,
        expected_blocks,
    );

    msg!("[block_height_prediction::resolve_market] Resolved height: {}", resolved_height);

    // Update market state
    market.state = MarketState::Resolved;
    market.resolved_height = Some(resolved_height);
    market.resolution_block = current_block as u64;

    // Store updated market
    wasm::db::db_set(markets_db, &serialize(&params.market_id), &serialize(&market))?;

    // Create the update
    let update = ResolveMarketUpdateV1 {
        market_id: params.market_id,
        resolved_height,
        resolution_block: current_block as u64,
        state: MarketState::Resolved,
    };

    msg!("[block_height_prediction::resolve_market] Market resolved successfully");
    Ok(serialize(&update))
}

/// Process update for ResolveMarketV1
pub fn block_height_prediction_resolve_market_process_update(
    _cid: ContractId,
    _update: ResolveMarketUpdateV1,
) -> Result<(), ContractError> {
    msg!("[block_height_prediction::resolve_market::update] Resolution confirmed");
    Ok(())
}
