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

//! RetireRiskTypeV1 Implementation

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};

use crate::error::InsuranceMarketError;
use crate::model::{RetireRiskTypeParamsV1, RetireRiskTypeUpdateV1};
use crate::INSURANCE_CONTRACT_RISK_TYPES_TREE;

/// Process instruction for RetireRiskTypeV1
pub fn insurance_market_retire_risk_type_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: RetireRiskTypeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::retire_risk_type] Retiring risk type {:?}", params.risk_type_id);

    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    let risk_type_bytes =
        wasm::db::db_get(risk_types_db, &params.risk_type_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let risk_type = crate::model::RiskType::decode(&risk_type_bytes)?;

    if !risk_type.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    let update = RetireRiskTypeUpdateV1 {
        risk_type_id: params.risk_type_id,
    };

    msg!("[insurance_market::retire_risk_type] Risk type retired: {:?}", params.risk_type_id);
    Ok(update.encode())
}

/// Process update for RetireRiskTypeV1
pub fn insurance_market_retire_risk_type_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: RetireRiskTypeUpdateV1,
) -> Result<(), ContractError> {
    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;

    let risk_type_bytes =
        wasm::db::db_get(risk_types_db, &update.risk_type_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut risk_type = crate::model::RiskType::decode(&risk_type_bytes)?;
    risk_type.active = false;
    wasm::db::db_set(
        risk_types_db,
        &update.risk_type_id.to_repr(),
        &risk_type.encode(),
    )?;

    msg!("[insurance_market::retire_risk_type::update] Risk type {:?} retired", update.risk_type_id);
    Ok(())
}
