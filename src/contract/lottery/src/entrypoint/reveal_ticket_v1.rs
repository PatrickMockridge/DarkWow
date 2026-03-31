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

//! RevealTicketV1 Implementation

use darkfi_sdk::{error::ContractError, msg, pasta::pallas, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::LotteryError;
use crate::model::{
    count_matches, determine_tier, validate_numbers, RevealTicketParamsV1, RevealTicketUpdateV1,
};
use crate::LOTTERY_CONTRACT_LOTTERIES_TREE;
use crate::LOTTERY_CONTRACT_TICKETS_TREE;

/// Process instruction for RevealTicketV1
pub fn lottery_reveal_ticket_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: RevealTicketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[lottery::reveal_ticket] Revealing ticket: {:?}", params.ticket_id);

    // Get ticket first to find lottery_id
    let tickets_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_TICKETS_TREE)?;
    let ticket: crate::model::Ticket =
        deserialize(&wasm::db::db_get(tickets_db, &serialize(&params.ticket_id))?.unwrap())?;

    // Get lottery state
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    let lottery: crate::model::Lottery =
        deserialize(&wasm::db::db_get(lotteries_db, &serialize(&ticket.lottery_id))?.unwrap())?;

    // Verify lottery is in winners drawn state
    if lottery.state != crate::model::LotteryState::WinnersDrawn {
        return Err(LotteryError::LotteryNotActive.into())
    }

    // Verify claim period hasn't expired
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block > lottery.claim_deadline {
        return Err(LotteryError::ClaimPeriodExpired.into())
    }

    // Verify winning numbers exist
    let winning_numbers = lottery.winning_numbers.as_ref().ok_or(LotteryError::LotteryNotFound)?;

    // Verify ticket belongs to this lottery
    if ticket.lottery_id != lottery.id {
        return Err(LotteryError::TicketNotFound.into())
    }

    // Validate the revealed numbers
    validate_numbers(&params.numbers, lottery.config.num_picks, lottery.config.number_range)?;

    // Verify commitment matches using iterative hashing
    let mut state = ticket.lottery_id;
    for &n in &params.numbers {
        state = darkfi_sdk::crypto::poseidon_hash([state, pallas::Base::from(n as u64)]);
    }
    let computed_commitment = darkfi_sdk::crypto::poseidon_hash([state, params.nonce]);

    if computed_commitment != ticket.commitment {
        return Err(LotteryError::InvalidCommitment.into())
    }

    // Count matches
    let matches = count_matches(&params.numbers, winning_numbers);

    msg!("[lottery::reveal_ticket] Ticket matched {} numbers", matches);

    // Determine prize tier
    let tier = determine_tier(&lottery.config, matches);

    // Create the update
    let update = RevealTicketUpdateV1 { ticket_id: params.ticket_id, matches, tier: tier.map(|t| t as u8) };

    msg!("[lottery::reveal_ticket] Reveal processed successfully");
    Ok(serialize(&update))
}

/// Process update for RevealTicketV1
pub fn lottery_reveal_ticket_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: RevealTicketUpdateV1,
) -> Result<(), ContractError> {
    // The reveal just verifies the ticket and calculates matches.
    // Actual prize claiming happens in ClaimPrizeV1.
    // We could store the reveal proof here if needed for ZK verification.

    msg!("[lottery::reveal_ticket::update] Reveal recorded for ticket: {:?}", update.ticket_id);
    msg!("[lottery::reveal_ticket::update] Matches: {}, Tier: {:?}", update.matches, update.tier);

    Ok(())
}
