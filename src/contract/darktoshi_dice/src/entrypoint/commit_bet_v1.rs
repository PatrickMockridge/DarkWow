/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! CommitBetV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES money_v3::transfer_v1 child calls to be bundled for
//! locking the player's bet value.

use darkfi_sdk::{
    error::ContractError,
    msg,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::DiceError;
use crate::model::{derive_bet_id, derive_nullifier, validate_house_edge, validate_target, Bet, BetState, CommitBetParamsV1, CommitBetUpdateV1};
use crate::DICE_CONTRACT_BETS_TREE;
use crate::DICE_CONTRACT_INFO_TREE;
use crate::DICE_CONTRACT_NULLIFIERS_TREE;
use crate::DICE_CONTRACT_HOUSE_EDGE;

/// Process instruction for CommitBetV1
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for locking the player's bet value.
pub fn dice_commit_bet_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CommitBetParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled for bet locking
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[CommitBetV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(DiceError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[CommitBetV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DiceError::InvalidChildCall.into())
    }

    msg!("[dice::commit_bet] Processing bet commitment");
    msg!("  player_pub: {:?}", params.player_pub);
    msg!("  bet_value: {}", params.bet_value);
    msg!("  target: {}", params.target);

    // Validate target
    validate_target(params.target)?;

    // Validate bet value
    if params.bet_value == 0 {
        return Err(DiceError::BetValueTooSmall.into())
    }

    // Look up house edge
    let info_db = wasm::db::db_lookup(cid, DICE_CONTRACT_INFO_TREE)?;
    let stored_house_edge_bytes = wasm::db::db_get(info_db, DICE_CONTRACT_HOUSE_EDGE)?.unwrap();
    let stored_house_edge: u32 = deserialize(&stored_house_edge_bytes)?;

    let house_edge = if params.house_edge == 0 { stored_house_edge } else { params.house_edge };
    validate_house_edge(house_edge)?;

    // Derive bet ID
    let bet_id = derive_bet_id(
        &params.player_pub,
        params.bet_value,
        params.target,
        params.secret_nonce,
        params.blind,
        params.token_id,
    );

    msg!("[dice::commit_bet] Derived bet_id");

    // Check if bet already exists
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    if wasm::db::db_contains_key(bets_db, &serialize(&bet_id))? {
        return Err(DiceError::BetAlreadyExists.into())
    }

    // Derive nullifier
    let nullifier = derive_nullifier(bet_id, params.secret_nonce);

    // Check nullifier hasn't been used
    let nullifiers_db = wasm::db::db_lookup(cid, DICE_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier))? {
        return Err(DiceError::DuplicateNullifier.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Calculate settle block: bet can only settle after confirmation_depth blocks
    let settle_block = current_block as u64 + params.confirmation_depth as u64;

    // Create the update
    let update = CommitBetUpdateV1 {
        bet_id,
        player_pub: params.player_pub,
        bet_value: params.bet_value,
        target: params.target,
        secret_nonce: params.secret_nonce,
        blind: params.blind,
        value_commit: params.value_commit,
        token_id: params.token_id,
        house_edge,
        confirmation_depth: params.confirmation_depth,
        settle_block,
        nullifier,
        created_at: current_block as u64,
    };

    msg!("[dice::commit_bet] Bet committed successfully");
    Ok(serialize(&update))
}

/// Process update for CommitBetV1
pub fn dice_commit_bet_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: CommitBetUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, DICE_CONTRACT_NULLIFIERS_TREE)?;

    // Create bet state
    let bet = Bet {
        id: update.bet_id,
        player_pub: update.player_pub,
        bet_value: update.bet_value,
        target: update.target,
        secret_nonce: update.secret_nonce,
        blind: update.blind,
        roll: None,
        state: BetState::Committed,
        house_edge: update.house_edge,
        confirmation_depth: update.confirmation_depth,
        created_at: update.created_at,
        revealed_at: 0,
        settle_block: update.settle_block,
        value_commit: update.value_commit,
        token_id: update.token_id,
        nullifier: update.nullifier,
    };

    // Store bet
    wasm::db::db_set(bets_db, &serialize(&update.bet_id), &serialize(&bet))?;
    msg!("[dice::commit_bet::update] Bet stored in database");

    // Store nullifier
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;
    msg!("[dice::commit_bet::update] Nullifier stored");

    Ok(())
}
