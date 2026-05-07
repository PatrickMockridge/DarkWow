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

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};

use crate::error::InsuranceMarketError;
use crate::model::{
    calculate_max_coverage,
    derive_underwriter_id,
    UnderwriteParamsV1,
    UnderwriteUpdateV1,
};
use crate::{INSURANCE_CONTRACT_MARKETS_TREE, INSURANCE_CONTRACT_RISK_TYPES_TREE, INSURANCE_CONTRACT_UNDERWRITERS_TREE};

/// Process instruction for UnderwriteV1
pub fn insurance_market_underwrite_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for bond transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[insurance_market::UnderwriteV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::UnderwriteV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }

    let self_ = &calls[call_idx].data;
    let params: UnderwriteParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::underwrite] Registering as underwriter");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  bond_amount: {}", params.bond_amount);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: crate::model::InsuranceMarket = deserialize(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    // Look up risk type to get min bond rate
    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    let risk_type_bytes =
        wasm::db::db_get(risk_types_db, &serialize(&market.risk_type))?.unwrap();
    let risk_type: crate::model::RiskType = deserialize(&risk_type_bytes)?;

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

    // Check if underwriter already exists
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    if wasm::db::db_contains_key(underwriters_db, &serialize(&underwriter_id))? {
        // Update existing underwriter's bond
        msg!("[insurance_market::underwrite] Updating existing underwriter");
    }

    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Create the update
    let update = UnderwriteUpdateV1 {
        underwriter_id,
        market_id: params.market_id,
        owner: params.underwriter,
        bond_amount: params.bond_amount,
        coverage_provided: params.coverage_limit,
        created_at: current_block,
    };

    msg!("[insurance_market::underwrite] Underwriter registered: {:?}", underwriter_id);
    Ok(serialize(&update))
}

/// Process update for UnderwriteV1
pub fn insurance_market_underwrite_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: UnderwriteUpdateV1,
) -> Result<(), ContractError> {
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;

    // Check if underwriter exists (update case)
    let existing: Option<crate::model::Underwriter> =
        if wasm::db::db_contains_key(underwriters_db, &serialize(&update.underwriter_id))? {
            let bytes =
                wasm::db::db_get(underwriters_db, &serialize(&update.underwriter_id))?.unwrap();
            Some(deserialize(&bytes)?)
        } else {
            None
        };

    if let Some(mut underwriter) = existing {
        // Update existing underwriter
        underwriter.bond_amount += update.bond_amount;
        underwriter.coverage_provided += update.coverage_provided;
        wasm::db::db_set(
            underwriters_db,
            &serialize(&update.underwriter_id),
            &serialize(&underwriter),
        )?;
    } else {
        // Create new underwriter
        let underwriter = crate::model::Underwriter {
            id: update.underwriter_id,
            owner: update.owner,
            market_id: update.market_id,
            bond_amount: update.bond_amount,
            coverage_provided: update.coverage_provided,
            coverage_sold: 0,
            earned_premiums: 0,
            claims_paid: 0,
            slash_count: 0,
            performance_score: 10000, // Start at perfect score
            active: true,
            created_at: update.created_at,
        };

        wasm::db::db_set(
            underwriters_db,
            &serialize(&update.underwriter_id),
            &serialize(&underwriter),
        )?;
    }

    // Update market coverage sold
    let market_bytes =
        wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: crate::model::InsuranceMarket = deserialize(&market_bytes)?;
    market.coverage_sold += update.coverage_provided;
    wasm::db::db_set(
        markets_db,
        &serialize(&update.market_id),
        &serialize(&market),
    )?;

    msg!(
        "[insurance_market::underwrite::update] Underwriter: {:?}, Coverage: {}",
        update.underwriter_id,
        update.coverage_provided
    );
    Ok(())
}