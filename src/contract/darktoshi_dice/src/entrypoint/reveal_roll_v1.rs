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

//! RevealRollV1 Implementation

use dwow_sdk::{
    crypto::poseidon_hash,
    error::ContractError,
    msg,
    wasm,
};
use dwow_sdk::pasta::pallas;
use dwow_serial::{deserialize, serialize};

use crate::error::DiceError;
use crate::model::{calculate_roll_with_depth, Bet, BetState, RevealRollParamsV1, RevealRollUpdateV1};
use crate::DICE_CONTRACT_BETS_TREE;
use crate::ROLL_RANGE;

/// Process instruction for RevealRollV1
pub fn dice_reveal_roll_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: RevealRollParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[dice::reveal_roll] Processing roll reveal");

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let bet: Bet = match wasm::db::db_get(bets_db, &serialize(&params.bet_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(DiceError::BetNotFound.into()),
    };

    msg!("[dice::reveal_roll] Found bet, current state: {:?}", bet.state as u8);

    // Verify bet is in Committed state
    if bet.state != BetState::Committed {
        return Err(DiceError::InvalidStateTransition.into())
    }

    // Verify ZK proof: prover knows secret_nonce matching stored commitment
    // The host-side ZK verification ensures H(secret_nonce) == secret_nonce_commit
    // We verify the commitment matches the bet's stored value
    let secret_nonce_commit = poseidon_hash([params.secret_nonce]);
    if secret_nonce_commit != bet.secret_nonce_commit {
        return Err(DiceError::CommitmentMismatch.into())
    }

    // Get block hash for randomness (use verifying block hash, not tx_hash)
    // tx_hash is player-influenced and breaks the randomness guarantee
    let verifying_height = wasm::util::get_verifying_block_height()?;
    let block_hash = wasm::util::get_block_hash(verifying_height)?.0;

    // Convert block hash bytes to pallas::Base for the depth-based roll calculation
    let block_hash_a = u64::from_le_bytes(block_hash[0..8].try_into().unwrap());
    let block_hash_b = u64::from_le_bytes(block_hash[8..16].try_into().unwrap());
    let block_hash_c = u64::from_le_bytes(block_hash[16..24].try_into().unwrap());
    let block_hash_d = u64::from_le_bytes(block_hash[24..32].try_into().unwrap());

    let block_hashes = vec![
        pallas::Base::from(block_hash_a),
        pallas::Base::from(block_hash_b),
        pallas::Base::from(block_hash_c),
        pallas::Base::from(block_hash_d),
    ];

    // Calculate roll using depth-based approach
    let roll = calculate_roll_with_depth(&block_hashes, bet.id, params.secret_nonce);

    msg!("[dice::reveal_roll] Calculated roll: {} (target: {})", roll, bet.target);

    // Validate roll is in range
    if roll >= ROLL_RANGE {
        return Err(DiceError::InvalidRoll.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Determine new state
    let new_state = if roll < bet.target { BetState::SettledPlayer } else { BetState::Revealed };

    // Create the update
    let update = RevealRollUpdateV1 {
        bet_id: bet.id,
        roll,
        state: new_state,
        revealed_at: current_block as u64,
    };

    msg!("[dice::reveal_roll] Roll revealed successfully");
    Ok(serialize(&update))
}

/// Process update for RevealRollV1
pub fn dice_reveal_roll_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: RevealRollUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;

    // Look up and update the bet
    let mut bet: Bet = match wasm::db::db_get(bets_db, &serialize(&update.bet_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(DiceError::BetNotFound.into()),
    };

    // Update bet state
    bet.roll = Some(update.roll);
    bet.state = update.state;
    bet.revealed_at = update.revealed_at;

    // Store updated bet
    wasm::db::db_set(bets_db, &serialize(&update.bet_id), &serialize(&bet))?;

    msg!("[dice::reveal_roll::update] Bet updated");
    Ok(())
}
