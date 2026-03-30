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

//! HouseCloseV1 Implementation

use darkfi_sdk::{
    error::ContractError,
    msg,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::BaccaratError;
use crate::model::{calculate_house_take, Bet, BetState, HouseCloseParamsV1, HouseCloseUpdateV1};
use crate::{
    BACCARAT_CONTRACT_BETS_TREE, BACCARAT_CONTRACT_HOUSE_PUBKEY,
    BACCARAT_CONTRACT_INFO_TREE,
};

/// Process instruction for HouseCloseV1
pub fn baccarat_house_close_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: HouseCloseParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[baccarat::house_close] Closing bet_id: {:?}", params.bet_id);

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&params.bet_id))?
        .ok_or(BaccaratError::BetNotFound)?;

    let bet: Bet = deserialize(&bet_bytes)?;

    // Verify bet is in correct state
    // Can close if: Committed (timeout reached) or CardsDrawn
    match bet.state {
        BetState::Committed | BetState::CardsDrawn => {}
        _ => return Err(BaccaratError::InvalidStateTransition.into()),
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // If bet is still Committed, verify timeout has been reached
    if bet.state == BetState::Committed && (current_block as u64) < bet.settle_block {
        return Err(BaccaratError::BetTimeoutNotReached.into())
    }

    // Verify the house is the one closing (authorization check)
    let info_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_INFO_TREE)?;
    let house_pubkey_bytes =
        wasm::db::db_get(info_db, BACCARAT_CONTRACT_HOUSE_PUBKEY)?;

    if let Some(bytes) = house_pubkey_bytes {
        // House pubkey is stored - verify caller matches
        // For now, we check via signature or explicit authorization
        // In production, this should verify the transaction signed by house key
        let _house_pubkey: darkfi_sdk::crypto::PublicKey = deserialize(&bytes)?;
        // TODO: Add actual house authorization check via signature or contract call
    }

    // Calculate house's take (player's bet value)
    let house_take = calculate_house_take(&bet);

    msg!("[baccarat::house_close] House take: {}", house_take);

    // Create the update
    let update = HouseCloseUpdateV1 {
        bet_id: bet.id,
        house_take,
        state: BetState::Cancelled,
    };

    msg!("[baccarat::house_close] Bet closed by house");
    Ok(serialize(&update))
}

/// Process update for HouseCloseV1
pub fn baccarat_house_close_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: HouseCloseUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;

    // Look up bet to update state
    let bet_bytes = wasm::db::db_get(bets_db, &serialize(&update.bet_id))?
        .ok_or(BaccaratError::BetNotFound)?;

    let mut bet: Bet = deserialize(&bet_bytes)?;
    bet.state = BetState::Cancelled;

    // Store updated bet
    wasm::db::db_set(bets_db, &serialize(&bet.id), &serialize(&bet))?;

    msg!("[baccarat::house_close::update] Bet state updated to Cancelled");
    Ok(())
}