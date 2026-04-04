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

use darkfi_sdk::{
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{ClaimParamsV1, ClaimUpdateV1, PlayerAccount, Pot, PotState},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_NULLIFIERS_TREE,
    GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_claim_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClaimParamsV1 = darkfi_serial::deserialize(&self_.data[1..])?;

    msg!(
        "[Claim] Claiming pot {:?} in room {:?} for winner {:?}",
        params.pot_id,
        params.room_id,
        params.winner
    );

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(_room_data) =
        wasm::db::db_get(rooms_db, &darkfi_serial::serialize(&params.room_id))?
    else {
        msg!("[Claim] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };

    // Get pot
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    let Some(pot_data) = wasm::db::db_get(pots_db, &darkfi_serial::serialize(&params.pot_id))?
    else {
        msg!("[Claim] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let pot: Pot =
        darkfi_serial::deserialize(&pot_data)?;

    // Validate pot state - must be settled
    if pot.state != PotState::Settled {
        msg!("[Claim] Error: Pot not settled");
        return Err(GameRoomError::PotSettled.into())
    }

    // Check nullifier to prevent double-claim
    let nullifiers_db = wasm::db::db_lookup(cid, GAME_ROOM_NULLIFIERS_TREE)?;
    let claim_key = darkfi_serial::serialize(&(params.pot_id, params.winner.xy().0));
    if wasm::db::db_contains_key(nullifiers_db, &claim_key)? {
        msg!("[Claim] Error: Already claimed");
        return Err(GameRoomError::AlreadyClaimed.into())
    }

    // Find the payout amount for this winner
    // In a real implementation, this would be passed as a parameter or looked up
    // For now, we assume the winner is in the settled pot
    let payout_amount = pot.total; // Simplified - actual implementation would track per-winner amounts

    // Get winner's account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = darkfi_serial::serialize(&(params.room_id, params.winner.xy().0));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Claim] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        darkfi_serial::deserialize(&account_data)?;

    // Calculate new balance (unlock the amount they had in the pot and add winnings)
    // Note: locked amount stays locked until explicitly unlocked
    // The winnings go to balance
    let winnings = payout_amount; // Simplified
    account.balance += winnings;

    // If they had locked funds from this pot, reduce locked
    // (In a real implementation, we'd track how much each player locked)
    let new_balance = account.balance;

    // Store updated account
    wasm::db::db_set(accounts_db, &account_key, &darkfi_serial::serialize(&account))?;

    // Record nullifier to prevent double-claim
    wasm::db::db_set(nullifiers_db, &claim_key, &[])?;

    msg!(
        "[Claim] Claim applied: winner {:?} received {} (new balance: {})",
        params.winner,
        winnings,
        new_balance
    );

    let update = ClaimUpdateV1 {
        room_id: params.room_id,
        pot_id: params.pot_id,
        winner: params.winner,
        amount: winnings,
        new_balance,
    };
    Ok(darkfi_serial::serialize(&update))
}

pub(crate) fn game_room_claim_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    update: ClaimUpdateV1,
) -> ContractResult {
    msg!(
        "[Claim] Update applied: winner {:?} claimed {} from pot {:?}, new balance {}",
        update.winner,
        update.amount,
        update.pot_id,
        update.new_balance
    );
    Ok(())
}