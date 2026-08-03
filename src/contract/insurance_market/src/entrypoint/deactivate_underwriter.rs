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

//! DeactivateUnderwriterV1 Implementation

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

use crate::error::InsuranceMarketError;
use crate::model::{DeactivateUnderwriterParamsV1, DeactivateUnderwriterUpdateV1};
use crate::INSURANCE_CONTRACT_UNDERWRITERS_TREE;

/// Process instruction for DeactivateUnderwriterV1
pub fn insurance_market_deactivate_underwriter_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = DeactivateUnderwriterParamsV1::decode(&self_.data[1..])?;

    msg!("[insurance_market::deactivate_underwriter] Deactivating underwriter {:?}", params.underwriter_id);

    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &params.underwriter_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let underwriter = crate::model::Underwriter::decode(&underwriter_bytes)?;

    if underwriter.owner != params.owner {
        return Err(InsuranceMarketError::UnauthorizedUnderwriter.into())
    }

    if !underwriter.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    let update = DeactivateUnderwriterUpdateV1 {
        underwriter_id: params.underwriter_id,
    };

    msg!("[insurance_market::deactivate_underwriter] Underwriter deactivated: {:?}", params.underwriter_id);
    Ok(update.encode())
}

/// Process update for DeactivateUnderwriterV1
pub fn insurance_market_deactivate_underwriter_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: DeactivateUnderwriterUpdateV1,
) -> Result<(), ContractError> {
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;

    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &update.underwriter_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut underwriter = crate::model::Underwriter::decode(&underwriter_bytes)?;
    underwriter.active = false;
    wasm::db::db_set(
        underwriters_db,
        &update.underwriter_id.to_repr(),
        &underwriter.encode(),
    )?;

    msg!("[insurance_market::deactivate_underwriter::update] Underwriter {:?} deactivated", update.underwriter_id);
    Ok(())
}
