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

//! SettleBetV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
//! paying out winnings to the player.

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::{deserialize, serialize};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};

use crate::error::DiceError;
use crate::model::{Bet, BetState, SettleBetParamsV1, SettleBetUpdateV1};
use crate::{
    DICE_CONTRACT_BETS_TREE, DICE_CONTRACT_HOUSE_TREE,
    DICE_CONTRACT_INFO_TREE, DICE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
};

/// Process instruction for SettleBetV1
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for paying out winnings to the player.
pub fn dice_settle_bet_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: SettleBetParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for payouts
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[SettleBetV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(DiceError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[SettleBetV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DiceError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DICE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DICE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DiceError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    msg!("[dice::settle_bet] Processing settlement");

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let bet: Bet = match wasm::db::db_get(bets_db, &serialize(&params.bet_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(DiceError::BetNotFound.into()),
    };

    msg!("[dice::settle_bet] Found bet, current state: {:?}", bet.state as u8);

    // Verify bet is in Revealed state
    if bet.state != BetState::Revealed {
        if bet.state == BetState::SettledPlayer || bet.state == BetState::SettledHouse {
            return Err(DiceError::InvalidStateTransition.into())
        }
        return Err(DiceError::BetNotRevealed.into())
    }

    // Get the roll result
    let roll = bet.roll.ok_or(DiceError::InvalidRoll)?;
    let player_won = roll < bet.target;

    msg!("[dice::settle_bet] Roll: {}, Target: {}, Player won: {}", roll, bet.target, player_won);

    // Determine payout
    let payout = if player_won {
        bet.calculate_payout().ok_or(DiceError::ArithmeticOverflow)?
    } else {
        0
    };

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(payout),
        bet.id,
    ]);
    validate_child_value_commit(&child_call.data, payout, value_blind)?;

    msg!("[dice::settle_bet] Payout: {}", payout);

    // Determine new state
    let new_state = if player_won { BetState::SettledPlayer } else { BetState::SettledHouse };

    // Create the update
    let update = SettleBetUpdateV1 { bet_id: bet.id, state: new_state, payout };

    msg!("[dice::settle_bet] Settlement prepared");
    Ok(serialize(&update))
}

/// Process update for SettleBetV1
pub fn dice_settle_bet_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: SettleBetUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let house_db = wasm::db::db_lookup(cid, DICE_CONTRACT_HOUSE_TREE)?;

    // Look up and update the bet
    let mut bet: Bet = match wasm::db::db_get(bets_db, &serialize(&update.bet_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(DiceError::BetNotFound.into()),
    };

    // Update bet state (roll was already set during reveal)
    bet.state = update.state;
    wasm::db::db_set(bets_db, &serialize(&update.bet_id), &serialize(&bet))?;

    // If house won, add to house funds
    if update.state == BetState::SettledHouse {
        let house_take = bet.calculate_house_take().ok_or(DiceError::ArithmeticOverflow)?;
        let mut house_balance: u64 = 0;
        if wasm::db::db_contains_key(house_db, b"balance")? {
            if let Some(balance_bytes) = wasm::db::db_get(house_db, b"balance")? {
                house_balance = deserialize(&balance_bytes)?;
            }
        }
        house_balance += house_take;
        wasm::db::db_set(house_db, b"balance", &serialize(&house_balance))?;
        msg!("[dice::settle_bet::update] House accumulated {}", house_take);
    }

    msg!("[dice::settle_bet::update] Bet settled");
    Ok(())
}
