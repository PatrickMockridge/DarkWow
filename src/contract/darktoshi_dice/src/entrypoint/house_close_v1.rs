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

//! HouseCloseV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES money_v3::transfer_v1 child calls to be bundled for
//! collecting the house's share of the bet.

use dwow_sdk::{
    error::ContractError,
    msg,
    wasm,
};
use dwow_serial::{deserialize, serialize};

use crate::error::DiceError;
use crate::model::{Bet, BetState, HouseCloseParamsV1, HouseCloseUpdateV1};
use crate::DICE_CONTRACT_BETS_TREE;
use crate::DICE_CONTRACT_HOUSE_TREE;
use crate::DICE_CONTRACT_INFO_TREE;
use crate::DICE_CONTRACT_ROLL_TIMEOUT;

/// Process instruction for HouseCloseV1
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for collecting the house's share.
pub fn dice_house_close_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: HouseCloseParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled for house's share
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[HouseCloseV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(DiceError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[HouseCloseV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DiceError::InvalidChildCall.into())
    }

    msg!("[dice::house_close] Processing house close");

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&params.bet_id))?.unwrap();
    let bet: Bet = deserialize(&bet_bytes)?;

    msg!("[dice::house_close] Found bet, current state: {:?}", bet.state as u8);

    // Get roll timeout from info
    let info_db = wasm::db::db_lookup(cid, DICE_CONTRACT_INFO_TREE)?;
    let timeout_bytes = wasm::db::db_get(info_db, DICE_CONTRACT_ROLL_TIMEOUT)?.unwrap();
    let roll_timeout: u32 = deserialize(&timeout_bytes)?;

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Check if timeout has been reached
    let blocks_since_creation = current_block.saturating_sub(bet.created_at as u32);
    let timeout_reached = blocks_since_creation >= roll_timeout;

    msg!(
        "[dice::house_close] Timeout: {}, Current: {}, Timeout reached: {}",
        roll_timeout,
        current_block,
        timeout_reached
    );

    // Bet can only be closed if Committed and timeout reached, or Revealed
    let can_close = match bet.state {
        BetState::Committed if timeout_reached => true,
        BetState::Revealed => true,
        _ => false,
    };

    if !can_close {
        if bet.state == BetState::Committed && !timeout_reached {
            return Err(DiceError::RollTimeoutNotReached.into())
        }
        return Err(DiceError::InvalidStateTransition.into())
    }

    // Create the update
    let update = HouseCloseUpdateV1 { bet_id: bet.id, state: BetState::Cancelled };

    msg!("[dice::house_close] House close approved");
    Ok(serialize(&update))
}

/// Process update for HouseCloseV1
pub fn dice_house_close_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: HouseCloseUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let house_db = wasm::db::db_lookup(cid, DICE_CONTRACT_HOUSE_TREE)?;

    // Look up and update the bet
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&update.bet_id))?.unwrap();
    let mut bet: Bet = deserialize(&bet_bytes)?;

    // Update bet state to Cancelled
    bet.state = update.state;
    wasm::db::db_set(bets_db, &serialize(&update.bet_id), &serialize(&bet))?;

    // House collects the bet value
    let house_take = bet.calculate_house_take().ok_or(DiceError::ArithmeticOverflow)?;
    let mut house_balance: u64 = 0;
    if wasm::db::db_contains_key(house_db, b"balance")? {
        let balance_bytes = wasm::db::db_get(house_db, b"balance")?.unwrap();
        house_balance = deserialize(&balance_bytes)?;
    }
    house_balance += house_take;
    wasm::db::db_set(house_db, b"balance", &serialize(&house_balance))?;

    msg!("[dice::house_close::update] House collected {}", house_take);
    Ok(())
}
