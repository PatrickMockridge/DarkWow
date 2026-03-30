/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! WithdrawPremiumV1 Implementation

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::InsuranceMarketError;
use crate::model::{WithdrawPremiumParamsV1, WithdrawPremiumUpdateV1};
use crate::INSURANCE_CONTRACT_UNDERWRITERS_TREE;

/// Process instruction for WithdrawPremiumV1
pub fn insurance_market_withdraw_premium_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: WithdrawPremiumParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_market::withdraw_premium] Withdrawing premium");
    msg!("  underwriter_id: {:?}", params.underwriter_id);
    msg!("  amount: {}", params.amount);

    // Look up the underwriter
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &serialize(&params.underwriter_id))?.unwrap();
    let underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;

    // Verify the caller is the underwriter owner (access control)
    if underwriter.owner != params.owner {
        return Err(InsuranceMarketError::UnauthorizedUnderwriter.into())
    }

    // Verify underwriter is active
    if !underwriter.active {
        return Err(InsuranceMarketError::UnauthorizedUnderwriter.into())
    }

    // Verify sufficient earned premiums
    if params.amount > underwriter.earned_premiums {
        return Err(InsuranceMarketError::InsufficientPremium.into())
    }

    let remaining_balance = underwriter.earned_premiums - params.amount;

    // Create the update
    let update = WithdrawPremiumUpdateV1 {
        underwriter_id: params.underwriter_id,
        amount: params.amount,
        remaining_balance,
    };

    msg!(
        "[insurance_market::withdraw_premium] Withdrawal: {} (remaining: {})",
        params.amount,
        remaining_balance
    );
    Ok(serialize(&update))
}

/// Process update for WithdrawPremiumV1
pub fn insurance_market_withdraw_premium_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: WithdrawPremiumUpdateV1,
) -> Result<(), ContractError> {
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;

    // Update underwriter's earned premiums
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &serialize(&update.underwriter_id))?.unwrap();
    let mut underwriter: crate::model::Underwriter = deserialize(&underwriter_bytes)?;
    underwriter.earned_premiums = update.remaining_balance;
    wasm::db::db_set(
        underwriters_db,
        &serialize(&update.underwriter_id),
        &serialize(&underwriter),
    )?;

    // In production: trigger Money::TokenMint to transfer the premium to underwriter

    msg!(
        "[insurance_market::withdraw_premium::update] Premium withdrawn: {:?}, new balance: {}",
        update.underwriter_id,
        update.remaining_balance
    );
    Ok(())
}