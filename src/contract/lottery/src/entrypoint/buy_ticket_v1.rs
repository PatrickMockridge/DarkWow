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

//! BuyTicketV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES money_v3::transfer_v1 child calls to be bundled for
//! the actual token transfer to lock the ticket price.

use darkfi_sdk::{error::ContractError, msg, pasta::pallas, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::LotteryError;
use crate::model::{
    derive_nullifier, derive_ticket_id, BuyTicketParamsV1, BuyTicketUpdateV1, Ticket,
};
use crate::{
    LOTTERY_CONTRACT_CURRENT_LOTTERY, LOTTERY_CONTRACT_LATEST_TICKET_ROOT,
    LOTTERY_CONTRACT_LOTTERIES_TREE, LOTTERY_CONTRACT_NULLIFIERS_TREE,
    LOTTERY_CONTRACT_TICKETS_ROOTS_TREE, LOTTERY_CONTRACT_TICKETS_SMT_TREE,
    LOTTERY_CONTRACT_TICKETS_TREE,
};

/// Process instruction for BuyTicketV1
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for the actual token transfer to lock the ticket price.
pub fn lottery_buy_ticket_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: BuyTicketParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled for ticket price payment
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[BuyTicketV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(LotteryError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[BuyTicketV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(LotteryError::InvalidChildCall.into())
    }

    msg!("[lottery::buy_ticket] Processing ticket purchase");
    msg!("  player_pub: {:?}", params.player_pub);
    msg!("  value: {}", params.value);

    // Get current lottery ID
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    let current_lottery_id_bytes =
        wasm::db::db_get(lotteries_db, LOTTERY_CONTRACT_CURRENT_LOTTERY)?.unwrap();
    let lottery_id: pallas::Base = deserialize(&current_lottery_id_bytes)?;

    msg!("[lottery::buy_ticket] lottery_id: {:?}", lottery_id);

    // Get lottery state
    let lottery: crate::model::Lottery =
        deserialize(&wasm::db::db_get(lotteries_db, &serialize(&lottery_id))?.unwrap())?;

    // Verify lottery is active
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if !lottery.is_active(current_block) {
        return Err(LotteryError::LotteryNotActive.into())
    }

    // Verify value matches ticket price
    if params.value != lottery.config.ticket_price {
        return Err(LotteryError::ValueMismatch.into())
    }

    // Derive ticket ID
    let ticket_id =
        derive_ticket_id(lottery_id, &params.player_pub, params.commitment, params.value);

    msg!("[lottery::buy_ticket] Derived ticket_id");

    // Check if ticket already exists
    let tickets_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_TICKETS_TREE)?;
    if wasm::db::db_contains_key(tickets_db, &serialize(&ticket_id))? {
        return Err(LotteryError::TicketAlreadyClaimed.into())
    }

    // Derive nullifier
    // Note: We use commitment as part of nullifier derivation since it's unique per ticket
    let nullifier = derive_nullifier(ticket_id, params.commitment);

    // Check nullifier hasn't been used
    let nullifiers_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier))? {
        return Err(LotteryError::InvalidNullifier.into())
    }

    // Create the update
    let update = BuyTicketUpdateV1 {
        ticket_id,
        lottery_id,
        player_pub: params.player_pub,
        commitment: params.commitment,
        token_id: params.token_id,
        value: params.value,
        ticket_count: lottery.ticket_count + 1,
        gross_pool: lottery.gross_pool + params.value,
        nullifier,
        created_at: current_block,
    };

    msg!("[lottery::buy_ticket] Ticket purchased successfully");
    Ok(serialize(&update))
}

/// Process update for BuyTicketV1
pub fn lottery_buy_ticket_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: BuyTicketUpdateV1,
) -> Result<(), ContractError> {
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    let tickets_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_TICKETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_NULLIFIERS_TREE)?;
    let smt_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_TICKETS_SMT_TREE)?;
    let roots_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_TICKETS_ROOTS_TREE)?;

    // Insert ticket commitment into the SMT and update the Merkle root
    wasm::merkle::sparse_merkle_insert_batch(
        lotteries_db,
        smt_db,
        roots_db,
        LOTTERY_CONTRACT_LATEST_TICKET_ROOT,
        &[update.commitment],
    )?;

    // Read the new Merkle root from the info database
    let new_merkle_root_bytes =
        wasm::db::db_get(lotteries_db, LOTTERY_CONTRACT_LATEST_TICKET_ROOT)?.unwrap();
    let new_merkle_root: pallas::Base = deserialize(&new_merkle_root_bytes)?;

    msg!("[lottery::buy_ticket::update] Ticket SMT root: {:?}", new_merkle_root);

    // Get and update lottery
    let mut lottery: crate::model::Lottery =
        deserialize(&wasm::db::db_get(lotteries_db, &serialize(&update.lottery_id))?.unwrap())?;

    lottery.ticket_count = update.ticket_count;
    lottery.gross_pool = update.gross_pool;
    lottery.ticket_merkle_root = new_merkle_root;

    // Store updated lottery
    wasm::db::db_set(lotteries_db, &serialize(&update.lottery_id), &serialize(&lottery))?;
    msg!("[lottery::buy_ticket::update] Lottery updated with new Merkle root");

    // Create ticket state
    let ticket = Ticket {
        id: update.ticket_id,
        lottery_id: update.lottery_id,
        player_pub: update.player_pub,
        commitment: update.commitment,
        token_id: update.token_id,
        value: update.value,
        nullifier: update.nullifier,
        created_at: update.created_at,
    };

    // Store ticket
    wasm::db::db_set(tickets_db, &serialize(&update.ticket_id), &serialize(&ticket))?;
    msg!("[lottery::buy_ticket::update] Ticket stored in database");

    // Store nullifier
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;
    msg!("[lottery::buy_ticket::update] Nullifier stored");

    Ok(())
}
