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

//! PurchaseCoverageWithCapabilityV1 Implementation
//!
//! Allows purchasing coverage with an O-Cap capability token for authorization.

use dwow_sdk::{
    crypto::{pasta_prelude::{Curve, CurveAffine, PrimeField}, poseidon_hash, ContractId},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::deserialize;
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};

use crate::error::InsuranceMarketError;
use crate::InsuranceMarketFunction;
use crate::model::{
    calculate_premium,
    derive_coverage_id,
    PurchaseCoverageWithCapabilityParamsV1,
    PurchaseCoverageWithCapabilityUpdateV1,
};
use crate::{
    INSURANCE_CONTRACT_COVERAGES_TREE, INSURANCE_CONTRACT_IDENTITY_CONTRACT_ID,
    INSURANCE_CONTRACT_INFO_TREE, INSURANCE_CONTRACT_MARKETS_TREE,
    INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, INSURANCE_CONTRACT_UNDERWRITERS_TREE,
    INSURANCE_MARKET_NULLIFIERS_TREE,
};

/// Process instruction for PurchaseCoverageWithCapabilityV1
pub fn insurance_market_purchase_coverage_with_capability_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    // Validate child calls: (1) PN::TransferV1 to pay the premium, (2) Identity::VerifyCapabilityV1
    // to prove the caller holds the capability the market requires.
    //
    // The same two holes as `underwrite_with_capability`: this path required neither child call
    // while its non-capability sibling (`purchase_coverage.rs`) requires child 0, and its only
    // capability check was the market-side `is_none()` presence test while the published
    // `required_capability_id` came from `params.capability_secret` — the caller's own input,
    // compared to nothing (register OBL-Z16). Child 0 is a port of `purchase_coverage.rs:62-86`,
    // child 1 of `labor_market`'s `accept_job_with_capability_v1`.
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 2 {
        msg!("[insurance_market::purchase_coverage_with_cap] Error: Expected 2 child calls (PN::transfer_v1 + Identity::VerifyCapabilityV1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }

    // Child 0: PN transfer — moves the premium
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::purchase_coverage_with_cap] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }
    let info_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Child 1: Identity::VerifyCapabilityV1
    //
    // As in `underwrite_with_capability`, this checks the child's shape — present, selector 0x06,
    // addressed to the configured Identity contract. The second half, that the capability the child
    // verified is the one this market requires, is done below once the market record is read.
    let identity_idx = this_call.children_indexes[1];
    let identity_call = &calls[identity_idx].data;
    if identity_call.data[0] != 0x06 {
        msg!("[insurance_market::purchase_coverage_with_cap] Error: Expected Identity::VerifyCapabilityV1 (0x06), got 0x{:02x}", identity_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }
    let identity_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_IDENTITY_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let identity_cid: ContractId = deserialize(&identity_bytes)?;
    if identity_cid == ContractId::ZERO {
        return Err(ContractError::IoError("identity contract ID not configured".into()));
    }
    validate_child_contract_id(&identity_call.contract_id, &identity_cid)?;

    let self_ = &calls[call_idx].data;
    let params = PurchaseCoverageWithCapabilityParamsV1::decode(&self_.data[1..])?;

    msg!("[insurance_market::purchase_coverage_with_cap] Purchasing coverage with capability");
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

    // Verify market requires a capability for buying coverage
    if market.required_buyer_capability.is_none() {
        return Err(InsuranceMarketError::CapabilityNotMet.into())
    }

    #[expect(clippy::unwrap_used, reason = "guarded by is_none() check above")]
    let required_capability_id = market.required_buyer_capability.unwrap();

    // The child must have verified *this* capability, not merely some capability — see the longer
    // note in `underwrite_with_capability.rs`. Identity reads `capability_id` from its own params and
    // never learns what the market requires, so a shape-only guard admits any valid capability.
    let identity_params = dwow_identity_contract::model::VerifyCapabilityParams::decode(
        &identity_call.data[1..],
    )?;
    if identity_params.capability_proof.capability_id.to_bytes() != required_capability_id {
        msg!("[insurance_market::purchase_coverage_with_cap] Error: child verified capability {:?}, market requires {:?}",
             identity_params.capability_proof.capability_id.to_bytes(), required_capability_id);
        return Err(InsuranceMarketError::CapabilityNotMet.into())
    }

    // ZK proof verified by host via get_metadata
    // (namespace: INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_WITH_CAPABILITY_NS_V1)

    // Verify market isn't closed
    let current_block = wasm::util::get_verifying_block_height()?.get();
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
    // Verify buyer nullifier hasn't been used (ZK proof verifies identity + capability)
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

    // The child call must move *this* premium, not merely exist. `purchase_coverage.rs:167-169` is
    // the source of the blind and of the comparison; without it a caller attaches a one-unit
    // transfer while the coverage below is priced from the declared `coverage_amount`.
    let value_blind = poseidon_hash([
        pallas::Base::from(premium),
        coverage_id,
    ]);
    validate_child_value_commit(&child_call.data, premium, value_blind)?;

    // Check if coverage already exists
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;
    if wasm::db::db_contains_key(coverages_db, &coverage_id.to_repr())? {
        return Err(InsuranceMarketError::CoverageAlreadyActive.into())
    }

    // Calculate coverage period
    let starts_at = current_block;
    let expires_at = starts_at + market.coverage_period;

    // Create the update
    // Apply the underwriter's increments here — apply used to do both after re-reading the record
    // (register OBL-C72).
    let mut underwriter = underwriter;
    underwriter.earned_premiums += premium;
    underwriter.coverage_sold += params.coverage_amount;

    let update = PurchaseCoverageWithCapabilityUpdateV1 {
        coverage_id,
        market_id: params.market_id,
        underwriter_id: params.underwriter_id,
        buyer: params.buyer,
        amount: params.coverage_amount,
        premium_paid: premium,
        starts_at,
        expires_at,
        required_capability_id,
        buyer_nullifier: params.buyer_nullifier,
        underwriter_bytes: underwriter.encode(),
    };

    msg!(
        "[insurance_market::purchase_coverage_with_cap] Coverage purchased: {:?}, premium: {}, required_cap: {:?}",
        coverage_id,
        premium,
        required_capability_id
    );
    Ok([&[InsuranceMarketFunction::PurchaseCoverageWithCapabilityV1 as u8],
        &update.encode()?[..]].concat())
}

/// Process update for PurchaseCoverageWithCapabilityV1
pub fn insurance_market_purchase_coverage_with_capability_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: PurchaseCoverageWithCapabilityUpdateV1,
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

    // Blind write — exec applied the increments and carried the record (register OBL-C72).
    wasm::db::db_set(
        underwriters_db,
        &update.underwriter_id.to_repr(),
        &update.underwriter_bytes,
    )?;

    // Record buyer nullifier for replay protection
    let nullifiers_db = wasm::db::db_lookup(cid, INSURANCE_MARKET_NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.buyer_nullifier.to_repr())?;

    msg!(
        "[insurance_market::purchase_coverage_with_cap::update] Coverage stored: {:?}, Required Cap: {:?}",
        update.coverage_id,
        update.required_capability_id
    );
    Ok(())
}