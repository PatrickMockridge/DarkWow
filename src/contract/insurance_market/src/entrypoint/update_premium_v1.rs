/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
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

//! UpdatePremiumV1 Implementation
//!
//! Allows updating the premium rate for an insurance market.

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};

use crate::error::InsuranceMarketError;
use crate::model::UpdatePremiumParamsV1;
use crate::INSURANCE_CONTRACT_MARKETS_TREE;

/// State update for UpdatePremiumV1
#[derive(Debug, Clone, dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
pub struct UpdatePremiumUpdateV1 {
    pub market_id: crate::model::MarketId,
    pub old_premium_rate: u32,
    pub new_premium_rate: u32,
}

/// Process instruction for UpdatePremiumV1
pub fn insurance_market_update_premium_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: UpdatePremiumParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[insurance_market::update_premium] Updating premium for market {:?}",
        params.market_id
    );
    msg!("[insurance_market::update_premium] New premium rate: {} bps", params.new_premium_rate);

    // Validate premium rate (must be > 0 and <= 10000 bps = 100%)
    if params.new_premium_rate == 0 || params.new_premium_rate > 10000 {
        return Err(InsuranceMarketError::InvalidPremiumRate.into())
    }

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes =
        wasm::db::db_get(markets_db, &serialize(&params.market_id))?.ok_or(ContractError::DbGetEmpty)?;
    let market: crate::model::InsuranceMarket = deserialize(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    let old_premium_rate = market.premium_rate;

    // Create the update
    let update = UpdatePremiumUpdateV1 {
        market_id: params.market_id,
        old_premium_rate,
        new_premium_rate: params.new_premium_rate,
    };

    msg!(
        "[insurance_market::update_premium] Premium updated: {} -> {} bps",
        old_premium_rate,
        params.new_premium_rate
    );
    Ok(serialize(&update))
}

/// Process update for UpdatePremiumV1
pub fn insurance_market_update_premium_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: UpdatePremiumUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;

    // Load and update market
    let market_bytes =
        wasm::db::db_get(markets_db, &serialize(&update.market_id))?.ok_or(ContractError::DbGetEmpty)?;
    let mut market: crate::model::InsuranceMarket = deserialize(&market_bytes)?;
    market.premium_rate = update.new_premium_rate;

    wasm::db::db_set(
        markets_db,
        &serialize(&update.market_id),
        &serialize(&market),
    )?;

    msg!(
        "[insurance_market::update_premium::update] Market {:?} premium: {} -> {} bps",
        update.market_id,
        update.old_premium_rate,
        update.new_premium_rate
    );
    Ok(())
}