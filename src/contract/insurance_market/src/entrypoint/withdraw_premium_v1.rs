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

//! WithdrawPremiumV1 Implementation

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};

use crate::error::InsuranceMarketError;
use crate::model::{WithdrawPremiumParamsV1, WithdrawPremiumUpdateV1};
use crate::{
    INSURANCE_CONTRACT_INFO_TREE, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    INSURANCE_CONTRACT_UNDERWRITERS_TREE,
};

/// Process instruction for WithdrawPremiumV1
pub fn insurance_market_withdraw_premium_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for premium withdrawal
    if this_call.children_indexes.len() != 1 {
        msg!("[insurance_market::WithdrawPremiumV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::WithdrawPremiumV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    let self_ = &calls[call_idx].data;
    let params = WithdrawPremiumParamsV1::decode(&self_.data[1..])?;

    msg!("[insurance_market::withdraw_premium] Withdrawing premium");
    msg!("  underwriter_id: {:?}", params.underwriter_id);
    msg!("  amount: {}", params.amount);

    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        params.underwriter_id,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    // Look up the underwriter
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &params.underwriter_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let underwriter = crate::model::Underwriter::decode(&underwriter_bytes)?;

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
    Ok(update.encode())
}

/// Process update for WithdrawPremiumV1
pub fn insurance_market_withdraw_premium_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: WithdrawPremiumUpdateV1,
) -> Result<(), ContractError> {
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;

    // Update underwriter's earned premiums
    let underwriter_bytes =
        wasm::db::db_get(underwriters_db, &update.underwriter_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut underwriter = crate::model::Underwriter::decode(&underwriter_bytes)?;
    underwriter.earned_premiums = update.remaining_balance;
    wasm::db::db_set(
        underwriters_db,
        &update.underwriter_id.to_repr(),
        &underwriter.encode(),
    )?;

    // In production: trigger Money::TokenMint to transfer the premium to underwriter

    msg!(
        "[insurance_market::withdraw_premium::update] Premium withdrawn: {:?}, new balance: {}",
        update.underwriter_id,
        update.remaining_balance
    );
    Ok(())
}