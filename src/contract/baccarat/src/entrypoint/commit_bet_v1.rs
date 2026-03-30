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
    crypto::pasta_prelude::Group,
    error::ContractError,
    msg,
    wasm,
    pasta::pallas,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::BaccaratError;
use crate::model::{
    derive_bet_id, derive_nullifier, Bet, BetState, BetType, CommitBetParamsV1, CommitBetUpdateV1,
};
use crate::{
    BACCARAT_CONTRACT_BETS_TREE, BACCARAT_CONTRACT_HOUSE_EDGE,
    BACCARAT_CONTRACT_INFO_TREE, BACCARAT_CONTRACT_NULLIFIERS_TREE,
    DEFAULT_HOUSE_EDGE, MAX_BET_VALUE, MAX_CONFIRMATION_DEPTH, MIN_BET_VALUE,
};

/// Validate bet type
fn validate_bet_type(bet_type: u8) -> Result<(), ContractError> {
    match BetType::from_u8(bet_type) {
        Some(_) => Ok(()),
        None => Err(BaccaratError::InvalidBetType.into()),
    }
}

/// Validate bet value is within bounds
fn validate_bet_value(bet_value: u64) -> Result<(), ContractError> {
    if bet_value < MIN_BET_VALUE {
        return Err(BaccaratError::BetValueTooSmall.into())
    }
    if bet_value > MAX_BET_VALUE {
        return Err(BaccaratError::BetValueTooLarge.into())
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

/// Verify value commitment is not identity (placeholder - real implementation
/// should verify the Pedersen commitment properly)
fn verify_value_commit(value_commit: pallas::Point) -> Result<(), ContractError> {
    if value_commit.is_identity().into() {
        return Err(BaccaratError::ValueCommitmentMismatch.into())
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

    // Verify value commitment is valid (not identity)
    verify_value_commit(params.value_commit)?;

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

    // Get current block height for created_at and settle_block
    let current_block = wasm::util::get_verifying_block_height()?;
    let created_at = current_block as u64;

    // Calculate settle block: bet can only settle after confirmation_depth blocks
    let settle_block = created_at + params.confirmation_depth as u64;

    // Derive nullifier using standalone function
    let nullifier = derive_nullifier(bet_id, params.secret_nonce);

    // Check nullifier hasn't been used
    let nullifiers_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier))? {
        return Err(BaccaratError::DuplicateNullifier.into())
    }

    // Create the update with all bet data needed to persist
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
        created_at,
    };

    msg!("[baccarat::commit_bet] Bet committed successfully");
    Ok(serialize(&update))
}

/// Process update for CommitBetV1 - persists the bet to database
pub fn baccarat_commit_bet_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: CommitBetUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_NULLIFIERS_TREE)?;

    // Create the bet struct to persist
    let bet = Bet {
        id: update.bet_id,
        player_pub: update.player_pub,
        bet_type: update.bet_type,
        bet_value: update.bet_value,
        secret_nonce: update.secret_nonce,
        blind: update.blind,
        player_hand: None,
        banker_hand: None,
        player_third_card: None,
        banker_third_card: None,
        outcome: None,
        state: update.state,
        house_edge: update.house_edge,
        confirmation_depth: update.confirmation_depth,
        created_at: update.created_at,
        settle_block: update.settle_block,
        value_commit: update.value_commit,
        token_id: update.token_id,
        nullifier: update.nullifier,
    };

    // Store the bet
    wasm::db::db_set(bets_db, &serialize(&bet.id), &serialize(&bet))?;

    // Store the nullifier to prevent double-spending
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &serialize(&update.nullifier))?;

    msg!("[baccarat::commit_bet::update] Bet persisted to database");
    Ok(())
}