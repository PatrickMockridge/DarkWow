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
    model::{DepositParamsV1, DepositUpdateV1, GameRoom, PlayerAccount},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_deposit_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: DepositParamsV1 = darkfi_serial::deserialize(&self_.data[1..])?;

    msg!("[Deposit] Depositing {} to room {:?}", params.amount, params.room_id);

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &darkfi_serial::serialize(&params.room_id))?
    else {
        msg!("[Deposit] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: GameRoom =
        darkfi_serial::deserialize(&room_data)?;

    // Validate room state
    if room.state != crate::model::RoomState::Open {
        msg!("[Deposit] Error: Room not open");
        return Err(GameRoomError::RoomNotOpen.into())
    }

    // Validate deposit amount
    if params.amount < room.config.min_stake {
        msg!("[Deposit] Error: Amount below minimum stake");
        return Err(GameRoomError::StakeBelowMin.into())
    }

    if params.amount > room.config.max_stake {
        msg!("[Deposit] Error: Amount above maximum stake");
        return Err(GameRoomError::StakeAboveMax.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?;

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Get or create account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = darkfi_serial::serialize(&(params.room_id, caller.xy().0));
    let account = match wasm::db::db_get(accounts_db, &account_key)? {
        Some(data) => {
            let mut acc: PlayerAccount =
                darkfi_serial::deserialize(&data)?;
            acc.balance += params.amount;
            acc.last_action_block = current_block as u64;
            acc
        }
        None => PlayerAccount::new(caller, params.amount, current_block as u64),
    };

    let new_balance = account.balance;

    msg!("[Deposit] Player balance now: {}", new_balance);

    // Store updated account
    wasm::db::db_set(accounts_db, &account_key, &darkfi_serial::serialize(&account))?;

    let update = DepositUpdateV1 { room_id: params.room_id, player: caller, new_balance };
    Ok(darkfi_serial::serialize(&update))
}

pub(crate) fn game_room_deposit_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    update: DepositUpdateV1,
) -> ContractResult {
    msg!(
        "[Deposit] Deposit applied: player {:?} balance now {}",
        update.player,
        update.new_balance
    );
    Ok(())
}