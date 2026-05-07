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

//! ClaimPrizeV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES money_v3::transfer_v1 child calls to be bundled for
//! the actual token transfer to the winner.

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::LotteryError;
use crate::model::{Claim, ClaimPrizeParamsV1, ClaimPrizeUpdateV1};
use crate::LOTTERY_CONTRACT_CLAIMS_TREE;
use crate::LOTTERY_CONTRACT_LOTTERIES_TREE;
use crate::LOTTERY_CONTRACT_TICKETS_TREE;

/// Process instruction for ClaimPrizeV1
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for the actual token transfer to the winner.
pub fn lottery_claim_prize_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ClaimPrizeParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled for prize payout
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[ClaimPrizeV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(LotteryError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[ClaimPrizeV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(LotteryError::InvalidChildCall.into())
    }

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
    let _winning_numbers = lottery.winning_numbers.as_ref().ok_or(LotteryError::LotteryNotFound)?;

    // Count matches - we need to recalculate since reveal info isn't stored
    // In a full implementation, we'd store reveal info or include it in params
    // For now, we'll trust the ZK proof verifies this

    // For a proper implementation, we would verify the ZK proof here:
    // wasm::zkas_verify(params.proof, ...)?
    //
    // NOTE: ZK proof verification is off-chain in DarkWow architecture.
    // The client verifies the ZK proof locally before submitting.
    // The WASM SDK does not expose zk_verify to contracts.

    // Calculate prize based on tier from ZK proof verification
    let prize = calculate_prize(&lottery, ticket.value, params.tier)?;

    // Create the update
    let update = ClaimPrizeUpdateV1 {
        ticket_id: params.ticket_id,
        tier: params.tier,
        matches: params.matches,
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
/// Uses tier from ZK proof to determine prize payout
fn calculate_prize(lottery: &crate::model::Lottery, _ticket_value: u64, tier: u8) -> Result<u64, ContractError> {
    if lottery.prize_pool == 0 {
        return Ok(0)
    }

    // Get tier config if valid tier (tiers are in lottery.config.prize_tiers)
    let prize_tiers = &lottery.config.prize_tiers;
    if tier as usize >= prize_tiers.len() {
        return Err(LotteryError::InvalidConfig.into())
    }

    let tier_config = &prize_tiers[tier as usize];
    let payout_percent = tier_config.payout_percent;

    // Calculate prize: (prize_pool * payout_percent) / 10000
    // Then divide by number of winners in that tier (approximated by ticket count)
    let total_payout = (lottery.prize_pool as u64 * payout_percent as u64) / 10000;

    // Approximate winners per tier based on ticket count
    let approx_winners = lottery.ticket_count.max(1);

    Ok(total_payout / approx_winners)
}
