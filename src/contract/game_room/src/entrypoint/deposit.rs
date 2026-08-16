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
    model::{DepositParamsV1, DepositUpdateV1, GameRoom, PlayerAccount},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_CONTRACT_INFO_TREE, GAME_ROOM_ROOMS_TREE,
    PROMISSORY_NOTE_CONTRACT_ID_KEY,
};

pub(crate) fn game_room_deposit_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = DepositParamsV1::decode(&self_.data[1..])?;

    msg!("[Deposit] Depositing {} to room {:?}", params.amount, params.room_id);

    // Validate child call is promissory_note::transfer_v1 (0x04) for token deposit
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[Deposit] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(GameRoomError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[Deposit] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(GameRoomError::InvalidChildCall.into())
    }
    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, GAME_ROOM_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(GameRoomError::InvalidChildCall)?;
    let promissory_note_cid: dwow_sdk::crypto::ContractId = ContractId::from_bytes(promissory_note_bytes.as_slice().try_into().map_err(|_| GameRoomError::InvalidChildCall)?)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

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
        msg!("[Deposit] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: GameRoom =
        GameRoom::decode(&room_data)?;

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
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Get or create account (token balance tracked by promissory_note)
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = [&params.room_id.to_repr()[..], &poseidon_hash([caller.x().expect("pk not identity"), caller.y().expect("pk not identity")]).to_repr()[..]].concat();
    let account = match wasm::db::db_get(accounts_db, &account_key)? {
        Some(data) => {
            let mut acc: PlayerAccount =
                PlayerAccount::decode(&data)?;
            acc.last_action_block = current_block;
            acc
        }
        None => PlayerAccount::new(caller, current_block, params.instance_seed),
    };

    msg!("[Deposit] Player account prepared at block {}", current_block);

    let update = DepositUpdateV1 { room_id: params.room_id, account };
    Ok(update.encode())
}

pub(crate) fn game_room_deposit_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: DepositUpdateV1,
) -> ContractResult {
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = [
        &update.room_id.to_repr()[..],
        &poseidon_hash([
            update.account.pubkey.x().expect("pk not identity"),
            update.account.pubkey.y().expect("pk not identity"),
        ]).to_repr()[..],
    ].concat();
    wasm::db::db_set(accounts_db, &account_key, &update.account.encode())?;
    msg!("[Deposit] Deposit applied: player {:?}", update.account.pubkey);
    Ok(())
}