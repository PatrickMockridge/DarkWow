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

//! DrawWinnersV1 Implementation

use dwow_sdk::{crypto::pasta_prelude::PrimeField, error::ContractError, msg, pasta::pallas, wasm};
use dwow_serial::{deserialize, serialize};

use crate::error::LotteryError;
use crate::model::{draw_winning_numbers, DrawWinnersParamsV1, DrawWinnersUpdateV1, LotteryState};
use crate::LOTTERY_CONTRACT_LOTTERIES_TREE;

/// Process instruction for DrawWinnersV1
pub fn lottery_draw_winners_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: DrawWinnersParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[lottery::draw_winners] Drawing winners for lottery: {:?}", params.lottery_id);

    // Get lottery state
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    let lottery: crate::model::Lottery =
        deserialize(&wasm::db::db_get(lotteries_db, &serialize(&params.lottery_id))?.unwrap())?;

    // Verify lottery is in correct state
    if lottery.state != LotteryState::Initialized {
        return Err(LotteryError::LotteryAlreadyExpired.into())
    }

    // Verify lottery has ended
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block < lottery.draw_block_deadline {
        return Err(LotteryError::DrawNotYetAvailable.into())
    }

    // Get block hash for randomness
    let tx_block_hash = wasm::util::get_block_hash(current_block as u32)?;

    // Convert block_hash bytes to pallas::Base for entropy
    // TransactionHash is a wrapper around [u8; 32]
    let hash_bytes = tx_block_hash.0;
    let block_hash = pallas::Base::from(u64::from_le_bytes(hash_bytes[0..8].try_into().unwrap()));

    // Convert nonce to u64 seed
    let seed_nonce = u64::from_le_bytes(params.nonce.to_repr()[0..8].try_into().unwrap());

    // Draw winning numbers
    let winning_numbers = draw_winning_numbers(
        block_hash,
        seed_nonce,
        lottery.config.num_picks,
        lottery.config.number_range,
    );

    msg!("[lottery::draw_winners] Winning numbers: {:?}", winning_numbers);

    // Calculate prize pools
    let gross_pool = lottery.gross_pool;
    let house_share = lottery.calculate_house_share();
    let prize_pool = gross_pool.saturating_sub(house_share);

    // Create the update
    let update = DrawWinnersUpdateV1 {
        lottery_id: params.lottery_id,
        winning_numbers,
        draw_block: current_block,
        gross_pool,
        house_share,
        prize_pool,
        state: LotteryState::WinnersDrawn,
    };

    msg!("[lottery::draw_winners] Winners drawn successfully");
    Ok(serialize(&update))
}

/// Process update for DrawWinnersV1
pub fn lottery_draw_winners_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: DrawWinnersUpdateV1,
) -> Result<(), ContractError> {
    let lotteries_db = wasm::db::db_lookup(cid, LOTTERY_CONTRACT_LOTTERIES_TREE)?;

    // Get and update lottery
    let mut lottery: crate::model::Lottery =
        deserialize(&wasm::db::db_get(lotteries_db, &serialize(&update.lottery_id))?.unwrap())?;

    lottery.state = update.state;
    lottery.winning_numbers = Some(update.winning_numbers.clone());
    lottery.draw_block = Some(update.draw_block);
    lottery.gross_pool = update.gross_pool;
    lottery.house_share = update.house_share;
    lottery.prize_pool = update.prize_pool;

    // Store updated lottery
    wasm::db::db_set(lotteries_db, &serialize(&update.lottery_id), &serialize(&lottery))?;
    msg!("[lottery::draw_winners::update] Lottery updated with winning numbers");

    Ok(())
}
