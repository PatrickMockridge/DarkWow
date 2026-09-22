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

//! UnderwriteWithCapabilityV1 Implementation
//!
//! Allows underwriting with an O-Cap capability token instead of direct authorization.

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

use crate::error::InsuranceMarketError;
use crate::InsuranceMarketFunction;
use crate::model::{
    calculate_max_coverage,
    derive_underwriter_id,
    UnderwriteWithCapabilityParamsV1,
    UnderwriteWithCapabilityUpdateV1,
};
use crate::{
    INSURANCE_CONTRACT_MARKETS_TREE, INSURANCE_CONTRACT_RISK_TYPES_TREE,
    INSURANCE_CONTRACT_UNDERWRITERS_TREE,
};

/// Process instruction for UnderwriteWithCapabilityV1
pub fn insurance_market_underwrite_with_capability_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = UnderwriteWithCapabilityParamsV1::decode(&self_.data[1..])?;

    msg!("[insurance_market::underwrite_with_cap] Registering as underwriter with capability");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  bond_amount: {}", params.bond_amount);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &params.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let market = crate::model::InsuranceMarket::decode(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    // Verify market requires a capability for underwriting
    if market.required_underwriter_capability.is_none() {
        return Err(InsuranceMarketError::CapabilityNotMet.into())
    }

    #[expect(clippy::unwrap_used, reason = "guarded by is_none() check above")]
    let required_capability_id = market.required_underwriter_capability.unwrap();

    // ZK proof verified by host via get_metadata
    // (namespace: INSURANCE_MARKET_ZKAS_UNDERWRITE_WITH_CAPABILITY_NS_V1)

    // Look up risk type to get min bond rate
    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    let risk_type_bytes =
        wasm::db::db_get(risk_types_db, &market.risk_type.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let risk_type = crate::model::RiskType::decode(&risk_type_bytes)?;

    // Validate bond amount meets minimum
    let min_bond = (params.coverage_limit * risk_type.min_bond_rate as u64) / 10000;
    if params.bond_amount < min_bond {
        return Err(InsuranceMarketError::InsufficientBond.into())
    }

    // Calculate max coverage this bond can support (10x leverage default)
    let coverage_leverage = 10u32;
    let max_coverage = calculate_max_coverage(params.bond_amount, coverage_leverage)?;

    if params.coverage_limit > max_coverage {
        return Err(InsuranceMarketError::BondTooSmall.into())
    }

    // Check if coverage_limit would exceed market's remaining coverage
    let remaining_coverage = market.total_coverage - market.coverage_sold;
    if params.coverage_limit > remaining_coverage {
        return Err(InsuranceMarketError::InsufficientCoverage.into())
    }

    // Derive underwriter ID
    let underwriter_id =
        derive_underwriter_id(params.market_id, &params.underwriter, params.bond_amount);

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Build both records here. apply used to read the underwriter (or construct it) and read the
    // market back after the block was accepted — reads the ACL denies in `Update` (register
    // OBL-C72).
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let underwriter = if wasm::db::db_contains_key(underwriters_db, &underwriter_id.to_repr())? {
        let bytes = wasm::db::db_get(underwriters_db, &underwriter_id.to_repr())?
            .ok_or(ContractError::DbGetEmpty)?;
        let mut existing = crate::model::Underwriter::decode(&bytes)?;
        existing.bond_amount += params.bond_amount;
        existing.coverage_provided += params.coverage_limit;
        existing
    } else {
        crate::model::Underwriter {
            version: 1,
            id: underwriter_id,
            owner: params.underwriter,
            market_id: params.market_id,
            bond_amount: params.bond_amount,
            coverage_provided: params.coverage_limit,
            coverage_sold: 0,
            earned_premiums: 0,
            claims_paid: 0,
            slash_count: 0,
            performance_score: 10000, // Start at perfect score
            active: true,
            created_at: current_block,
        }
    };

    let mut market = market;
    market.coverage_sold += params.coverage_limit;

    // Create the update
    let update = UnderwriteWithCapabilityUpdateV1 {
        underwriter_id,
        market_id: params.market_id,
        owner: params.underwriter,
        bond_amount: params.bond_amount,
        coverage_provided: params.coverage_limit,
        required_capability_id,
        created_at: current_block,
        underwriter_bytes: underwriter.encode(),
        market_bytes: market.encode(),
    };

    msg!(
        "[insurance_market::underwrite_with_cap] Underwriter registered with capability: {:?}",
        underwriter_id
    );
    Ok([&[InsuranceMarketFunction::UnderwriteWithCapabilityV1 as u8],
        &update.encode()?[..]].concat())
}

/// Process update for UnderwriteWithCapabilityV1
pub fn insurance_market_underwrite_with_capability_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: UnderwriteWithCapabilityUpdateV1,
) -> Result<(), ContractError> {
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;

    // Blind writes — exec built the underwriter (new or updated) and advanced the market's
    // coverage_sold, and carried both (register OBL-C72).
    wasm::db::db_set(
        underwriters_db,
        &update.underwriter_id.to_repr(),
        &update.underwriter_bytes,
    )?;

    wasm::db::db_set(
        markets_db,
        &update.market_id.to_repr(),
        &update.market_bytes,
    )?;

    msg!(
        "[insurance_market::underwrite_with_cap::update] Underwriter: {:?}, Coverage: {}, Required Cap: {:?}",
        update.underwriter_id,
        update.coverage_provided,
        update.required_capability_id
    );
    Ok(())
}