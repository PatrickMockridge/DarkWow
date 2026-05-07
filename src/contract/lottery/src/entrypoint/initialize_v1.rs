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

//! InitializeV1 Implementation

use dwow_sdk::{error::ContractError, msg, pasta::pallas, wasm};
use dwow_serial::{deserialize, serialize};

use crate::error::LotteryError;
use crate::model::{derive_lottery_id, InitializeParamsV1, InitializeUpdateV1, Lottery, LotteryState};
use crate::LOTTERY_CONTRACT_CURRENT_LOTTERY;
use crate::LOTTERY_CONTRACT_LOTTERIES_TREE;

/// Process instruction for InitializeV1
pub fn lottery_initialize_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: InitializeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[lottery::initialize] Initializing new lottery");
    msg!("  num_picks: {}", params.config.num_picks);
    msg!("  number_range: {}", params.config.number_range);
    msg!("  house_edge_bp: {}", params.config.house_edge_bp);
    msg!("  ticket_price: {}", params.config.ticket_price);
    msg!("  duration: {}", params.duration);
    msg!("  rolled_over: {}", params.rolled_over);

    // Validate configuration
    params.config.validate()?;

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;
    let current_block = current_block as u64;

    // Derive lottery ID
    let lottery_id = derive_lottery_id(&params.house_pub, current_block);

    msg!("[lottery::initialize] Derived lottery_id");

    // Check if lottery already exists
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    if wasm::db::db_contains_key(lotteries_db, &serialize(&lottery_id))? {
        return Err(LotteryError::LotteryAlreadyExpired.into())
    }

    // Calculate deadlines
    let draw_block_deadline = current_block + params.duration;
    let claim_deadline = draw_block_deadline + params.claim_duration;

    // Create the update
    let update = InitializeUpdateV1 {
        lottery_id,
        config: params.config.clone(),
        house_pub: params.house_pub,
        draw_block_deadline,
        claim_deadline,
        rolled_over: params.rolled_over,
        state: LotteryState::Initialized,
    };

    msg!("[lottery::initialize] Lottery initialized successfully");
    Ok(serialize(&update))
}

/// Process update for InitializeV1
pub fn lottery_initialize_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: InitializeUpdateV1,
) -> Result<(), ContractError> {
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;

    // Create lottery state
    let lottery = Lottery {
        id: update.lottery_id,
        config: update.config.clone(),
        house_pub: update.house_pub,
        state: update.state,
        ticket_count: 0,
        gross_pool: 0,
        house_share: 0,
        prize_pool: 0,
        winning_numbers: None,
        draw_block: None,
        ticket_merkle_root: pallas::Base::zero(),
        created_at: wasm::util::get_verifying_block_height()? as u64,
        draw_block_deadline: update.draw_block_deadline,
        claim_deadline: update.claim_deadline,
        rolled_over: update.rolled_over,
    };

    // Store lottery
    wasm::db::db_set(lotteries_db, &serialize(&update.lottery_id), &serialize(&lottery))?;
    msg!("[lottery::initialize::update] Lottery stored in database");

    // Set as current lottery
    wasm::db::db_set(
        lotteries_db,
        LOTTERY_CONTRACT_CURRENT_LOTTERY,
        &serialize(&update.lottery_id),
    )?;
    msg!("[lottery::initialize::update] Current lottery set");

    Ok(())
}
