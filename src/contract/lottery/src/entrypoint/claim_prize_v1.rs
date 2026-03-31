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

//! ClaimPrizeV1 Implementation

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::LotteryError;
use crate::model::{Claim, ClaimPrizeParamsV1, ClaimPrizeUpdateV1};
use crate::LOTTERY_CONTRACT_CLAIMS_TREE;
use crate::LOTTERY_CONTRACT_LOTTERIES_TREE;
use crate::LOTTERY_CONTRACT_TICKETS_TREE;

/// Process instruction for ClaimPrizeV1
pub fn lottery_claim_prize_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClaimPrizeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[lottery::claim_prize] Claiming prize for ticket: {:?}", params.ticket_id);

    // First get the ticket to find the lottery_id
    let tickets_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_TICKETS_TREE)?;
    let ticket: crate::model::Ticket =
        deserialize(&wasm::db::db_get(tickets_db, &serialize(&params.ticket_id))?.unwrap())?;

    // Get lottery state
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    let lottery: crate::model::Lottery = deserialize(
        &wasm::db::db_get(lotteries_db, &serialize(&ticket.lottery_id))?.unwrap(),
    )?;

    // Verify lottery is in winners drawn state
    if lottery.state != crate::model::LotteryState::WinnersDrawn {
        return Err(LotteryError::LotteryNotActive.into())
    }

    // Verify claim period hasn't expired
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block > lottery.claim_deadline {
        return Err(LotteryError::ClaimPeriodExpired.into())
    }

    // Ticket already retrieved above

    // Check if already claimed
    let claims_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_CLAIMS_TREE)?;
    if wasm::db::db_contains_key(claims_db, &serialize(&params.ticket_id))? {
        return Err(LotteryError::PrizeAlreadyClaimed.into())
    }

    // Get winning numbers
    let winning_numbers = lottery.winning_numbers.as_ref().ok_or(LotteryError::LotteryNotFound)?;

    // Count matches - we need to recalculate since reveal info isn't stored
    // In a full implementation, we'd store reveal info or include it in params
    // For now, we'll trust the ZK proof verifies this

    // For a proper implementation, we would verify the ZK proof here:
    // wasm::zkas_verify(params.proof, ...)?

    // Calculate prize based on tier
    // This is simplified - in reality we'd need to track how many winners per tier
    // and the ZK proof should verify the matches

    let prize = calculate_prize(&lottery, ticket.value)?;

    // Create the update
    let update = ClaimPrizeUpdateV1 {
        ticket_id: params.ticket_id,
        tier: 0, // Would come from proof verification
        matches: 0, // Would come from proof verification
        prize,
        claimed_at: current_block,
    };

    msg!("[lottery::claim_prize] Prize claimed successfully: {}", prize);
    Ok(serialize(&update))
}

/// Process update for ClaimPrizeV1
pub fn lottery_claim_prize_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ClaimPrizeUpdateV1,
) -> Result<(), ContractError> {
    let claims_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_CLAIMS_TREE)?;

    // Create claim record
    let claim = Claim {
        ticket_id: update.ticket_id,
        tier: update.tier,
        matches: update.matches,
        prize: update.prize,
        claimed_at: update.claimed_at,
    };

    // Store claim
    wasm::db::db_set(claims_db, &serialize(&update.ticket_id), &serialize(&claim))?;
    msg!("[lottery::claim_prize::update] Claim stored for ticket: {:?}", update.ticket_id);

    Ok(())
}

/// Calculate prize for a winning ticket
/// Note: This is simplified - proper implementation needs to track winners per tier
fn calculate_prize(lottery: &crate::model::Lottery, _ticket_value: u64) -> Result<u64, ContractError> {
    // For now, return the prize pool divided equally among winners
    // A proper implementation would:
    // 1. Count total winners per tier
    // 2. Calculate prize pool percentage per tier
    // 3. Divide by number of winners in that tier
    // 4. Use ZK proof to verify the claim is valid

    // Simplified: return some portion of the prize pool
    // This should be tied to actual matches from ZK proof
    if lottery.prize_pool == 0 {
        return Ok(0)
    }

    // For demo purposes, just return a share
    // Real implementation needs ZK proof verification to determine actual matches
    Ok(lottery.prize_pool / 10) // Simplified - 10 winners assumed
}
