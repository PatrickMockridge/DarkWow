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

//! UnderwriteV1 Implementation

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::deserialize;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};

use crate::error::InsuranceMarketError;
use crate::InsuranceMarketFunction;
use crate::model::{
    calculate_max_coverage,
    derive_underwriter_id,
    UnderwriteParamsV1,
    UnderwriteUpdateV1,
};
use crate::{
    INSURANCE_CONTRACT_INFO_TREE, INSURANCE_CONTRACT_MARKETS_TREE,
    INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, INSURANCE_CONTRACT_RISK_TYPES_TREE,
    INSURANCE_CONTRACT_UNDERWRITERS_TREE,
};

/// Process instruction for UnderwriteV1
pub fn insurance_market_underwrite_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for bond transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[insurance_market::UnderwriteV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::UnderwriteV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params = UnderwriteParamsV1::decode(&self_.data[1..])?;

    msg!("[insurance_market::underwrite] Registering as underwriter");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  bond_amount: {}", params.bond_amount);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &params.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let market = crate::model::InsuranceMarket::decode(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

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
        msg!("[insurance_market::underwrite] Updating existing underwriter");
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

    let value_blind = poseidon_hash([
        pallas::Base::from(params.bond_amount),
        underwriter_id,
    ]);
    validate_child_value_commit(&child_call.data, params.bond_amount, value_blind)?;

    // Create the update
    let update = UnderwriteUpdateV1 {
        underwriter_id,
        market_id: params.market_id,
        owner: params.underwriter,
        bond_amount: params.bond_amount,
        coverage_provided: params.coverage_limit,
        created_at: current_block,
        underwriter_bytes: underwriter.encode(),
        market_bytes: market.encode(),
    };

    msg!("[insurance_market::underwrite] Underwriter registered: {:?}", underwriter_id);
    Ok([&[InsuranceMarketFunction::UnderwriteV1 as u8], &update.encode()?[..]].concat())
}

/// Process update for UnderwriteV1
pub fn insurance_market_underwrite_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: UnderwriteUpdateV1,
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
        "[insurance_market::underwrite::update] Underwriter: {:?}, Coverage: {}",
        update.underwriter_id,
        update.coverage_provided
    );
    Ok(())
}