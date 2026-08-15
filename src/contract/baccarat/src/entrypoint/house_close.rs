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

//! HouseCloseV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
//! collecting the house's share of the bet.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::deserialize;
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};

use crate::error::BaccaratError;
use crate::model::{calculate_house_take, Bet, BetState, HouseCloseParamsV1, HouseCloseUpdateV1};
use crate::{
    BACCARAT_CONTRACT_BETS_TREE, BACCARAT_CONTRACT_HOUSE_PUBKEY,
    BACCARAT_CONTRACT_INFO_TREE, BACCARAT_CONTRACT_NULLIFIERS_TREE,
    BACCARAT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
};

/// Process instruction for HouseCloseV1
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for collecting the house's share.
pub fn baccarat_house_close_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = HouseCloseParamsV1::decode(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for house's share
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[HouseCloseV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(BaccaratError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[HouseCloseV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(BaccaratError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, BACCARAT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(BaccaratError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    msg!("[baccarat::house_close] Closing bet_id: {:?}", params.bet_id);

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &params.bet_id.to_repr())?
        .ok_or(BaccaratError::BetNotFound)?;

    let mut bet = Bet::decode(&bet_bytes)?;

    // Verify bet is in correct state
    // Can close if: Committed (timeout reached) or CardsDrawn
    match bet.state {
        BetState::Committed | BetState::CardsDrawn => {}
        _ => return Err(BaccaratError::InvalidStateTransition.into()),
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // If bet is still Committed, verify timeout has been reached
    if bet.state == BetState::Committed && current_block.get() < bet.settle_block {
        return Err(BaccaratError::BetTimeoutNotReached.into())
    }

    // Verify the house is the one closing (ZK-based authorization)
    // The host-side ZK verification ensures house knows secret matching stored pubkey
    let info_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_INFO_TREE)?;
    let house_pubkey_bytes =
        wasm::db::db_get(info_db, BACCARAT_CONTRACT_HOUSE_PUBKEY)?;

    let stored_house_pubkey: dwow_sdk::crypto::PublicKey = match house_pubkey_bytes {
        Some(bytes) => deserialize(&bytes)?,
        None => return Err(BaccaratError::UnauthorizedCaller.into()),
    };

    // Verify the provided house pubkey coordinates match stored value
    let (stored_x, stored_y) = stored_house_pubkey.xy().expect("pk not identity");
    if params.house_pub_x != stored_x || params.house_pub_y != stored_y {
        msg!("[baccarat::house_close] Error: House pubkey does not match stored value");
        return Err(BaccaratError::UnauthorizedCaller.into())
    }

    // Verify close_nullifier hasn't been used (ZK proof verifies it's correctly derived)
    let nullifiers_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.close_nullifier.to_repr())? {
        return Err(BaccaratError::DuplicateNullifier.into())
    }

    // Calculate house's take (player's bet value)
    let house_take = calculate_house_take(&bet);

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(house_take),
        bet.id,
    ]);
    validate_child_value_commit(&child_call.data, house_take, value_blind)?;

    msg!("[baccarat::house_close] House take: {}", house_take);

    // Advance state and carry the full bet to apply
    bet.state = BetState::Cancelled;

    let update = HouseCloseUpdateV1 {
        bet,
        close_nullifier: params.close_nullifier,
    };

    msg!("[baccarat::house_close] Bet closed by house");
    Ok(update.encode())
}

/// Process update for HouseCloseV1 - persists the cancelled bet + records nullifier
pub fn baccarat_house_close_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: HouseCloseUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;

    wasm::db::db_set(bets_db, &update.bet.id.to_repr(), &update.bet.encode())?;

    // Record close nullifier to prevent replay
    let nullifiers_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.close_nullifier.to_repr())?;

    msg!("[baccarat::house_close::update] Bet state updated to Cancelled");
    Ok(())
}