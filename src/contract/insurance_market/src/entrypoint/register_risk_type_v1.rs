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

//! RegisterRiskTypeV1 Implementation

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

use crate::error::InsuranceMarketError;
use crate::model::{derive_risk_type_id, RegisterRiskTypeParamsV1, RegisterRiskTypeUpdateV1};
use crate::INSURANCE_CONTRACT_RISK_TYPES_TREE;

/// Process instruction for RegisterRiskTypeV1
pub fn insurance_market_register_risk_type_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: RegisterRiskTypeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::register_risk_type] Registering new risk type");
    msg!("  category: {:?}", params.category);

    // Derive risk type ID
    let risk_type_id = derive_risk_type_id(
        params.category,
        &params.description,
        &params.oracle_pubkey,
    );

    // Check if risk type already exists
    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    if wasm::db::db_contains_key(risk_types_db, &risk_type_id.to_repr())? {
        return Err(InsuranceMarketError::RiskTypeAlreadyExists.into())
    }

    // Validate premium rate (0-10000 basis points = 0-100%)
    if params.base_premium_rate > 10000 {
        return Err(InsuranceMarketError::InvalidPremiumRate.into())
    }

    // Validate bond rate
    if params.min_bond_rate < 100 || params.min_bond_rate > 10000 {
        // Minimum 1% bond, maximum 100%
        return Err(InsuranceMarketError::InvalidParameter("Invalid bond rate".to_string()).into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create the update
    let update = RegisterRiskTypeUpdateV1 {
        risk_type_id,
        category: params.category,
        description: params.description.clone(),
        base_premium_rate: params.base_premium_rate,
        min_bond_rate: params.min_bond_rate,
        oracle_pubkey: params.oracle_pubkey,
        created_at: current_block,
    };

    msg!("[insurance_market::register_risk_type] Risk type registered: {:?}", update.risk_type_id);
    Ok(update.encode())
}

/// Process update for RegisterRiskTypeV1
pub fn insurance_market_register_risk_type_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: RegisterRiskTypeUpdateV1,
) -> Result<(), ContractError> {
    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;

    // Create risk type state
    let risk_type = crate::model::RiskType {
        version: 1,
        id: update.risk_type_id,
        category: update.category,
        description: update.description.clone(),
        base_premium_rate: update.base_premium_rate,
        min_bond_rate: update.min_bond_rate,
        oracle_pubkey: update.oracle_pubkey,
        active: true,
        created_at: update.created_at,
    };

    // Store risk type
    wasm::db::db_set(
        risk_types_db,
        &update.risk_type_id.to_repr(),
        &risk_type.encode(),
    )?;

    msg!("[insurance_market::register_risk_type::update] Risk type stored: {:?}", update.risk_type_id);
    Ok(())
}