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

//! FileClaimV1 Implementation

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

use crate::error::InsuranceMarketError;
use crate::model::{derive_claim_id, FileClaimParamsV1, FileClaimUpdateV1};
use crate::{INSURANCE_CONTRACT_CLAIMS_TREE, INSURANCE_CONTRACT_COVERAGES_TREE};

/// Process instruction for FileClaimV1
pub fn insurance_market_file_claim_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: FileClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::file_claim] Filing claim");
    msg!("  coverage_id: {:?}", params.coverage_id);
    msg!("  amount: {}", params.amount);

    // Look up the coverage
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;
    let coverage_bytes =
        wasm::db::db_get(coverages_db, &params.coverage_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let coverage = crate::model::Coverage::decode(&coverage_bytes)?;

    // Verify coverage is active
    if coverage.state != crate::model::CoverageState::Active {
        return Err(InsuranceMarketError::CoverageNotFound.into())
    }

    // Verify coverage hasn't expired
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block >= coverage.expires_at {
        return Err(InsuranceMarketError::CoverageExpired.into())
    }

    // Verify the caller is the coverage buyer (access control)
    if coverage.buyer != params.buyer {
        return Err(InsuranceMarketError::CoverageNotFound.into())
    }

    // Verify claim amount doesn't exceed coverage
    if params.amount > coverage.amount {
        return Err(InsuranceMarketError::ClaimNotCovered.into())
    }

    // Verify market_id matches
    if coverage.market_id != params.market_id {
        return Err(InsuranceMarketError::MarketNotFound.into())
    }

    // Derive claim ID
    // Use first 8 bytes of evidence as u64 for hash input
    let mut bytes = [0u8; 8];
    let e_len = params.evidence.len().min(8);
    bytes[..e_len].copy_from_slice(&params.evidence[..e_len]);
    let evidence_hash = dwow_sdk::pasta::pallas::Base::from(u64::from_le_bytes(bytes));
    let claim_id = derive_claim_id(params.coverage_id, evidence_hash, current_block);

    // Check if claim already exists
    let claims_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_CLAIMS_TREE)?;
    if wasm::db::db_contains_key(claims_db, &claim_id.to_repr())? {
        return Err(InsuranceMarketError::ClaimAlreadyResolved.into())
    }

    // Create the update
    let update = FileClaimUpdateV1 {
        claim_id,
        coverage_id: params.coverage_id,
        market_id: params.market_id,
        amount: params.amount,
        state: crate::model::ClaimState::Filed,
        created_at: current_block,
        oracle_signature: params.oracle_signature,
    };

    msg!("[insurance_market::file_claim] Claim filed: {:?}", claim_id);
    Ok(update.encode())
}

/// Process update for FileClaimV1
pub fn insurance_market_file_claim_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: FileClaimUpdateV1,
) -> Result<(), ContractError> {
    let claims_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_CLAIMS_TREE)?;
    let coverages_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_COVERAGES_TREE)?;

    // Create claim state
    let claim = crate::model::Claim {
        version: 1,
        id: update.claim_id,
        coverage_id: update.coverage_id,
        market_id: update.market_id,
        amount: update.amount,
        payout: 0, // Calculated at resolution
        state: crate::model::ClaimState::Filed,
        evidence: vec![], // Stored separately or in metadata
        attestation: vec![],
        oracle_signature: update.oracle_signature,
        resolved_at: 0,
    };

    // Store claim
    wasm::db::db_set(
        claims_db,
        &update.claim_id.to_repr(),
        &claim.encode(),
    )?;

    // Update coverage to mark claim in progress
    let coverage_bytes =
        wasm::db::db_get(coverages_db, &update.coverage_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut coverage = crate::model::Coverage::decode(&coverage_bytes)?;
    coverage.claim_id = Some(update.claim_id);
    wasm::db::db_set(
        coverages_db,
        &update.coverage_id.to_repr(),
        &coverage.encode(),
    )?;

    msg!(
        "[insurance_market::file_claim::update] Claim stored: {:?}",
        update.claim_id
    );
    Ok(())
}