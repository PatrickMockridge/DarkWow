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

//! RevealRollV1 Implementation

use darkfi_sdk::{
    error::ContractError,
    msg,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::DiceError;
use crate::model::{calculate_roll, Bet, BetState, RevealRollParamsV1, RevealRollUpdateV1};
use crate::DICE_CONTRACT_BETS_TREE;
use crate::ROLL_RANGE;

/// Process instruction for RevealRollV1
pub fn dice_reveal_roll_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: RevealRollParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[dice::reveal_roll] Processing roll reveal");

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&params.bet_id))?.unwrap();
    let bet: Bet = deserialize(&bet_bytes)?;

    msg!("[dice::reveal_roll] Found bet, current state: {:?}", bet.state as u8);

    // Verify bet is in Committed state
    if bet.state != BetState::Committed {
        return Err(DiceError::InvalidStateTransition.into())
    }

    // Verify the secret nonce matches
    if bet.secret_nonce != params.secret_nonce {
        return Err(DiceError::CommitmentMismatch.into())
    }

    // Get block hash for randomness
    let tx_hash = wasm::util::get_tx_hash()?;

    // Calculate the roll using full tx_hash for better randomness
    let roll = calculate_roll(tx_hash.0, bet.id, params.secret_nonce);

    msg!("[dice::reveal_roll] Calculated roll: {} (target: {})", roll, bet.target);

    // Validate roll is in range
    if roll >= ROLL_RANGE {
        return Err(DiceError::InvalidRoll.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Determine new state
    let new_state = if roll < bet.target { BetState::SettledPlayer } else { BetState::Revealed };

    // Create the update
    let update = RevealRollUpdateV1 {
        bet_id: bet.id,
        roll,
        state: new_state,
        revealed_at: current_block as u64,
    };

    msg!("[dice::reveal_roll] Roll revealed successfully");
    Ok(serialize(&update))
}

/// Process update for RevealRollV1
pub fn dice_reveal_roll_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: RevealRollUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;

    // Look up and update the bet
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&update.bet_id))?.unwrap();
    let mut bet: Bet = deserialize(&bet_bytes)?;

    // Update bet state
    bet.roll = Some(update.roll);
    bet.state = update.state;
    bet.revealed_at = update.revealed_at;

    // Store updated bet
    wasm::db::db_set(bets_db, &serialize(&update.bet_id), &serialize(&bet))?;

    msg!("[dice::reveal_roll::update] Bet updated");
    Ok(())
}
