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

//! SettleBetV1 Implementation

use darkfi_sdk::{
    error::ContractError,
    msg,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::BaccaratError;
use crate::model::{calculate_payout, Bet, BetState, SettleBetParamsV1, SettleBetUpdateV1};
use crate::BACCARAT_CONTRACT_BETS_TREE;

/// Process instruction for SettleBetV1
pub fn baccarat_settle_bet_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: SettleBetParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[baccarat::settle_bet] Settling bet_id: {:?}", params.bet_id);

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&params.bet_id))?
        .ok_or(BaccaratError::BetNotFound)?;

    let bet: Bet = deserialize(&bet_bytes)?;

    // Verify bet is in CardsDrawn state
    if bet.state != BetState::CardsDrawn {
        return Err(BaccaratError::InvalidStateTransition.into())
    }

    // Verify we have an outcome
    let outcome = bet.outcome.ok_or(BaccaratError::CardsNotDrawn)?;

    // Calculate payout
    let payout = calculate_payout(&bet, outcome);

    msg!("[baccarat::settle_bet] Calculated payout: {}", payout);

    // Create the update
    let update = SettleBetUpdateV1 {
        bet_id: bet.id,
        payout,
        state: BetState::Settled,
    };

    msg!("[baccarat::settle_bet] Bet settled successfully");
    Ok(serialize(&update))
}

/// Process update for SettleBetV1
pub fn baccarat_settle_bet_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: SettleBetUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;

    // Look up bet to update state
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&update.bet_id))?
        .ok_or(BaccaratError::BetNotFound)?;

    let mut bet: Bet = deserialize(&bet_bytes)?;
    bet.state = BetState::Settled;

    // Store updated bet
    wasm::db::db_set(bets_db, &serialize(&bet.id), &serialize(&bet))?;

    msg!("[baccarat::settle_bet::update] Bet state updated to Settled");
    Ok(())
}
