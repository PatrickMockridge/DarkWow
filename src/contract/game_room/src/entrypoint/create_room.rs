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

use dwow_sdk::{
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{CreateRoomParamsV1, CreateRoomUpdateV1, GameRoom, RoomConfig}, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_create_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CreateRoomParamsV1 = dwow_serial::deserialize(&self_.data[1..])?;

    msg!("[CreateRoom] Creating room with token: {:?}", params.token_id);

    // Validate params
    if params.min_stake > params.max_stake {
        msg!("[CreateRoom] Error: min_stake > max_stake");
        return Err(GameRoomError::InvalidAmount.into())
    }

    if params.max_players < 2 {
        msg!("[CreateRoom] Error: max_players < 2");
        return Err(GameRoomError::InvalidAmount.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?;

    // Use owner from params (verified by proof/signature)
    let owner = params.owner;

    // Derive room ID
    let room_id = GameRoom::derive_room_id(
        &dwow_sdk::crypto::ContractId::derive_public(owner),
        params.token_id,
        current_block as u64,
        params.nonce,
    );

    msg!("[CreateRoom] Derived room_id: {:?}", room_id);

    // Create room config
    let config = RoomConfig {
        owner_dao: dwow_sdk::crypto::ContractId::derive_public(owner),
        token_id: params.token_id,
        min_stake: params.min_stake,
        max_stake: params.max_stake,
        entropy_mode: params.entropy_mode,
        confirmation_depth: params.confirmation_depth,
        required_entropy_contributions: params.required_entropy_contributions,
        entropy_contribution_deadline: params.entropy_contribution_deadline,
        max_players: params.max_players,
    };

    // Create room
    let room = GameRoom::new(room_id, config.clone(), current_block as u64);

    // Store room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    wasm::db::db_set(
        rooms_db,
        &dwow_serial::serialize(&room_id),
        &dwow_serial::serialize(&room),
    )?;

    msg!("[CreateRoom] Room created successfully: {:?}", room_id);

    let update = CreateRoomUpdateV1 { room_id, owner_dao: config.owner_dao.clone(), config };
    Ok(dwow_serial::serialize(&update))
}

pub(crate) fn game_room_create_process_update_v1(
    _cid: dwow_sdk::crypto::ContractId,
    update: CreateRoomUpdateV1,
) -> ContractResult {
    // State is already stored in process_instruction
    msg!("[CreateRoom] Update applied for room: {:?}", update.room_id);
    Ok(())
}