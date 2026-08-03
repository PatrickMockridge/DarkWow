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

//! CloseMarketV1 Implementation

use dwow_sdk::{error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

use crate::error::InsuranceMarketError;
use crate::model::{CloseMarketParamsV1, CloseMarketUpdateV1};
use crate::INSURANCE_CONTRACT_MARKETS_TREE;

/// Process instruction for CloseMarketV1
pub fn insurance_market_close_market_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = CloseMarketParamsV1::decode(&self_.data[1..])?;

    msg!("[insurance_market::close_market] Closing market {:?}", params.market_id);

    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes =
        wasm::db::db_get(markets_db, &params.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let market: crate::model::InsuranceMarket = crate::model::InsuranceMarket::decode(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    let update = CloseMarketUpdateV1 {
        market_id: params.market_id,
    };

    msg!("[insurance_market::close_market] Market closed: {:?}", params.market_id);
    Ok(update.encode())
}

/// Process update for CloseMarketV1
pub fn insurance_market_close_market_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: CloseMarketUpdateV1,
) -> Result<(), ContractError> {
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;

    let market_bytes =
        wasm::db::db_get(markets_db, &update.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut market = crate::model::InsuranceMarket::decode(&market_bytes)?;
    market.active = false;
    wasm::db::db_set(
        markets_db,
        &update.market_id.to_repr(),
        &market.encode(),
    )?;

    msg!("[insurance_market::close_market::update] Market {:?} closed", update.market_id);
    Ok(())
}
