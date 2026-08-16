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

use dwow_sdk::crypto::{pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::{
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{CreatePotParamsV1, CreatePotUpdateV1, GameRoom, Pot, RoomState},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_NULLIFIERS_TREE, GAME_ROOM_POTS_TREE,
    GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_create_pot_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = CreatePotParamsV1::decode(&self_.data[1..])?;

    msg!("[CreatePot] Creating pot in room {:?}", params.room_id);

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) = wasm::db::db_get(rooms_db, &params.room_id.to_repr())? else {
        msg!("[CreatePot] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let mut room: GameRoom = GameRoom::decode(&room_data)?;

    // Validate room state
    if room.state != RoomState::Open && room.state != RoomState::Active {
        msg!("[CreatePot] Error: Room not open or active");
        return Err(GameRoomError::RoomNotActive.into())
    }

    // Use player from params (verified by proof)
    let caller = params.player;

    // Verify account exists (player is a room member)
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = [
        &params.room_id.to_repr()[..],
        &poseidon_hash([
            caller.x().expect("pk not identity"),
            caller.y().expect("pk not identity"),
        ]).to_repr()[..],
    ].concat();
    if !wasm::db::db_contains_key(accounts_db, &account_key)? {
        msg!("[CreatePot] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    }

    // Validate nullifier unspent (identity-proof anti-replay)
    let nullifiers_db = wasm::db::db_lookup(cid, GAME_ROOM_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.player_nullifier.to_repr())? {
        msg!("[CreatePot] Error: Duplicate nullifier");
        return Err(GameRoomError::NullifierExists.into())
    }

    // Derive pot_id (domain-separated, matches CreatePotV2 circuit)
    let pot_id = poseidon_hash([
        pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.room_id,
        caller.x().expect("pk not identity"),
        params.nonce,
    ]);

    // Create pot
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let pot = Pot::new(pot_id, params.room_id, current_block);

    // Update room to point at the new pot
    room.current_pot_id = Some(pot_id);
    if room.state == RoomState::Open {
        room.state = RoomState::Active;
    }

    msg!("[CreatePot] Pot {:?} created", pot_id);

    let update = CreatePotUpdateV1 { pot, room, player_nullifier: params.player_nullifier };
    Ok(update.encode())
}

pub(crate) fn game_room_create_pot_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: CreatePotUpdateV1,
) -> ContractResult {
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    wasm::db::db_set(pots_db, &update.pot.pot_id.to_repr(), &update.pot.encode())?;
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    wasm::db::db_set(rooms_db, &update.room.room_id.to_repr(), &update.room.encode())?;
    let nullifiers_db = wasm::db::db_lookup(cid, GAME_ROOM_NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.player_nullifier.to_repr())?;
    msg!("[CreatePot] Update applied: pot {:?}", update.pot.pot_id);
    Ok(())
}
