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

//! ExpireLotteryV1 Implementation
//!
//! ## Money Integration
//!
//! This function REQUIRES money_v3::transfer_v1 child calls to be bundled for
//! the actual token transfer to the house for unclaimed prizes.

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::error::LotteryError;
use crate::model::{ExpireLotteryParamsV1, ExpireLotteryUpdateV1, LotteryState};
use crate::LOTTERY_CONTRACT_LOTTERIES_TREE;

/// Process instruction for ExpireLotteryV1
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for the actual token transfer to the house.
pub fn lottery_expire_lottery_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: ExpireLotteryParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled for house claim
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[ExpireLotteryV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(LotteryError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[ExpireLotteryV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(LotteryError::InvalidChildCall.into())
    }

    msg!("[lottery::expire_lottery] Expiring lottery: {:?}", params.lottery_id);

    // Get lottery state
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    let lottery: crate::model::Lottery =
        deserialize(&wasm::db::db_get(lotteries_db, &serialize(&params.lottery_id))?.unwrap())?;

    // Verify lottery is in winners drawn state
    if lottery.state != LotteryState::WinnersDrawn {
        return Err(LotteryError::LotteryAlreadyExpired.into())
    }

    // Verify claim period has expired
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block <= lottery.claim_deadline {
        return Err(LotteryError::ClaimPeriodExpired.into())
    }

    // Calculate unclaimed amount and house's claim
    // For now, assume all unclaimed goes to house
    // In a proper implementation, we'd track actual claims per tier
    let unclaimed_rollover = calculate_unclaimed(&lottery)?;
    let house_claim = lottery.prize_pool.saturating_sub(unclaimed_rollover);

    msg!("[lottery::expire_lottery] Unclaimed rollover: {}, House claim: {}", unclaimed_rollover, house_claim);

    // Create the update
    let update = ExpireLotteryUpdateV1 {
        lottery_id: params.lottery_id,
        unclaimed_rollover,
        house_claim,
        state: LotteryState::Expired,
    };

    msg!("[lottery::expire_lottery] Lottery expired successfully");
    Ok(serialize(&update))
}

/// Process update for ExpireLotteryV1
pub fn lottery_expire_lottery_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ExpireLotteryUpdateV1,
) -> Result<(), ContractError> {
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;

    // Get and update lottery
    let mut lottery: crate::model::Lottery =
        deserialize(&wasm::db::db_get(lotteries_db, &serialize(&update.lottery_id))?.unwrap())?;

    lottery.state = update.state;

    // Store updated lottery
    wasm::db::db_set(lotteries_db, &serialize(&update.lottery_id), &serialize(&lottery))?;
    msg!("[lottery::expire_lottery::update] Lottery marked as expired");

    Ok(())
}

/// Calculate unclaimed prize amount
/// Note: This is simplified - proper implementation needs to track actual claims per tier
fn calculate_unclaimed(lottery: &crate::model::Lottery) -> Result<u64, ContractError> {
    // For now, assume 50% unclaimed (simplified)
    // Real implementation would:
    // 1. Query claims database for all claims this lottery
    // 2. Sum up prizes paid out per tier
    // 3. Subtract from prize_pool to get unclaimed
    Ok(lottery.prize_pool / 2)
}
