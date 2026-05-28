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
    crypto::poseidon_hash,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    wasm, ContractCall,
};
use dwow_money_v3_contract::validation::validate_child_contract_id;

use crate::{
    error::GameRoomError,
    model::{WithdrawParamsV1, WithdrawUpdateV1},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_CONTRACT_INFO_TREE, GAME_ROOM_ROOMS_TREE,
    MONEY_V3_CONTRACT_ID_KEY,
};

pub(crate) fn game_room_withdraw_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: WithdrawParamsV1 = dwow_serial::deserialize(&self_.data[1..])?;

    msg!("[Withdraw] Requesting withdrawal of {} from room {:?}", params.amount, params.room_id);

    // Validate child call is money_v3::transfer_v1 (0x04) for token withdrawal
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[Withdraw] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(GameRoomError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[Withdraw] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(GameRoomError::InvalidChildCall.into())
    }
    // Validate child call targets money_v3 (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, GAME_ROOM_CONTRACT_INFO_TREE)?;
    let money_v3_bytes = wasm::db::db_get(info_db, MONEY_V3_CONTRACT_ID_KEY)?
        .ok_or(GameRoomError::InvalidChildCall)?;
    let money_v3_cid: dwow_sdk::crypto::ContractId = dwow_serial::deserialize(&money_v3_bytes)?;
    // Only validate if money_v3_contract_id was configured (non-zero)
    if money_v3_cid != dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).unwrap() {
        validate_child_contract_id(&child_call.contract_id, &money_v3_cid)?;
    }

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &dwow_serial::serialize(&params.room_id))?
    else {
        msg!("[Withdraw] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: crate::model::GameRoom =
        dwow_serial::deserialize(&room_data)?;

    // Validate room state - can withdraw if Open or Active (but not concluded)
    if room.state == crate::model::RoomState::Concluded {
        msg!("[Withdraw] Error: Room concluded");
        return Err(GameRoomError::RoomConcluded.into())
    }

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Verify account exists (balance enforced by money_v3 child call)
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = dwow_serial::serialize(&(params.room_id, poseidon_hash([caller.x(), caller.y()])));
    if !wasm::db::db_contains_key(accounts_db, &account_key)? {
        msg!("[Withdraw] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    }

    msg!("[Withdraw] Withdrawal prepared: player {:?} amount {}", caller, params.amount);

    let update = WithdrawUpdateV1 { room_id: params.room_id, player: caller, amount: params.amount };
    Ok(dwow_serial::serialize(&update))
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