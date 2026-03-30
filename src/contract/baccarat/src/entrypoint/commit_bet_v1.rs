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

use darkfi_sdk::{
    error::ContractError,
    msg,
    wasm,
    pasta::pallas,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::BaccaratError;
use crate::model::{
    derive_bet_id, Bet, BetState, BetType, CommitBetParamsV1, CommitBetUpdateV1,
};
use crate::{
    BACCARAT_CONTRACT_BETS_TREE, BACCARAT_CONTRACT_HOUSE_EDGE, BACCARAT_CONTRACT_INFO_TREE,
    BACCARAT_CONTRACT_NULLIFIERS_TREE, DEFAULT_HOUSE_EDGE, MAX_CONFIRMATION_DEPTH,
};

/// Validate bet type
fn validate_bet_type(bet_type: u8) -> Result<(), ContractError> {
    match BetType::from_u8(bet_type) {
        Some(_) => Ok(()),
        None => Err(BaccaratError::InvalidBetType.into()),
    }
}

/// Validate bet value
fn validate_bet_value(bet_value: u64) -> Result<(), ContractError> {
    if bet_value == 0 {
        return Err(BaccaratError::BetValueTooSmall.into())
    }
    Ok(())
}

/// Validate house edge
fn validate_house_edge(house_edge: u32) -> Result<(), ContractError> {
    if house_edge < 100 || house_edge > 300 {
        return Err(BaccaratError::InvalidHouseEdge.into())
    }
    Ok(())
}

/// Validate confirmation depth
fn validate_confirmation_depth(depth: u8) -> Result<(), ContractError> {
    if depth == 0 || depth > MAX_CONFIRMATION_DEPTH {
        return Err(BaccaratError::InvalidConfirmationDepth.into())
    }
    Ok(())
}

/// Process instruction for CommitBetV1
pub fn baccarat_commit_bet_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: CommitBetParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[baccarat::commit_bet] Processing bet commitment");
    msg!("  player_pub: {:?}", params.player_pub);
    msg!("  bet_value: {}", params.bet_value);
    msg!("  bet_type: {}", params.bet_type);

    // Validate bet type
    validate_bet_type(params.bet_type)?;

    // Validate bet value
    validate_bet_value(params.bet_value)?;

    // Validate confirmation depth
    validate_confirmation_depth(params.confirmation_depth)?;

    // Look up house edge from contract info or use default
    let info_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_INFO_TREE)?;
    let stored_house_edge_bytes = wasm::db::db_get(info_db, BACCARAT_CONTRACT_HOUSE_EDGE)?
        .unwrap_or_else(|| serialize(&DEFAULT_HOUSE_EDGE));
    let stored_house_edge: u32 = deserialize(&stored_house_edge_bytes)?;

    let house_edge = if params.house_edge == 0 { stored_house_edge } else { params.house_edge };
    validate_house_edge(house_edge)?;

    // Derive bet ID
    let bet_id = derive_bet_id(
        &params.player_pub,
        params.bet_type,
        params.bet_value,
        params.secret_nonce,
        params.blind,
        params.token_id,
    );

    msg!("[baccarat::commit_bet] Derived bet_id: {:?}", bet_id);

    // Check if bet already exists
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;
    if wasm::db::db_contains_key(bets_db, &serialize(&bet_id))? {
        return Err(BaccaratError::BetAlreadyExists.into())
    }

    // Derive nullifier
    let nullifier = Bet::derive_nullifier(&Bet {
        id: bet_id,
        player_pub: params.player_pub,
        bet_type: params.get_bet_type().unwrap(),
        bet_value: params.bet_value,
        secret_nonce: params.secret_nonce,
        blind: params.blind,
        player_hand: None,
        banker_hand: None,
        player_third_card: None,
        banker_third_card: None,
        outcome: None,
        state: BetState::Committed,
        house_edge,
        confirmation_depth: params.confirmation_depth,
        created_at: 0,
        settle_block: 0,
        value_commit: params.value_commit,
        token_id: params.token_id,
        nullifier: pallas::Base::zero(),
    });

    // Check nullifier hasn't been used
    let nullifiers_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier))? {
        return Err(BaccaratError::DuplicateNullifier.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Calculate settle block: bet can only settle after confirmation_depth blocks
    let settle_block = current_block as u64 + params.confirmation_depth as u64;

    // Create the update
    let update = CommitBetUpdateV1 {
        bet_id,
        player_pub: params.player_pub,
        bet_type: params.get_bet_type().unwrap(),
        bet_value: params.bet_value,
        secret_nonce: params.secret_nonce,
        blind: params.blind,
        house_edge,
        confirmation_depth: params.confirmation_depth,
        token_id: params.token_id,
        value_commit: params.value_commit,
        settle_block,
        nullifier,
        state: BetState::Committed,
    };

    msg!("[baccarat::commit_bet] Bet committed successfully");
    Ok(serialize(&update))
}

/// Process update for CommitBetV1
pub fn baccarat_commit_bet_process_update_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    _update: CommitBetUpdateV1,
) -> Result<(), ContractError> {
    // Note: In a real implementation, the update would contain all necessary
    // data to recreate the bet. For now, this is a placeholder.
    // The instruction phase should store the full bet, and this just confirms.

    msg!("[baccarat::commit_bet::update] Bet committed confirmed");
    Ok(())
}
