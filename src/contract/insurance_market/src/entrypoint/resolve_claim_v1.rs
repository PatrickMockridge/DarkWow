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

//! ResolveClaimV1 Implementation

use dwow_sdk::{crypto::ContractId, error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};
use dwow_promissory_note_contract::validation::validate_child_contract_id;

use crate::error::InsuranceMarketError;
use crate::model::{
    calculate_slash,
    ResolveClaimParamsV1,
    ResolveClaimUpdateV1,
};
use crate::{
    INSURANCE_CONTRACT_CLAIMS_TREE, INSURANCE_CONTRACT_COVERAGES_TREE,
    INSURANCE_CONTRACT_INFO_TREE, INSURANCE_CONTRACT_MARKETS_TREE,
    INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, INSURANCE_CONTRACT_UNDERWRITERS_TREE,
};

/// Process instruction for ResolveClaimV1
pub fn insurance_market_resolve_claim_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for claim payout
    if this_call.children_indexes.len() != 1 {
        msg!("[insurance_market::ResolveClaimV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::ResolveClaimV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::from_bytes([0u8; 32]).unwrap() {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    let self_ = &calls[call_idx].data;
    let params: ResolveClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::resolve_claim] Resolving claim");
    msg!("  claim_id: {:?}", params.claim_id);
    msg!("  is_valid: {}", params.is_valid);

    // Look up the claim
    let claims_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_CLAIMS_TREE)?;
    let claim_bytes = wasm::db::db_get(claims_db, &serialize(&params.claim_id))?.ok_or(ContractError::DbGetEmpty)?;
    let claim: crate::model::Claim = deserialize(&claim_bytes)?;

    if claim.state != crate::model::ClaimState::Filed {
        return Err(InsuranceMarketError::ClaimAlreadyResolved.into())
    }

    // Look up the coverage
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;
    let coverage_bytes =
        wasm::db::db_get(coverages_db, &serialize(&claim.coverage_id))?.ok_or(ContractError::DbGetEmpty)?;
    let coverage: crate::model::Coverage = deserialize(&coverage_bytes)?;

    // Look up the market to get deductible
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes =
        wasm::db::db_get(markets_db, &serialize(&coverage.market_id))?.ok_or(ContractError::DbGetEmpty)?;
    let market: crate::model::InsuranceMarket = deserialize(&market_bytes)?;

    // Look up the underwriter
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &serialize(&coverage.underwriter_id))?.ok_or(ContractError::DbGetEmpty)?;
    let underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;

    // Calculate payout (coverage amount minus deductible)
    let payout = if params.is_valid {
        params.payout_amount.saturating_sub(market.deductible).min(coverage.amount)
    } else {
        0
    };

    // Calculate slash amount for underwriter if claim was valid
    let slash_amount = if params.is_valid {
        calculate_slash(
            payout,
            coverage.amount,
            underwriter.bond_amount,
            underwriter.performance_score,
        )?
    } else {
        0
    };

    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Create the update
    let update = ResolveClaimUpdateV1 {
        claim_id: params.claim_id,
        coverage_id: claim.coverage_id,
        is_valid: params.is_valid,
        payout_amount: payout,
        slash_amount,
        resolved_at: current_block,
        oracle_signature: params.oracle_signature,
    };

    msg!(
        "[insurance_market::resolve_claim] Claim resolved: {:?}, payout: {}, slash: {}",
        params.claim_id,
        payout,
        slash_amount
    );
    Ok(serialize(&update))
}

/// Process update for ResolveClaimV1
pub fn insurance_market_resolve_claim_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: ResolveClaimUpdateV1,
) -> Result<(), ContractError> {
    let claims_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_CLAIMS_TREE)?;
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;

    // Update claim
    let claim_bytes =
        wasm::db::db_get(claims_db, &serialize(&update.claim_id))?.ok_or(ContractError::DbGetEmpty)?;
    let mut claim: crate::model::Claim = deserialize(&claim_bytes)?;
    claim.payout = update.payout_amount;
    claim.state = if update.is_valid {
        crate::model::ClaimState::Paid
    } else {
        crate::model::ClaimState::Rejected
    };
    claim.attestation = vec![]; // Would be stored from params
    claim.oracle_signature = update.oracle_signature;
    claim.resolved_at = update.resolved_at;
    wasm::db::db_set(
        claims_db,
        &serialize(&update.claim_id),
        &serialize(&claim),
    )?;

    // Update coverage state
    let coverage_bytes =
        wasm::db::db_get(coverages_db, &serialize(&update.coverage_id))?.ok_or(ContractError::DbGetEmpty)?;
    let mut coverage: crate::model::Coverage = deserialize(&coverage_bytes)?;
    coverage.state = if update.is_valid {
        crate::model::CoverageState::Claimed
    } else {
        crate::model::CoverageState::Active // Can file again
    };
    wasm::db::db_set(
        coverages_db,
        &serialize(&update.coverage_id),
        &serialize(&coverage),
    )?;

    // Update underwriter if slash occurred
    if update.slash_amount > 0 {
        let underwriter_bytes =
            wasm::db::db_get(underwriters_db, &serialize(&coverage.underwriter_id))?.ok_or(ContractError::DbGetEmpty)?;
        let mut underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;

        // Slash the bond
        underwriter.bond_amount = underwriter.bond_amount.saturating_sub(update.slash_amount);
        underwriter.claims_paid += update.payout_amount;
        underwriter.slash_count += 1;

        // Update performance score (penalize for claims)
        let new_score = underwriter.performance_score.saturating_sub(100);
        underwriter.performance_score = new_score;

        wasm::db::db_set(
            underwriters_db,
            &serialize(&coverage.underwriter_id),
            &serialize(&underwriter),
        )?;

        msg!(
            "[insurance_market::resolve_claim::update] Underwriter slashed: {:?}, new bond: {}",
            coverage.underwriter_id,
            underwriter.bond_amount
        );
    }

    msg!(
        "[insurance_market::resolve_claim::update] Claim resolved: {:?}",
        update.claim_id
    );
    Ok(())
}