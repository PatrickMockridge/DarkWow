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

//! PurchaseCoverageV1 Implementation

use dwow_sdk::{
    crypto::{pasta_prelude::{Curve, CurveAffine}, poseidon_hash, schnorr::SchnorrPublic},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::{deserialize, serialize};

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
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for premium transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[insurance_market::PurchaseCoverageV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::PurchaseCoverageV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }

    let self_ = &calls[call_idx].data;
    let params: PurchaseCoverageParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::purchase_coverage] Purchasing coverage");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  underwriter_id: {:?}", params.underwriter_id);
    msg!("  coverage_amount: {}", params.coverage_amount);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.ok_or(ContractError::DbGetEmpty)?;
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
    let remaining_coverage = market.total_coverage.saturating_sub(market.coverage_sold);
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
        wasm::db::db_get(underwriters_db, &serialize(&params.underwriter_id))?.ok_or(ContractError::DbGetEmpty)?;
    let underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;

    if !underwriter.active {
        return Err(InsuranceMarketError::UnauthorizedUnderwriter.into())
    }

    // Check underwriter has sufficient coverage available
    let available_coverage = underwriter.coverage_provided - underwriter.coverage_sold;
    if params.coverage_amount > available_coverage {
        return Err(InsuranceMarketError::InsufficientCoverage.into())
    }

    // Calculate premium
    let premium = calculate_premium(params.coverage_amount, market.premium_rate)?;

    // Verify buyer signature binding value_commit and premium
    let vc_coords = params.value_commit.to_affine().coordinates();
    if vc_coords.is_none().into() {
        return Err(InsuranceMarketError::InvalidParameter("Invalid value commit".to_string()).into())
    }
    let vc_coords = vc_coords.unwrap();
    let signature_msg = serialize(&poseidon_hash([
        params.buyer.x(),
        params.buyer.y(),
        *vc_coords.x(),
        *vc_coords.y(),
        pallas::Base::from(premium),
    ]));
    if !params.buyer.verify(&signature_msg, &params.signature) {
        msg!("[insurance_market::PurchaseCoverageV1] Error: Invalid buyer signature");
        return Err(InsuranceMarketError::InvalidParameter("Invalid signature".to_string()).into())
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
    cid: dwow_sdk::crypto::ContractId,
    update: PurchaseCoverageUpdateV1,
) -> Result<(), ContractError> {
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;

    // Create coverage state
    let coverage = crate::model::Coverage {
        version: 1,
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

    // Update underwriter's earned premiums and coverage sold
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &serialize(&update.underwriter_id))?.ok_or(ContractError::DbGetEmpty)?;
    let mut underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;
    underwriter.earned_premiums += update.premium_paid;
    underwriter.coverage_sold += update.amount; // Track coverage sold
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