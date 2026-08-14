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
    model::{
        Bet, BetType, PlayerAccount, RaiseParamsV1, RaiseUpdateV1, GameRoom, Pot,
        PotContribution, RoomState,
    },
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_BETS_TREE, GAME_ROOM_CONTRACT_INFO_TREE,
    GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE,
    PROMISSORY_NOTE_CONTRACT_ID_KEY,
};

pub(crate) fn game_room_raise_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = RaiseParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[Raise] Raising bet by {} in room {:?}",
        params.amount,
        params.room_id
    );

    // Validate child call is promissory_note::transfer_v1 (0x04) for raise stake
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[Raise] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(GameRoomError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[Raise] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
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
        msg!("[Raise] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let mut room: GameRoom =
        GameRoom::decode(&room_data)?;

    // Validate room state
    if room.state != RoomState::Active {
        msg!("[Raise] Error: Room not active");
        return Err(GameRoomError::RoomNotActive.into())
    }

    // Validate there's an active bet to raise
    if room.current_bet_amount == 0 {
        msg!("[Raise] Error: No current bet to raise");
        return Err(GameRoomError::NotCurrentBet.into())
    }

    // New total must be more than current bet
    let raise_total = room.current_bet_amount + params.amount;

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Cannot raise self
    if let Some(current_better) = room.current_better {
        if current_better == caller {
            msg!("[Raise] Error: Cannot raise own bet");
            return Err(GameRoomError::CallerNotPlayer.into())
        }
    }

    // Verify account exists (balance enforced by promissory_note child call)
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = [&params.room_id.to_repr()[..], &poseidon_hash([caller.x().expect("pk not identity"), caller.y().expect("pk not identity")]).to_repr()[..]].concat();
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Raise] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        PlayerAccount::decode(&account_data)?;

    if account.has_folded {
        msg!("[Raise] Error: Player has folded");
        return Err(GameRoomError::CallerNotPlayer.into())
    }

    // Get pot
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    let Some(pot_id) = room.current_pot_id else {
        msg!("[Raise] Error: No current pot");
        return Err(GameRoomError::PotNotFound.into())
    };
    let Some(pot_data) = wasm::db::db_get(pots_db, &pot_id.to_repr())? else {
        msg!("[Raise] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let mut pot: Pot =
        Pot::decode(&pot_data)?;

    // Validate pot state
    if pot.state != crate::model::PotState::Open {
        msg!("[Raise] Error: Pot not open");
        return Err(GameRoomError::PotNotOpen.into())
    }

    // Only update last_action_block (token movement handled by promissory_note child call)
    account.last_action_block = wasm::util::get_verifying_block_height()?.get();

    // Update pot
    pot.total += params.amount;
    pot.contributions.push(PotContribution {
        player: caller,
        amount: params.amount,
        bet_type: BetType::Raise,
        block: wasm::util::get_verifying_block_height()?.get(),
    });
    let new_pot_total = pot.total;

    // Create bet record
    let bet_id = poseidon_hash([
        pot_id,
        caller.xy().expect("pk not identity").0,
        pallas::Base::from(raise_total),
        pallas::Base::from(wasm::util::get_verifying_block_height()?.get()),
    ]);
    let bet = Bet::new(
        bet_id,
        params.room_id,
        pot_id,
        caller,
        raise_total,
        BetType::Raise,
        pot.betting_round,
        params.nonce,
        wasm::util::get_verifying_block_height()?.get(),
    );

    // Store bet
    let bets_db = wasm::db::db_lookup(cid, GAME_ROOM_BETS_TREE)?;
    wasm::db::db_set(bets_db, &bet_id.to_repr(), &bet.encode())?;

    // Store updated pot
    wasm::db::db_set(pots_db, &pot_id.to_repr(), &pot.encode())?;

    // Store updated account
    wasm::db::db_set(accounts_db, &account_key, &account.encode())?;

    // Update room
    room.current_bet_amount = raise_total;
    room.current_better = Some(caller);
    wasm::db::db_set(
        rooms_db,
        &params.room_id.to_repr(),
        &room.encode(),
    )?;

    msg!("[Raise] Raise applied successfully");

    let update = RaiseUpdateV1 {
        room_id: params.room_id,
        player: caller,
        amount: params.amount,
        new_pot_total,
        new_current_bet: raise_total,
    };
    Ok(update.encode())
}

pub(crate) fn game_room_raise_process_update_v1(
    _cid: dwow_sdk::crypto::ContractId,
    update: RaiseUpdateV1,
) -> ContractResult {
    msg!(
        "[Raise] Update applied: player {:?} amount {}, pot total {}, new bet {}",
        update.player,
        update.amount,
        update.new_pot_total,
        update.new_current_bet
    );
    Ok(())
}