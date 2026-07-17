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

//! CreateMarketV1 Implementation

use dwow_sdk::{
    crypto::poseidon_hash,
    error::ContractError,
    msg,
    wasm,
};
use dwow_serial::{deserialize, serialize};

use crate::error::InsuranceMarketError;
use crate::model::{CreateMarketParamsV1, CreateMarketUpdateV1};
use crate::{INSURANCE_CONTRACT_MARKETS_TREE, INSURANCE_CONTRACT_RISK_TYPES_TREE};

/// Process instruction for CreateMarketV1
pub fn insurance_market_create_market_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CreateMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::create_market] Creating new insurance market");
    msg!("  risk_type_id: {:?}", params.risk_type_id);

    // Verify risk type exists
    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    let risk_type_bytes =
        wasm::db::db_get(risk_types_db, &serialize(&params.risk_type_id))?.ok_or(ContractError::DbGetEmpty)?;
    let risk_type: crate::model::RiskType = deserialize(&risk_type_bytes)?;

    if !risk_type.active {
        return Err(InsuranceMarketError::RiskTypeNotFound.into())
    }

    // Derive market ID
    let market_id = poseidon_hash([
        params.risk_type_id,
        dwow_sdk::pasta::pallas::Base::from(params.total_coverage),
        dwow_sdk::pasta::pallas::Base::from(params.coverage_period),
        dwow_sdk::pasta::pallas::Base::from(wasm::util::get_verifying_block_height()?.get()),
    ]);

    // Check if market already exists
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    if wasm::db::db_contains_key(markets_db, &serialize(&market_id))? {
        return Err(InsuranceMarketError::MarketAlreadyExists.into())
    }

    // Validate parameters
    if params.total_coverage == 0 {
        return Err(InsuranceMarketError::InvalidParameter("Coverage must be > 0".to_string()).into())
    }

    if params.coverage_period == 0 {
        return Err(InsuranceMarketError::InvalidCoveragePeriod.into())
    }

    if params.initial_premium_rate > 10000 {
        return Err(InsuranceMarketError::InvalidPremiumRate.into())
    }

    // Use base premium rate from risk type if 0 provided
    let premium_rate = if params.initial_premium_rate == 0 {
        risk_type.base_premium_rate
    } else {
        params.initial_premium_rate
    };

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create the update
    let update = CreateMarketUpdateV1 {
        market_id,
        risk_type: params.risk_type_id,
        premium_rate,
        total_coverage: params.total_coverage,
        coverage_period: params.coverage_period,
        deductible: params.deductible,
        max_coverage_per_buyer: params.max_coverage_per_buyer,
        created_at: current_block,
        required_underwriter_capability: params.required_underwriter_capability,
        required_buyer_capability: params.required_buyer_capability,
        required_dag_id: params.required_dag_id,
    };

    msg!("[insurance_market::create_market] Market created: {:?}", market_id);
    Ok(serialize(&update))
}

/// Process update for CreateMarketV1
pub fn insurance_market_create_market_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: CreateMarketUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;

    // Create market state
    let market = crate::model::InsuranceMarket {
        version: 1,
        id: update.market_id,
        risk_type: update.risk_type,
        premium_rate: update.premium_rate,
        total_coverage: update.total_coverage,
        coverage_sold: 0,
        coverage_period: update.coverage_period,
        deductible: update.deductible,
        max_coverage_per_buyer: update.max_coverage_per_buyer,
        active: true,
        created_at: update.created_at,
        closes_at: 0,
        required_underwriter_capability: update.required_underwriter_capability,
        required_buyer_capability: update.required_buyer_capability,
        required_dag_id: update.required_dag_id,
    };

    // Store market
    wasm::db::db_set(
        markets_db,
        &serialize(&update.market_id),
        &serialize(&market),
    )?;

    msg!("[insurance_market::create_market::update] Market stored: {:?}", update.market_id);
    Ok(())
}