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

use dwow_sdk::{error::ContractError, msg, pasta::pallas, wasm};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

use crate::error::InsuranceMarketError;
use crate::InsuranceMarketFunction;
use crate::model::UpdatePremiumParamsV1;
use crate::INSURANCE_CONTRACT_MARKETS_TREE;

/// State update for UpdatePremiumV1
#[derive(Debug, Clone)]
pub struct UpdatePremiumUpdateV1 {
    pub market_id: crate::model::MarketId,
    pub old_premium_rate: u32,
    pub new_premium_rate: u32,
    /// `InsuranceMarket::encode()` with the new premium rate applied, as exec produced it. Carried
    /// so apply re-stores it rather than reading it back (register OBL-C72).
    pub market_bytes: Vec<u8>,
}

#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl UpdatePremiumUpdateV1 {
    pub const FIXED: usize = 44;
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.market_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.market_bytes.len()); b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.old_premium_rate.to_le_bytes()); b.extend_from_slice(&self.new_premium_rate.to_le_bytes()); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.market_bytes); Ok(b) }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("UpdatePremiumUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[40..44].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("UpdatePremiumUpdateV1: {} record bytes do not fit {} total", n, data.len()))); } Ok(UpdatePremiumUpdateV1 { market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdatePremiumUpdateV1: invalid market_id".into()))?, old_premium_rate: u32::from_le_bytes(data[32..36].try_into().unwrap()), new_premium_rate: u32::from_le_bytes(data[36..40].try_into().unwrap()), market_bytes: data[Self::FIXED..].to_vec() }) }
}

/// Process instruction for UpdatePremiumV1
pub fn insurance_market_update_premium_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = UpdatePremiumParamsV1::decode(&self_.data[1..])?;

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
        wasm::db::db_get(markets_db, &params.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut market = crate::model::InsuranceMarket::decode(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    let old_premium_rate = market.premium_rate;

    // Apply the new rate here and carry the record — apply may not read it (OBL-C72).
    market.premium_rate = params.new_premium_rate;

    // Create the update
    let update = UpdatePremiumUpdateV1 {
        market_id: params.market_id,
        old_premium_rate,
        new_premium_rate: params.new_premium_rate,
        market_bytes: market.encode(),
    };

    msg!(
        "[insurance_market::update_premium] Premium updated: {} -> {} bps",
        old_premium_rate,
        params.new_premium_rate
    );
    Ok([&[InsuranceMarketFunction::UpdatePremiumV1 as u8], &update.encode()?[..]].concat())
}

/// Process update for UpdatePremiumV1
pub fn insurance_market_update_premium_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: UpdatePremiumUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;

    // Blind write — exec applied the new rate and carried the record (OBL-C72).
    wasm::db::db_set(
        markets_db,
        &update.market_id.to_repr(),
        &update.market_bytes,
    )?;

    msg!(
        "[insurance_market::update_premium::update] Market {:?} premium: {} -> {} bps",
        update.market_id,
        update.old_premium_rate,
        update.new_premium_rate
    );
    Ok(())
}