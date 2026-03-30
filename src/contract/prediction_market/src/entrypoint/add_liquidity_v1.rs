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

//! AddLiquidityV1 Implementation
//!
//! Allows liquidity providers to add funds to a market's liquidity pool.

use darkfi_sdk::{
    crypto::{pasta_prelude::{Curve, CurveAffine}, poseidon_hash},
    error::ContractError,
    msg,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::PredictionMarketError;
use crate::model::{derive_lp_share_id, calculate_lp_shares, validate_amount, MarketState};
use crate::model::AddLiquidityParamsV1;
use crate::model::AddLiquidityUpdateV1;
use crate::{PREDICTION_CONTRACT_LIQUIDITY_TREE, PREDICTION_CONTRACT_MARKETS_TREE};

/// Process instruction for AddLiquidityV1
pub fn prediction_market_add_liquidity_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: AddLiquidityParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[prediction_market::add_liquidity] Adding liquidity");
    msg!("  market_id: {:?}", params.market_id);
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

    // Look up existing LP share for this provider
    let liquidity_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_LIQUIDITY_TREE)?;

    // Calculate LP shares to mint
    let existing_shares: u64 = if wasm::db::db_contains_key(liquidity_db, &serialize(&params.provider))? {
        let lp_bytes = wasm::db::db_get(liquidity_db, &serialize(&params.provider))?.unwrap();
        let lp: crate::model::LpShare = deserialize(&lp_bytes)?;
        lp.shares
    } else {
        0
    };

    let shares_to_mint = calculate_lp_shares(params.amount, existing_shares, market.total_lp_shares)?;

    // Derive LP share ID
    let vc_coords = params.value_commit.to_affine().coordinates();
    let nonce = if vc_coords.is_some().into() {
        let coords = vc_coords.unwrap();
        poseidon_hash([*coords.x(), *coords.y()])
    } else {
        darkfi_sdk::pasta::pallas::Base::zero()
    };
    let lp_share_id = derive_lp_share_id(
        params.market_id,
        &params.provider,
        shares_to_mint,
        nonce,
    );

    msg!("[prediction_market::add_liquidity] LP shares to mint: {}", shares_to_mint);

    // Create the update
    let update = AddLiquidityUpdateV1 {
        lp_share_id,
        market_id: params.market_id,
        provider: params.provider,
        shares_minted: shares_to_mint,
        fees_earned: 0,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[prediction_market::add_liquidity] Liquidity added successfully");
    Ok(serialize(&update))
}

/// Process update for AddLiquidityV1
pub fn prediction_market_add_liquidity_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: AddLiquidityUpdateV1,
) -> Result<(), ContractError> {
    let liquidity_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_LIQUIDITY_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, PREDICTION_CONTRACT_MARKETS_TREE)?;

    // Look up or create LP share
    let existing_shares: u64;
    let _existing_fees: u64;

    if wasm::db::db_contains_key(liquidity_db, &serialize(&update.provider))? {
        let lp_bytes = wasm::db::db_get(liquidity_db, &serialize(&update.provider))?.unwrap();
        let mut lp: crate::model::LpShare = deserialize(&lp_bytes)?;
        lp.shares += update.shares_minted;
        existing_shares = lp.shares;
        _existing_fees = lp.earned_fees;
        wasm::db::db_set(liquidity_db, &serialize(&update.provider), &serialize(&lp))?;
    } else {
        let lp = crate::model::LpShare {
            id: update.lp_share_id,
            market_id: update.market_id,
            provider: update.provider,
            shares: update.shares_minted,
            earned_fees: 0,
            created_at: update.created_at,
        };
        existing_shares = update.shares_minted;
        _existing_fees = 0;
        wasm::db::db_set(liquidity_db, &serialize(&update.provider), &serialize(&lp))?;
    }

    // Update market total liquidity
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: crate::model::Market = deserialize(&market_bytes)?;
    market.total_pool += update.shares_minted; // LP adds to total pool
    market.total_lp_shares += update.shares_minted; // Track LP shares separately
    wasm::db::db_set(markets_db, &serialize(&update.market_id), &serialize(&market))?;

    msg!(
        "[prediction_market::add_liquidity::update] LP shares: {}, Total pool: {}",
        existing_shares,
        market.total_pool
    );

    Ok(())
}
