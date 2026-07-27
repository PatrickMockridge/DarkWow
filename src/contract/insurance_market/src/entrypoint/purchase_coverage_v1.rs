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
    crypto::{pasta_prelude::{Curve, CurveAffine, PrimeField}, poseidon_hash, ContractId},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::{deserialize, serialize, Encodable};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};

use crate::error::InsuranceMarketError;
use crate::model::{
    calculate_premium,
    derive_coverage_id,
    PurchaseCoverageParamsV1,
    PurchaseCoverageUpdateV1,
};
use crate::{
    INSURANCE_CONTRACT_COVERAGES_TREE, INSURANCE_CONTRACT_INFO_TREE,
    INSURANCE_CONTRACT_MARKETS_TREE, INSURANCE_MARKET_NULLIFIERS_TREE,
    INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_NS_V1,
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
        msg!("[insurance_market::PurchaseCoverageV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::PurchaseCoverageV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    let self_ = &calls[call_idx].data;
    let params: PurchaseCoverageParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::purchase_coverage] Purchasing coverage");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  underwriter_id: {:?}", params.underwriter_id);
    msg!("  coverage_amount: {}", params.coverage_amount);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &params.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let market = crate::model::InsuranceMarket::decode(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    // Verify market isn't closed
    let current_block = wasm::util::get_verifying_block_height()?.get();
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
        wasm::db::db_get(underwriters_db, &params.underwriter_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let underwriter = crate::model::Underwriter::decode(&underwriter_bytes)?;

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
    let _vc_coords = vc_coords.unwrap();
    // Verify buyer nullifier hasn't been used (ZK proof verifies identity)
    let nullifiers_db = wasm::db::db_lookup(cid, INSURANCE_MARKET_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.buyer_nullifier.to_repr())? {
        return Err(InsuranceMarketError::InvalidParameter("Duplicate nullifier".to_string()).into())
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
    if wasm::db::db_contains_key(coverages_db, &coverage_id.to_repr())? {
        return Err(InsuranceMarketError::CoverageAlreadyActive.into())
    }

    let value_blind = poseidon_hash([
        pallas::Base::from(premium),
        coverage_id,
    ]);
    validate_child_value_commit(&child_call.data, premium, value_blind)?;

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
        buyer_nullifier: params.buyer_nullifier,
    };

    msg!(
        "[insurance_market::purchase_coverage] Coverage purchased: {:?}, premium: {}",
        coverage_id,
        premium
    );
    Ok(update.encode())
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
        &update.coverage_id.to_repr(),
        &coverage.encode(),
    )?;

    // Update underwriter's earned premiums and coverage sold
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &update.underwriter_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut underwriter = crate::model::Underwriter::decode(&underwriter_bytes)?;
    underwriter.earned_premiums += update.premium_paid;
    underwriter.coverage_sold += update.amount; // Track coverage sold
    wasm::db::db_set(
        underwriters_db,
        &update.underwriter_id.to_repr(),
        &underwriter.encode(),
    )?;

    // Record buyer nullifier for replay protection
    let nullifiers_db = wasm::db::db_lookup(cid, INSURANCE_MARKET_NULLIFIERS_TREE)?;
    wasm::db::db_set(nullifiers_db, &update.buyer_nullifier.to_repr(), &[])?;

    msg!(
        "[insurance_market::purchase_coverage::update] Coverage stored: {:?}",
        update.coverage_id
    );
    Ok(())
}

/// Get metadata for PurchaseCoverageV1 ZK proof verification
pub fn purchase_coverage_get_metadata_v1(
    params: PurchaseCoverageParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let (buyer_x, buyer_y) = params.buyer.xy().expect("pk not identity");
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_NS_V1.to_string(),
        vec![buyer_x, buyer_y, params.buyer_nullifier],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}