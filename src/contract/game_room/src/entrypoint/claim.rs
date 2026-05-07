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

//! ClaimV1 entrypoint - Winner claims their share of a settled pot
//!
//! ## Money Integration
//!
//! This function REQUIRES money_v3::transfer_v1 child calls to be bundled for
//! distributing the prize payout. The child call transfers the payout_amount
//! to the winner's public key.
//!
//! ## Flow
//!
//! 1. Owner calls `settle_pot` to determine winners and payouts
//! 2. Each winner calls `claim` to receive their payout
//! 3. The claim must bundle money_v3::transfer_v1 for actual token transfer

use dwow_sdk::{
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    wasm, ContractCall,
};

use crate::{
    error::GameRoomError,
    model::{ClaimParamsV1, ClaimUpdateV1, PlayerAccount, Pot, PotState},
    GAME_ROOM_ACCOUNTS_TREE, GAME_ROOM_NULLIFIERS_TREE,
    GAME_ROOM_POTS_TREE, GAME_ROOM_ROOMS_TREE,
};

pub(crate) fn game_room_claim_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClaimParamsV1 = dwow_serial::deserialize(&self_.data[1..])?;

    msg!(
        "[Claim] Claiming pot {:?} in room {:?} for winner {:?}, amount: {}",
        params.pot_id,
        params.room_id,
        params.winner,
        params.payout_amount
    );

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled
    // The child call should transfer params.payout_amount to params.winner
    let children = &calls[call_idx].children_indexes;
    if children.len() != 1 {
        msg!(
            "[Claim] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            children.len()
        );
        return Err(GameRoomError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1
    let child_idx = children[0];
    let child_call = &calls[child_idx].data;
    // money_v3::transfer_v1 function code is 0x04
    if child_call.data[0] != 0x04 {
        msg!(
            "[Claim] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(GameRoomError::InvalidChildCall.into())
    }

    // Get room
    let rooms_db = wasm::db::db_lookup(cid, GAME_ROOM_ROOMS_TREE)?;
    let Some(_room_data) =
        wasm::db::db_get(rooms_db, &dwow_serial::serialize(&params.room_id))?
    else {
        msg!("[Claim] Error: Room not found");
        return Err(GameRoomError::RoomNotFound.into())
    };

    // Get pot
    let pots_db = wasm::db::db_lookup(cid, GAME_ROOM_POTS_TREE)?;
    let Some(pot_data) = wasm::db::db_get(pots_db, &dwow_serial::serialize(&params.pot_id))?
    else {
        msg!("[Claim] Error: Pot not found");
        return Err(GameRoomError::PotNotFound.into())
    };
    let pot: Pot =
        dwow_serial::deserialize(&pot_data)?;

    // Validate pot state - must be settled
    if pot.state != PotState::Settled {
        msg!("[Claim] Error: Pot not settled");
        return Err(GameRoomError::PotSettled.into())
    }

    // Check nullifier to prevent double-claim
    let nullifiers_db = wasm::db::db_lookup(cid, GAME_ROOM_NULLIFIERS_TREE)?;
    let claim_key = dwow_serial::serialize(&(params.pot_id, params.winner.xy().0));
    if wasm::db::db_contains_key(nullifiers_db, &claim_key)? {
        msg!("[Claim] Error: Already claimed");
        return Err(GameRoomError::AlreadyClaimed.into())
    }

    // Validate payout_amount against the pot total
    // The payout must not exceed the pot total
    if params.payout_amount > pot.total {
        msg!(
            "[Claim] Error: Payout {} exceeds pot total {}",
            params.payout_amount,
            pot.total
        );
        return Err(GameRoomError::InvalidAmount.into())
    }

    // Get winner's account
    let accounts_db = wasm::db::db_lookup(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    let account_key = dwow_serial::serialize(&(params.room_id, params.winner.xy().0));
    let Some(account_data) = wasm::db::db_get(accounts_db, &account_key)? else {
        msg!("[Claim] Error: Account not found");
        return Err(GameRoomError::AccountNotFound.into())
    };
    let mut account: PlayerAccount =
        dwow_serial::deserialize(&account_data)?;

    // Calculate new balance - add winnings to balance
    // Note: In a full implementation with locked funds tracking, we would unlock
    // the winner's contribution to the pot here.
    let winnings = params.payout_amount;
    account.balance += winnings;

    let new_balance = account.balance;

    // Store updated account
    wasm::db::db_set(accounts_db, &account_key, &dwow_serial::serialize(&account))?;

    // Record nullifier to prevent double-claim
    wasm::db::db_set(nullifiers_db, &claim_key, &[])?;

    msg!(
        "[Claim] Claim prepared: winner {:?} will receive {} (new balance: {})",
        params.winner,
        winnings,
        new_balance
    );

    let update = ClaimUpdateV1 {
        room_id: params.room_id,
        pot_id: params.pot_id,
        winner: params.winner,
        amount: winnings,
        new_balance,
    };
    Ok(dwow_serial::serialize(&update))
}

pub(crate) fn game_room_claim_process_update_v1(
    _cid: dwow_sdk::crypto::ContractId,
    update: ClaimUpdateV1,
) -> ContractResult {
    msg!(
        "[Claim] Update applied: winner {:?} claimed {} from pot {:?}, new balance {}",
        update.winner,
        update.amount,
        update.pot_id,
        update.new_balance
    );
    Ok(())
}