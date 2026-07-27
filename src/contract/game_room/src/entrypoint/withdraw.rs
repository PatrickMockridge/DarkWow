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

use dwow_sdk::crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId};
use dwow_sdk::{
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};

use crate::{
    error::GameRoomError,
    model::{WithdrawParamsV1, WithdrawUpdateV1},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_CONTRACT_INFO_TREE, GAME_ROOM_ROOMS_TREE,
    PROMISSORY_NOTE_CONTRACT_ID_KEY,
};

pub(crate) fn game_room_withdraw_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: WithdrawParamsV1 = dwow_serial::deserialize(&self_.data[1..])?;

    msg!("[Withdraw] Requesting withdrawal of {} from room {:?}", params.amount, params.room_id);

    // Validate child call is promissory_note::transfer_v1 (0x04) for token withdrawal
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[Withdraw] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(GameRoomError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[Withdraw] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(GameRoomError::InvalidChildCall.into())
    }
    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, GAME_ROOM_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(GameRoomError::InvalidChildCall)?;
    let promissory_note_cid: dwow_sdk::crypto::ContractId = ContractId::from_bytes(promissory_note_bytes.as_slice().try_into().map_err(|_| GameRoomError::InvalidChildCall)?)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    if promissory_note_cid != dwow_sdk::crypto::ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        params.room_id,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &params.room_id.to_repr())?
    else {
        msg!("[Withdraw] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: crate::model::GameRoom =
        crate::model::GameRoom::decode(&room_data)?;

    // Validate room state - can withdraw if Open or Active (but not concluded)
    if room.state == crate::model::RoomState::Concluded {
        msg!("[Withdraw] Error: Room concluded");
        return Err(GameRoomError::RoomConcluded.into())
    }

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Verify account exists (balance enforced by promissory_note child call)
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = [&params.room_id.to_repr()[..], &poseidon_hash([caller.x().expect("pk not identity"), caller.y().expect("pk not identity")]).to_repr()[..]].concat();
    if !wasm::db::db_contains_key(accounts_db, &account_key)? {
        msg!("[Withdraw] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    }

    msg!("[Withdraw] Withdrawal prepared: player {:?} amount {}", caller, params.amount);

    let update = WithdrawUpdateV1 { room_id: params.room_id, player: caller, amount: params.amount };
    Ok(update.encode())
}

pub(crate) fn game_room_withdraw_process_update_v1(
    _cid: dwow_sdk::crypto::ContractId,
    update: WithdrawUpdateV1,
) -> ContractResult {
    msg!(
        "[Withdraw] Withdrawal applied: player {:?} amount {}",
        update.player,
        update.amount
    );
    Ok(())
}