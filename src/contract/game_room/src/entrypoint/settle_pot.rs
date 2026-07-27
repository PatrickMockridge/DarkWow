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
    crypto::pasta_prelude::PrimeField,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{SettlePotParamsV1, SettlePotUpdateV1, Pot, PotState, RoomState},
    GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_settle_pot_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: SettlePotParamsV1 = dwow_serial::deserialize(&self_.data[1..])?;

    msg!(
        "[SettlePot] Settling pot {:?} in room {:?} with {} winners",
        params.pot_id,
        params.room_id,
        params.winners.len()
    );

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &params.room_id.to_repr())?
    else {
        msg!("[SettlePot] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: crate::model::GameRoom =
        crate::model::GameRoom::decode(&room_data)?;

    // Validate caller is owner DAO (use caller from params, verified by signature)
    let caller = params.caller;
    if dwow_sdk::crypto::ContractId::derive_public(caller) != room.config.owner_dao {
        msg!("[SettlePot] Error: Caller is not owner DAO");
        return Err(GameRoomError::CallerNotOwner.into())
    }

    // Validate room state
    if room.state != RoomState::Active && room.state != RoomState::Open {
        msg!("[SettlePot] Error: Room not active or open");
        return Err(GameRoomError::RoomNotActive.into())
    }

    // Get pot
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    let Some(pot_data) = wasm::db::db_get(pots_db, &params.pot_id.to_repr())?
    else {
        msg!("[SettlePot] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let mut pot: Pot =
        Pot::decode(&pot_data)?;

    // Validate pot state
    if pot.state == PotState::Settled {
        msg!("[SettlePot] Error: Pot already settled");
        return Err(GameRoomError::PotSettled.into())
    }

    // Validate winners list
    if params.winners.is_empty() {
        msg!("[SettlePot] Error: No winners provided");
        return Err(GameRoomError::InvalidAmount.into())
    }

    // Calculate total payout
    let total_payout: u64 = params.winners.iter().map(|(_, amount)| amount).sum();
    if total_payout > pot.total {
        msg!(
            "[SettlePot] Error: Total payout {} exceeds pot total {}",
            total_payout,
            pot.total
        );
        return Err(GameRoomError::InvalidAmount.into())
    }

    // Settle the pot
    pot.state = PotState::Settled;
    wasm::db::db_set(pots_db, &params.pot_id.to_repr(), &pot.encode())?;

    msg!("[SettlePot] Pot {:?} settled with total {}", params.pot_id, pot.total);

    let winners: Vec<_> = params.winners.iter().map(|(pubkey, _)| *pubkey).collect();
    let payouts: Vec<_> = params.winners.iter().map(|(_, amount)| *amount).collect();

    let update = SettlePotUpdateV1 {
        room_id: params.room_id,
        pot_id: params.pot_id,
        new_pot_state: PotState::Settled,
        winners,
        payouts,
    };
    Ok(update.encode())
}

pub(crate) fn game_room_settle_pot_process_update_v1(
    _cid: dwow_sdk::crypto::ContractId,
    update: SettlePotUpdateV1,
) -> ContractResult {
    msg!(
        "[SettlePot] Update applied: pot {:?} settled with {} winners",
        update.pot_id,
        update.winners.len()
    );
    Ok(())
}