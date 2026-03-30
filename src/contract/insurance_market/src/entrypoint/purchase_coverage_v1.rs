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

//! PurchaseCoverageV1 Implementation

use darkfi_sdk::{
    crypto::pasta_prelude::{Curve, CurveAffine},
    error::ContractError,
    msg,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::InsuranceMarketError;
use crate::model::{
    calculate_premium,
    derive_coverage_id,
    PurchaseCoverageParamsV1,
    PurchaseCoverageUpdateV1,
};
use crate::{
    INSURANCE_CONTRACT_COVERAGES_TREE, INSURANCE_CONTRACT_MARKETS_TREE,
    INSURANCE_CONTRACT_UNDERWRITERS_TREE,
};

/// Process instruction for PurchaseCoverageV1
pub fn insurance_market_purchase_coverage_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: PurchaseCoverageParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::purchase_coverage] Purchasing coverage");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  underwriter_id: {:?}", params.underwriter_id);
    msg!("  coverage_amount: {}", params.coverage_amount);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::InsuranceMarket = deserialize(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    // Verify market isn't closed
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if market.closes_at > 0 && current_block >= market.closes_at {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    // Check remaining coverage
    let remaining_coverage = market.total_coverage - market.coverage_sold;
    if params.coverage_amount > remaining_coverage {
        return Err(InsuranceMarketError::InsufficientCoverage.into())
    }

    // Check max coverage per buyer
    if params.coverage_amount > market.max_coverage_per_buyer {
        return Err(InsuranceMarketError::InvalidParameter("Exceeds max coverage per buyer".to_string()).into())
    }

    // Look up the underwriter
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &serialize(&params.underwriter_id))?.unwrap();
    let underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;

    if !underwriter.active {
        return Err(InsuranceMarketError::UnauthorizedUnderwriter.into())
    }

    // Check underwriter has sufficient coverage available
    if params.coverage_amount > underwriter.coverage_provided {
        return Err(InsuranceMarketError::InsufficientCoverage.into())
    }

    // Calculate premium
    let premium = calculate_premium(params.coverage_amount, market.premium_rate);

    // Verify value commitment matches premium (simplified - in production would verify signature)
    let vc_coords = params.value_commit.to_affine().coordinates();
    if vc_coords.is_none().into() {
        return Err(InsuranceMarketError::InvalidParameter("Invalid value commit".to_string()).into())
    }

    // Derive coverage ID
    let coverage_id = derive_coverage_id(
        params.market_id,
        &params.buyer,
        params.coverage_amount,
        current_block,
    );

    // Check if coverage already exists
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;
    if wasm::db::db_contains_key(coverages_db, &serialize(&coverage_id))? {
        return Err(InsuranceMarketError::CoverageAlreadyActive.into())
    }

    // Calculate coverage period
    let starts_at = current_block;
    let expires_at = starts_at + market.coverage_period;

    // Create the update
    let update = PurchaseCoverageUpdateV1 {
        coverage_id,
        market_id: params.market_id,
        underwriter_id: params.underwriter_id,
        buyer: params.buyer,
        amount: params.coverage_amount,
        premium_paid: premium,
        starts_at,
        expires_at,
    };

    msg!(
        "[insurance_market::purchase_coverage] Coverage purchased: {:?}, premium: {}",
        coverage_id,
        premium
    );
    Ok(serialize(&update))
}

/// Process update for PurchaseCoverageV1
pub fn insurance_market_purchase_coverage_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: PurchaseCoverageUpdateV1,
) -> Result<(), ContractError> {
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;

    // Create coverage state
    let coverage = crate::model::Coverage {
        id: update.coverage_id,
        market_id: update.market_id,
        buyer: update.buyer,
        underwriter_id: update.underwriter_id,
        amount: update.amount,
        premium_paid: update.premium_paid,
        state: crate::model::CoverageState::Active,
        starts_at: update.starts_at,
        expires_at: update.expires_at,
        claim_id: None,
    };

    // Store coverage
    wasm::db::db_set(
        coverages_db,
        &serialize(&update.coverage_id),
        &serialize(&coverage),
    )?;

    // Update underwriter's earned premiums
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &serialize(&update.underwriter_id))?.unwrap();
    let mut underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;
    underwriter.earned_premiums += update.premium_paid;
    wasm::db::db_set(
        underwriters_db,
        &serialize(&update.underwriter_id),
        &serialize(&underwriter),
    )?;

    msg!(
        "[insurance_market::purchase_coverage::update] Coverage stored: {:?}",
        update.coverage_id
    );
    Ok(())
}