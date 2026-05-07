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
    model::{ClosePotParamsV1, ClosePotUpdateV1, Pot, PotState, RoomState},
    GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_close_pot_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClosePotParamsV1 = darkfi_serial::deserialize(&self_.data[1..])?;

    msg!("[ClosePot] Closing pot {:?} in room {:?}", params.pot_id, params.room_id);

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &darkfi_serial::serialize(&params.room_id))?
    else {
        msg!("[ClosePot] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let mut room: crate::model::GameRoom =
        darkfi_serial::deserialize(&room_data)?;

    // Validate room state
    if room.state != RoomState::Active {
        msg!("[ClosePot] Error: Room not active");
        return Err(GameRoomError::RoomNotActive.into())
    }

    // Validate pot ID matches
    if room.current_pot_id != Some(params.pot_id) {
        msg!("[ClosePot] Error: Pot ID does not match current pot");
        return Err(GameRoomError::PotNotFound.into())
    }

    // Get pot
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    let Some(pot_data) = wasm::db::db_get(pots_db, &darkfi_serial::serialize(&params.pot_id))?
    else {
        msg!("[ClosePot] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let mut pot: Pot =
        darkfi_serial::deserialize(&pot_data)?;

    // Validate pot state
    if pot.state != PotState::Open {
        msg!("[ClosePot] Error: Pot not open");
        return Err(GameRoomError::PotNotOpen.into())
    }

    // Close the pot
    pot.state = PotState::Closed;
    wasm::db::db_set(pots_db, &darkfi_serial::serialize(&params.pot_id), &darkfi_serial::serialize(&pot))?;

    // Update room
    room.current_pot_id = None;
    room.current_bet_amount = 0;
    room.current_better = None;
    wasm::db::db_set(rooms_db, &darkfi_serial::serialize(&params.room_id), &darkfi_serial::serialize(&room))?;

    msg!("[ClosePot] Pot closed successfully");

    let update = ClosePotUpdateV1 {
        room_id: params.room_id,
        pot_id: params.pot_id,
        new_pot_state: PotState::Closed,
        new_betting_round: pot.betting_round + 1,
        new_current_bet: 0,
        new_current_better: None,
    };
    Ok(darkfi_serial::serialize(&update))
}

pub(crate) fn game_room_close_pot_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    update: ClosePotUpdateV1,
) -> ContractResult {
    msg!(
        "[ClosePot] Update applied: pot {:?} closed, new betting round {}",
        update.pot_id,
        update.new_betting_round
    );
    Ok(())
}