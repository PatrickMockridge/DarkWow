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

//! SettleBetV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
//! paying out winnings to the player.

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
use crate::model::{calculate_payout, Bet, BetState, SettleBetParamsV1, SettleBetUpdateV1};
use crate::{
    BACCARAT_CONTRACT_BETS_TREE, BACCARAT_CONTRACT_INFO_TREE,
    BACCARAT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
};

/// Process instruction for SettleBetV1
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for paying out winnings to the player.
pub fn baccarat_settle_bet_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = SettleBetParamsV1::decode(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for payout
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[SettleBetV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(BaccaratError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[SettleBetV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
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

    msg!("[baccarat::settle_bet] Settling bet_id: {:?}", params.bet_id);

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &params.bet_id.to_repr())?
        .ok_or(BaccaratError::BetNotFound)?;

    let mut bet = Bet::decode(&bet_bytes)?;

    // Verify bet is in CardsDrawn state
    if bet.state != BetState::CardsDrawn {
        return Err(BaccaratError::InvalidStateTransition.into())
    }

    // Verify we have an outcome
    let outcome = bet.outcome.ok_or(BaccaratError::CardsNotDrawn)?;

    // Calculate payout
    let payout = calculate_payout(&bet, outcome);

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(payout),
        bet.id,
    ]);
    validate_child_value_commit(&child_call.data, payout, value_blind)?;

    msg!("[baccarat::settle_bet] Calculated payout: {}", payout);

    // Advance state and carry the full bet to apply
    bet.state = BetState::Settled;

    let update = SettleBetUpdateV1 { bet };

    msg!("[baccarat::settle_bet] Bet settled successfully");
    Ok(update.encode())
}

/// Process update for SettleBetV1 - persists the settled bet state
pub fn baccarat_settle_bet_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: SettleBetUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;

    wasm::db::db_set(bets_db, &update.bet.id.to_repr(), &update.bet.encode())?;

    msg!("[baccarat::settle_bet::update] Bet state updated to Settled");
    Ok(())
}
