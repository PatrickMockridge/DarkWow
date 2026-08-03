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
    crypto::{pasta_prelude::PrimeField, poseidon_hash},
    error::ContractError,
    msg,
    wasm,
};
use dwow_sdk::pasta::pallas;

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
    let params = RevealRollParamsV1::decode(&self_.data[1..])?;

    msg!("[dice::reveal_roll] Processing roll reveal");

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;
    let bet: Bet = match wasm::db::db_get(bets_db, &params.bet_id.to_repr())? {
        Some(data) => Bet::decode(&data)?,
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

    // Collect block hashes across confirmation_depth for entropy.
    // Uses dwow_entropy_contract (ported from Mudra Arweave beacon).
    let verifying_height = wasm::util::get_verifying_block_height()?;
    let depth = bet.confirmation_depth as u64;
    let mut entropy_blocks = Vec::with_capacity(depth as usize);
    for i in 0..depth {
        let h = verifying_height.get().saturating_sub(i);
        let block_hash = wasm::util::get_block_hash(
            dwow_sdk::blockchain::BlockHeight::new(h),
        )?.0;
        entropy_blocks.push(dwow_entropy_contract::EntropyBlock { height: h, block_hash });
    }
    let seed = dwow_entropy_contract::derive_seed(&entropy_blocks);

    // Calculate roll from the seed
    let roll = calculate_roll_with_depth(pallas::Base::from(seed), bet.id, params.secret_nonce);

    msg!("[dice::reveal_roll] Calculated roll: {} (target: {})", roll, bet.target);

    // Validate roll is in range
    if roll >= ROLL_RANGE {
        return Err(DiceError::InvalidRoll.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Determine new state
    let new_state = if roll < bet.target { BetState::SettledPlayer } else { BetState::Revealed };

    // Create the update
    let update = RevealRollUpdateV1 {
        bet_id: bet.id,
        roll,
        state: new_state,
        revealed_at: current_block,
    };

    msg!("[dice::reveal_roll] Roll revealed successfully");
    Ok(update.encode())
}

/// Process update for RevealRollV1
pub fn dice_reveal_roll_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: RevealRollUpdateV1,
) -> Result<(), ContractError> {
    let bets_db = wasm::db::db_lookup(cid, DICE_CONTRACT_BETS_TREE)?;

    // Look up and update the bet
    let mut bet: Bet = match wasm::db::db_get(bets_db, &update.bet_id.to_repr())? {
        Some(data) => Bet::decode(&data)?,
        None => return Err(DiceError::BetNotFound.into()),
    };

    // Update bet state
    bet.roll = Some(update.roll);
    bet.state = update.state;
    bet.revealed_at = update.revealed_at;

    // Store updated bet
    wasm::db::db_set(bets_db, &update.bet_id.to_repr(), &bet.encode())?;

    msg!("[dice::reveal_roll::update] Bet updated");
    Ok(())
}
