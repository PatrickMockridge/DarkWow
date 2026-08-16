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

use crate::error::DiceError;
use crate::model::{Bet, BetState, HouseCloseParamsV1, HouseCloseUpdateV1};
use crate::{
    DICE_CONTRACT_BETS_TREE, DICE_CONTRACT_HOUSE_TREE,
    DICE_CONTRACT_INFO_TREE, DICE_CONTRACT_NULLIFIERS_TREE,
    DICE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    DICE_CONTRACT_ROLL_TIMEOUT,
};

/// Process instruction for HouseCloseV1
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for collecting the house's share.
pub fn dice_house_close_process_instruction_v1(
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
        return Err(DiceError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[HouseCloseV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DiceError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DICE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DICE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DiceError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    msg!("[dice::house_close] Processing house close");

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &params.bet_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let bet: Bet = Bet::decode(&bet_bytes)?;

    msg!("[dice::house_close] Found bet, current state: {:?}", bet.state as u8);

    // Get roll timeout from info
    let info_db = wasm::db::db_lookup(cid, DICE_CONTRACT_INFO_TREE)?;
    let timeout_bytes = wasm::db::db_get(info_db, DICE_CONTRACT_ROLL_TIMEOUT)?.ok_or(ContractError::DbGetEmpty)?;
    let roll_timeout: u32 = u32::from_le_bytes(timeout_bytes.try_into().map_err(|e| ContractError::IoError(format!("{e:?}")))?);

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Check if timeout has been reached
    let blocks_since_creation = current_block.get().saturating_sub(bet.created_at);
    let timeout_reached = blocks_since_creation >= u64::from(roll_timeout);

    msg!(
        "[dice::house_close] Timeout: {}, Current: {}, Timeout reached: {}",
        roll_timeout,
        current_block,
        timeout_reached
    );

    // Bet can only be closed if Committed and timeout reached, or Revealed
    let can_close = match bet.state {
        BetState::Committed if timeout_reached => true,
        BetState::Revealed => true,
        _ => false,
    };

    if !can_close {
        if bet.state == BetState::Committed && !timeout_reached {
            return Err(DiceError::RollTimeoutNotReached.into())
        }
        return Err(DiceError::InvalidStateTransition.into())
    }

    // Verify the house is the one closing (ZK-based authorization)
    // The host-side ZK verification ensures house knows secret matching stored pubkey
    let info_db = wasm::db::db_lookup(cid, DICE_CONTRACT_INFO_TREE)?;
    let house_pubkey_bytes =
        wasm::db::db_get(info_db, crate::DICE_CONTRACT_HOUSE_PUBKEY)?;

    let stored_house_pubkey: dwow_sdk::crypto::PublicKey = match house_pubkey_bytes {
        Some(bytes) => dwow_sdk::crypto::PublicKey::from_bytes(bytes.try_into().map_err(|e| ContractError::IoError(format!("{e:?}")))?)?,
        None => return Err(DiceError::UnauthorizedCaller.into()),
    };

    // Verify the provided house pubkey coordinates match stored value
    let (stored_x, stored_y) = stored_house_pubkey.xy().expect("pk not identity");
    if params.house_pub_x != stored_x || params.house_pub_y != stored_y {
        msg!("[dice::house_close] Error: House pubkey does not match stored value");
        return Err(DiceError::UnauthorizedCaller.into())
    }

    // Verify close_nullifier hasn't been used (ZK proof verifies it's correctly derived)
    let nullifiers_db = wasm::db::db_lookup(cid, DICE_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.close_nullifier.to_repr())? {
        return Err(DiceError::DuplicateNullifier.into())
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(bet.bet_value),
        bet.id,
    ]);
    validate_child_value_commit(&child_call.data, bet.bet_value, value_blind)?;

    // Advance the carried bet + house balance in exec (apply re-stores them, no db_get-in-apply).
    let mut updated_bet = bet.clone();
    updated_bet.state = BetState::Cancelled;
    let house_take = updated_bet.calculate_house_take().ok_or(DiceError::ArithmeticOverflow)?;
    let house_db = wasm::db::db_lookup(cid, DICE_CONTRACT_HOUSE_TREE)?;
    let mut house_balance: u64 = 0;
    if wasm::db::db_contains_key(house_db, b"balance")? {
        if let Some(bal_bytes) = wasm::db::db_get(house_db, b"balance")? {
            house_balance = u64::from_le_bytes(bal_bytes.try_into().map_err(|e| ContractError::IoError(format!("{e:?}")))?);
        }
    }
    house_balance += house_take;

    // Create the update
    let update = HouseCloseUpdateV1 { bet_id: bet.id, close_nullifier: params.close_nullifier, bet: updated_bet, house_balance };

    msg!("[dice::house_close] House close approved");
    Ok(update.encode())
}

/// Process update for HouseCloseV1
pub fn dice_house_close_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: HouseCloseUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let house_db = wasm::db::db_lookup(cid, DICE_CONTRACT_HOUSE_TREE)?;

    // Re-store the carried bet + house balance (already advanced in exec).
    wasm::db::db_set(bets_db, &update.bet_id.to_repr(), &update.bet.encode())?;
    wasm::db::db_set(house_db, b"balance", &update.house_balance.to_le_bytes())?;

    // Record close nullifier to prevent replay
    let nullifiers_db = wasm::db::db_lookup(cid, DICE_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.close_nullifier.to_repr())?;

    msg!("[dice::house_close::update] House collected");
    Ok(())
}
