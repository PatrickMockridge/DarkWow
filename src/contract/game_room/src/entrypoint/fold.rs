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
    model::{FoldParamsV1, FoldUpdateV1, PlayerAccount, RoomState},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_fold_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: FoldParamsV1 = darkfi_serial::deserialize(&self_.data[1..])?;

    msg!("[Fold] Folding in room {:?}", params.room_id);

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &darkfi_serial::serialize(&params.room_id))?
    else {
        msg!("[Fold] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let _room: crate::model::GameRoom =
        darkfi_serial::deserialize(&room_data)?;

    // Validate room state
    if _room.state != RoomState::Active {
        msg!("[Fold] Error: Room not active");
        return Err(GameRoomError::RoomNotActive.into())
    }

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Get account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = darkfi_serial::serialize(&(params.room_id, caller.xy().0));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Fold] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        darkfi_serial::deserialize(&account_data)?;

    if account.has_folded {
        msg!("[Fold] Error: Already folded");
        return Err(GameRoomError::CallerNotPlayer.into())
    }

    // Mark as folded
    account.has_folded = true;
    wasm::db::db_set(accounts_db, &account_key, &darkfi_serial::serialize(&account))?;

    msg!("[Fold] Player {:?} folded", caller);

    let update = FoldUpdateV1 { room_id: params.room_id, player: caller, has_folded: true };
    Ok(darkfi_serial::serialize(&update))
}

pub(crate) fn game_room_fold_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    update: FoldUpdateV1,
) -> ContractResult {
    msg!(
        "[Fold] Update applied: player {:?} folded: {}",
        update.player,
        update.has_folded
    );
    Ok(())
}