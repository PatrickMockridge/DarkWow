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
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};

use crate::{
    error::GameRoomError,
    model::{Bet, BetType, CallParamsV1, CallUpdateV1, GameRoom, PlayerAccount, Pot,
           PotContribution, RoomState},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_BETS_TREE, GAME_ROOM_CONTRACT_INFO_TREE,
    GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE, PROMISSORY_NOTE_CONTRACT_ID_KEY,
};

pub(crate) fn game_room_call_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CallParamsV1 = dwow_serial::deserialize(&self_.data[1..])?;

    msg!("[Call] Calling bet in room {:?}", params.room_id);

    // Validate child call is promissory_note::transfer_v1 (0x04) for call stake
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[Call] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(GameRoomError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[Call] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(GameRoomError::InvalidChildCall.into())
    }
    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, GAME_ROOM_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(GameRoomError::InvalidChildCall)?;
    let promissory_note_cid: dwow_sdk::crypto::ContractId = dwow_serial::deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    if promissory_note_cid != dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).unwrap() {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &dwow_serial::serialize(&params.room_id))?
    else {
        msg!("[Call] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: GameRoom =
        dwow_serial::deserialize(&room_data)?;

    // Validate room state
    if room.state != RoomState::Active {
        msg!("[Call] Error: Room not active");
        return Err(GameRoomError::RoomNotActive.into())
    }

    // Validate there's an active bet to call
    if room.current_bet_amount == 0 {
        msg!("[Call] Error: No current bet to call");
        return Err(GameRoomError::NotCurrentBet.into())
    }

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Cannot call self
    if let Some(current_better) = room.current_better {
        if current_better == caller {
            msg!("[Call] Error: Cannot call own bet");
            return Err(GameRoomError::CallerNotPlayer.into())
        }
    }

    // Verify account exists (balance enforced by promissory_note child call)
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = dwow_serial::serialize(&(params.room_id, poseidon_hash([caller.x(), caller.y()])));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Call] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        dwow_serial::deserialize(&account_data)?;

    if account.has_folded {
        msg!("[Call] Error: Player has folded");
        return Err(GameRoomError::CallerNotPlayer.into())
    }

    // Get pot
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    let Some(pot_id) = room.current_pot_id else {
        msg!("[Call] Error: No current pot");
        return Err(GameRoomError::PotNotFound.into())
    };
    let Some(pot_data) = wasm::db::db_get(pots_db, &dwow_serial::serialize(&pot_id))? else {
        msg!("[Call] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let mut pot: Pot =
        dwow_serial::deserialize(&pot_data)?;

    // Validate pot state
    if pot.state != crate::model::PotState::Open {
        msg!("[Call] Error: Pot not open");
        return Err(GameRoomError::PotNotOpen.into())
    }

    // Call amount is the current bet amount
    let call_amount = room.current_bet_amount;

    let value_blind = poseidon_hash([
        pallas::Base::from(call_amount),
        params.room_id,
    ]);
    validate_child_value_commit(&child_call.data, call_amount, value_blind)?;

    // Only update last_action_block (token movement handled by promissory_note child call)
    account.last_action_block = wasm::util::get_verifying_block_height()? as u64;

    // Update pot
    pot.total += call_amount;
    pot.contributions.push(PotContribution {
        player: caller,
        amount: call_amount,
        bet_type: BetType::Call,
        block: wasm::util::get_verifying_block_height()? as u64,
    });
    let new_pot_total = pot.total;

    // Create bet record
    let bet_id = poseidon_hash([
        pot_id,
        caller.xy().0,
        pallas::Base::from(call_amount),
        pallas::Base::from(wasm::util::get_verifying_block_height()? as u64),
    ]);
    let bet = Bet::new(
        bet_id,
        params.room_id,
        pot_id,
        caller,
        call_amount,
        BetType::Call,
        pot.betting_round,
        params.nonce,
        wasm::util::get_verifying_block_height()? as u64,
    );

    // Store bet
    let bets_db = wasm::db::db_lookup(cid, GAME_ROOM_BETS_TREE)?;
    wasm::db::db_set(bets_db, &dwow_serial::serialize(&bet_id), &dwow_serial::serialize(&bet))?;

    // Store updated pot
    wasm::db::db_set(pots_db, &dwow_serial::serialize(&pot_id), &dwow_serial::serialize(&pot))?;

    // Store updated account
    wasm::db::db_set(accounts_db, &account_key, &dwow_serial::serialize(&account))?;

    // Note: Call doesn't change current_better or current_bet_amount
    // The better still needs to respond

    msg!("[Call] Call applied successfully");

    let update = CallUpdateV1 {
        room_id: params.room_id,
        player: caller,
        amount: call_amount,
        new_pot_total,
    };
    Ok(dwow_serial::serialize(&update))
}

pub(crate) fn game_room_call_process_update_v1(
    _cid: dwow_sdk::crypto::ContractId,
    update: CallUpdateV1,
) -> ContractResult {
    msg!(
        "[Call] Update applied: player {:?} amount {}, pot total {}",
        update.player,
        update.amount,
        update.new_pot_total
    );
    Ok(())
}