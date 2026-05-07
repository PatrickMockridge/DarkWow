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

use darkfi_sdk::{
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{PlayerAccount, WithdrawParamsV1, WithdrawUpdateV1},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_withdraw_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: WithdrawParamsV1 = darkfi_serial::deserialize(&self_.data[1..])?;

    msg!("[Withdraw] Requesting withdrawal of {} from room {:?}", params.amount, params.room_id);

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &darkfi_serial::serialize(&params.room_id))?
    else {
        msg!("[Withdraw] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: crate::model::GameRoom =
        darkfi_serial::deserialize(&room_data)?;

    // Validate room state - can withdraw if Open or Active (but not concluded)
    if room.state == crate::model::RoomState::Concluded {
        msg!("[Withdraw] Error: Room concluded");
        return Err(GameRoomError::RoomConcluded.into())
    }

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Get account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = darkfi_serial::serialize(&(params.room_id, caller.xy().0));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Withdraw] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let account: PlayerAccount =
        darkfi_serial::deserialize(&account_data)?;

    // Check available balance (not locked in pot)
    if account.available_balance() < params.amount {
        msg!(
            "[Withdraw] Error: Insufficient available balance (have {}, need {})",
            account.available_balance(),
            params.amount
        );
        return Err(GameRoomError::InsufficientBalance.into())
    }

    // Calculate new balance
    let new_balance = account.balance - params.amount;

    msg!("[Withdraw] New balance: {}", new_balance);

    // Store updated account
    let mut account = account;
    account.balance = new_balance;
    wasm::db::db_set(accounts_db, &account_key, &darkfi_serial::serialize(&account))?;

    let update = WithdrawUpdateV1 { room_id: params.room_id, player: caller, new_balance };
    Ok(darkfi_serial::serialize(&update))
}

pub(crate) fn game_room_withdraw_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    update: WithdrawUpdateV1,
) -> ContractResult {
    msg!(
        "[Withdraw] Withdrawal applied: player {:?} new balance {}",
        update.player,
        update.new_balance
    );
    Ok(())
}