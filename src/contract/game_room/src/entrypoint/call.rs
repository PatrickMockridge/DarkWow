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
    crypto::poseidon_hash,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{Bet, BetType, CallParamsV1, CallUpdateV1, GameRoom, PlayerAccount, Pot,
           PotContribution, RoomState},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_BETS_TREE, GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_call_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CallParamsV1 = darkfi_serial::deserialize(&self_.data[1..])?;

    msg!("[Call] Calling bet in room {:?}", params.room_id);

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &darkfi_serial::serialize(&params.room_id))?
    else {
        msg!("[Call] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let room: GameRoom =
        darkfi_serial::deserialize(&room_data)?;

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

    // Get account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = darkfi_serial::serialize(&(params.room_id, caller.xy().0));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Call] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        darkfi_serial::deserialize(&account_data)?;

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
    let Some(pot_data) = wasm::db::db_get(pots_db, &darkfi_serial::serialize(&pot_id))? else {
        msg!("[Call] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let mut pot: Pot =
        darkfi_serial::deserialize(&pot_data)?;

    // Validate pot state
    if pot.state != crate::model::PotState::Open {
        msg!("[Call] Error: Pot not open");
        return Err(GameRoomError::PotNotOpen.into())
    }

    // Call amount is the current bet amount
    let call_amount = room.current_bet_amount;

    // Check available balance
    if account.available_balance() < call_amount {
        msg!(
            "[Call] Error: Insufficient available balance (have {}, need {})",
            account.available_balance(),
            call_amount
        );
        return Err(GameRoomError::InsufficientBalance.into())
    }

    // Update account
    account.balance -= call_amount;
    account.locked += call_amount;
    let new_balance = account.balance;
    let new_locked = account.locked;

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
    wasm::db::db_set(bets_db, &darkfi_serial::serialize(&bet_id), &darkfi_serial::serialize(&bet))?;

    // Store updated pot
    wasm::db::db_set(pots_db, &darkfi_serial::serialize(&pot_id), &darkfi_serial::serialize(&pot))?;

    // Store updated account
    wasm::db::db_set(accounts_db, &account_key, &darkfi_serial::serialize(&account))?;

    // Note: Call doesn't change current_better or current_bet_amount
    // The better still needs to respond

    msg!("[Call] Call applied successfully");

    let update = CallUpdateV1 {
        room_id: params.room_id,
        player: caller,
        new_balance,
        new_locked,
        new_pot_total,
    };
    Ok(darkfi_serial::serialize(&update))
}

pub(crate) fn game_room_call_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    update: CallUpdateV1,
) -> ContractResult {
    msg!(
        "[Call] Update applied: player {:?} new balance {}, locked {}, pot total {}",
        update.player,
        update.new_balance,
        update.new_locked,
        update.new_pot_total
    );
    Ok(())
}