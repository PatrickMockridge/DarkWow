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
    model::{
        Bet, BetType, GameRoom, PlaceBetParamsV1, PlaceBetUpdateV1, PlayerAccount, Pot, PotContribution,
        RoomState,
    },
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_BETS_TREE, GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_place_bet_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: PlaceBetParamsV1 = darkfi_serial::deserialize(&self_.data[1..])?;

    msg!(
        "[PlaceBet] Placing bet of {} (type {:?}) in room {:?}",
        params.amount,
        params.bet_type,
        params.room_id
    );

    // Validate bet type
    match params.bet_type {
        BetType::Ante | BetType::Blind | BetType::Bet => {}
        _ => {
            msg!("[PlaceBet] Error: Invalid initial bet type");
            return Err(GameRoomError::InvalidBetType.into())
        }
    }

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(room_data) =
        wasm::db::db_get(rooms_db, &darkfi_serial::serialize(&params.room_id))?
    else {
        msg!("[PlaceBet] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };
    let mut room: GameRoom =
        darkfi_serial::deserialize(&room_data)?;

    // Validate room state
    if room.state != RoomState::Open && room.state != RoomState::Active {
        msg!("[PlaceBet] Error: Room not open or active");
        return Err(GameRoomError::RoomNotActive.into())
    }

    // If there's already a current bet, this must be higher (or it's a call/raise)
    if room.current_bet_amount > 0 && params.amount <= room.current_bet_amount {
        msg!(
            "[PlaceBet] Error: Bet must be higher than current bet {}",
            room.current_bet_amount
        );
        return Err(GameRoomError::NotCurrentBet.into())
    }

    // Use player from params (verified by proof/signature)
    let caller = params.player;

    // Get account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = darkfi_serial::serialize(&(params.room_id, caller.xy().0));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[PlaceBet] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        darkfi_serial::deserialize(&account_data)?;

    if account.has_folded {
        msg!("[PlaceBet] Error: Player has folded");
        return Err(GameRoomError::CallerNotPlayer.into())
    }

    // Check available balance
    if account.available_balance() < params.amount {
        msg!(
            "[PlaceBet] Error: Insufficient available balance (have {}, need {})",
            account.available_balance(),
            params.amount
        );
        return Err(GameRoomError::InsufficientBalance.into())
    }

    // Get or create current pot
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    let pot_id = match room.current_pot_id {
        Some(id) => id,
        None => {
            let new_pot_id = poseidon_hash([
                params.room_id,
                pallas::Base::from(wasm::util::get_verifying_block_height()? as u64),
                caller.xy().0,
            ]);
            let new_pot = Pot::new(new_pot_id, params.room_id, wasm::util::get_verifying_block_height()? as u64);
            wasm::db::db_set(
                pots_db,
                &darkfi_serial::serialize(&new_pot_id),
                &darkfi_serial::serialize(&new_pot),
            )?;
            new_pot_id
        }
    };

    // Get pot
    let Some(pot_data) = wasm::db::db_get(pots_db, &darkfi_serial::serialize(&pot_id))? else {
        msg!("[PlaceBet] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let mut pot: Pot =
        darkfi_serial::deserialize(&pot_data)?;

    // Validate pot state
    if pot.state != crate::model::PotState::Open {
        msg!("[PlaceBet] Error: Pot not open");
        return Err(GameRoomError::PotNotOpen.into())
    }

    // Update account
    account.balance -= params.amount;
    account.locked += params.amount;
    let new_balance = account.balance;
    let new_locked = account.locked;

    // Update pot
    pot.total += params.amount;
    pot.contributions.push(PotContribution {
        player: caller,
        amount: params.amount,
        bet_type: params.bet_type,
        block: wasm::util::get_verifying_block_height()? as u64,
    });
    let new_pot_total = pot.total;

    // Create bet record
    let bet_id = poseidon_hash([
        pot_id,
        caller.xy().0,
        pallas::Base::from(params.amount),
        pallas::Base::from(wasm::util::get_verifying_block_height()? as u64),
    ]);
    let bet = Bet::new(
        bet_id,
        params.room_id,
        pot_id,
        caller,
        params.amount,
        params.bet_type,
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

    // Update room state
    room.current_bet_amount = params.amount;
    room.current_better = Some(caller);
    if room.state == RoomState::Open {
        room.state = RoomState::Active;
    }
    wasm::db::db_set(
        rooms_db,
        &darkfi_serial::serialize(&params.room_id),
        &darkfi_serial::serialize(&room),
    )?;

    msg!("[PlaceBet] Bet placed successfully: {:?}", bet_id);

    let update = PlaceBetUpdateV1 {
        room_id: params.room_id,
        pot_id,
        player: caller,
        bet_id,
        new_balance,
        new_locked,
        new_pot_total,
        new_current_bet: params.amount,
        new_current_better: caller,
    };
    Ok(darkfi_serial::serialize(&update))
}

pub(crate) fn game_room_place_bet_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    update: PlaceBetUpdateV1,
) -> ContractResult {
    msg!(
        "[PlaceBet] Update applied: bet {:?} by {:?}, pot {:?} now {}",
        update.bet_id,
        update.player,
        update.pot_id,
        update.new_pot_total
    );
    Ok(())
}