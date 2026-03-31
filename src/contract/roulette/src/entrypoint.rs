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

//! Roulette Contract Entrypoint

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractResult,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::RouletteError;
use crate::model::{
    Bet, HouseCloseParamsV1, HouseCloseUpdateV1, InitializeParamsV1, InitializeUpdateV1,
    PlaceBetParamsV1, PlaceBetUpdateV1, RouletteTable, RouletteTableState,
    SettleBetsParamsV1, SettleBetsUpdateV1, SpinWheelParamsV1, SpinWheelUpdateV1,
    derive_table_id, draw_winning_number,
};
use crate::RouletteFunction;
use crate::{
    ROULETTE_CONTRACT_BETS_HISTORY_TREE, ROULETTE_CONTRACT_BETS_TREE,
    ROULETTE_CONTRACT_NULLIFIERS_TREE, ROULETTE_CONTRACT_TABLES_TREE,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize database trees
    wasm::db::db_init(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    wasm::db::db_init(cid, ROULETTE_CONTRACT_BETS_TREE)?;
    wasm::db::db_init(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_init(cid, ROULETTE_CONTRACT_BETS_HISTORY_TREE)?;

    Ok(())
}

/// Get metadata for verification
fn _get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = RouletteFunction::try_from(self_.data[0])?;

    let update_data = match func {
        RouletteFunction::InitializeV1 => {
            roulette_initialize_process_instruction_v1(cid, call_idx, calls)?
        }
        RouletteFunction::PlaceBetV1 => roulette_place_bet_process_instruction_v1(cid, call_idx, calls)?,
        RouletteFunction::SpinWheelV1 => {
            roulette_spin_wheel_process_instruction_v1(cid, call_idx, calls)?
        }
        RouletteFunction::SettleBetsV1 => {
            roulette_settle_bets_process_instruction_v1(cid, call_idx, calls)?
        }
        RouletteFunction::HouseCloseV1 => {
            roulette_house_close_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match RouletteFunction::try_from(update_data[0])? {
        RouletteFunction::InitializeV1 => {
            let update: InitializeUpdateV1 = deserialize(&update_data[1..])?;
            roulette_initialize_process_update_v1(cid, update)
        }
        RouletteFunction::PlaceBetV1 => {
            let update: PlaceBetUpdateV1 = deserialize(&update_data[1..])?;
            roulette_place_bet_process_update_v1(cid, update)
        }
        RouletteFunction::SpinWheelV1 => {
            let update: SpinWheelUpdateV1 = deserialize(&update_data[1..])?;
            roulette_spin_wheel_process_update_v1(cid, update)
        }
        RouletteFunction::SettleBetsV1 => {
            let update: SettleBetsUpdateV1 = deserialize(&update_data[1..])?;
            roulette_settle_bets_process_update_v1(cid, update)
        }
        RouletteFunction::HouseCloseV1 => {
            let update: HouseCloseUpdateV1 = deserialize(&update_data[1..])?;
            roulette_house_close_process_update_v1(cid, update)
        }
    }
}

// =============================================================================
// INITIALIZE
// =============================================================================

fn roulette_initialize_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: InitializeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[roulette::initialize] Initializing roulette table");
    msg!("  american_wheel: {}", params.american_wheel);
    msg!("  house_capital: {}", params.house_capital);
    msg!("  max_straight_bet: {}", params.max_straight_bet);

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Derive table ID
    let table_id = derive_table_id(&params.house_pub, current_block);

    // Check if table already exists
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    if wasm::db::db_contains_key(tables_db, &serialize(&table_id))? {
        return Err(RouletteError::TableAlreadyClosed.into())
    }

    let (wheel_size, house_edge_bp) = if params.american_wheel {
        (38u8, crate::AMERICAN_HOUSE_EDGE_BP)
    } else {
        (37u8, crate::EUROPEAN_HOUSE_EDGE_BP)
    };

    let update = InitializeUpdateV1 {
        table_id,
        wheel_size,
        house_edge_bp,
        house_capital: params.house_capital,
        max_straight_bet: params.max_straight_bet,
        bets_close_block: current_block + params.duration_blocks,
    };

    msg!("[roulette::initialize] Table {} initialized", table_id);
    Ok(serialize(&update))
}

fn roulette_initialize_process_update_v1(cid: ContractId, update: InitializeUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let current_block = wasm::util::get_verifying_block_height()? as u64;

    let table = if update.wheel_size == 38 {
        RouletteTable::new_american(
            update.table_id,
            update.table_id.into(), // placeholder
            update.house_capital,
            update.max_straight_bet,
            update.bets_close_block - current_block,
            current_block,
        )
    } else {
        RouletteTable::new_european(
            update.table_id,
            update.table_id.into(),
            update.house_capital,
            update.max_straight_bet,
            update.bets_close_block - current_block,
            current_block,
        )
    };

    wasm::db::db_set(tables_db, &serialize(&update.table_id), &serialize(&table))?;
    msg!("[roulette::initialize::update] Table stored in database");

    Ok(())
}

// =============================================================================
// PLACE BET
// =============================================================================

fn roulette_place_bet_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: PlaceBetParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[roulette::place_bet] Placing bet on table {:?}", params.table_id);

    // Get table
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let mut table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&params.table_id))?.unwrap())?;

    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Check if table can accept bet
    let bet = Bet::new(
        params.table_id,
        params.player_pub,
        params.bet_type,
        params.numbers.clone(),
        params.amount,
        table.spin_count,
        current_block,
    );

    table.can_accept_bet(&bet, current_block)
        .map_err(|e| RouletteError::TableNotActive)?;

    // Check table has enough capital for potential payout
    if table.house_capital < bet.payout {
        return Err(RouletteError::InsufficientCapital.into())
    }

    // Check nullifier not used
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&bet.nullifier))? {
        return Err(RouletteError::DuplicateBet.into())
    }

    // Update table state
    table.house_capital -= bet.payout; // Reserve payout

    let update = PlaceBetUpdateV1 {
        bet_id: bet.bet_id,
        table_id: params.table_id,
        player_pub: params.player_pub,
        bet_type: params.bet_type,
        numbers: params.numbers,
        amount: params.amount,
        payout: bet.payout,
        spin_number: table.spin_count,
        nullifier: bet.nullifier,
        table_house_capital: table.house_capital,
        total_bets: 0, // Would track in full impl
    };

    msg!("[roulette::place_bet] Bet {} placed", bet.bet_id);
    Ok(serialize(&update))
}

fn roulette_place_bet_process_update_v1(cid: ContractId, update: PlaceBetUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let bets_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_BETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;

    // Get and update table
    let mut table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&update.table_id))?.unwrap())?;
    table.house_capital = update.table_house_capital;

    wasm::db::db_set(tables_db, &serialize(&update.table_id), &serialize(&table))?;

    // Create bet
    let bet = Bet {
        bet_id: update.bet_id,
        table_id: update.table_id,
        player_pub: update.player_pub,
        bet_type: update.bet_type,
        numbers: update.numbers,
        amount: update.amount,
        payout: update.payout,
        won: None,
        actual_payout: 0,
        spin_number: update.spin_number,
        placed_at: wasm::util::get_verifying_block_height()? as u64,
        nullifier: update.nullifier,
    };

    wasm::db::db_set(bets_db, &serialize(&update.bet_id), &serialize(&bet))?;
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;

    msg!("[roulette::place_bet::update] Bet stored, capital reserved");

    Ok(())
}

// =============================================================================
// SPIN WHEEL
// =============================================================================

fn roulette_spin_wheel_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: SpinWheelParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[roulette::spin] Spinning wheel for table {:?}", params.table_id);

    // Get table
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let mut table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&params.table_id))?.unwrap())?;

    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Check if bets should close
    if current_block < table.bets_close_block {
        return Err(RouletteError::SpinNotReady.into())
    }

    // Get block hash for randomness
    let block_hash = wasm::util::get_block_hash(current_block as u32)?;

    // Draw winning number
    let winning_number = draw_winning_number(
        block_hash.0.into(),
        params.nonce,
        table.wheel_size,
    );

    // Update table
    table.winning_number = Some(winning_number);
    table.spin_count += 1;
    table.spun_at_block = Some(current_block);

    let update = SpinWheelUpdateV1 {
        table_id: params.table_id,
        winning_number,
        spin_number: table.spin_count,
        spun_at_block: current_block,
    };

    msg!("[roulette::spin] Winning number: {}", winning_number);
    Ok(serialize(&update))
}

fn roulette_spin_wheel_process_update_v1(cid: ContractId, update: SpinWheelUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;

    let mut table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&update.table_id))?.unwrap())?;

    table.winning_number = Some(update.winning_number);
    table.spin_count = update.spin_number;
    table.spun_at_block = Some(update.spun_at_block);
    table.state = RouletteTableState::Spun;

    wasm::db::db_set(tables_db, &serialize(&update.table_id), &serialize(&table))?;
    msg!("[roulette::spin::update] Wheel spun, state updated");

    Ok(())
}

// =============================================================================
// SETTLE BETS
// =============================================================================

fn roulette_settle_bets_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: SettleBetsParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[roulette::settle] Settling {} bets", params.bet_ids.len());

    // Get table
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let mut table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&params.table_id))?.unwrap())?;

    // Get winning number
    let winning_number = table.winning_number.ok_or(RouletteError::WheelAlreadySpun)?;

    // Get bets and settle
    let bets_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_BETS_TREE)?;
    let mut total_payout: u64 = 0;

    for bet_id in &params.bet_ids {
        let bet: Bet =
            deserialize(&wasm::db::db_get(bets_db, &serialize(bet_id))?.unwrap())?;

        if bet.won.is_some() {
            continue // Already settled
        }

        let won = bet.check_win(winning_number);
        if won {
            total_payout += bet.payout;
        }
    }

    // House pays out from reserved capital + house edge
    let house_payout = total_payout;
    let new_capital = table.house_capital + house_payout;

    let update = SettleBetsUpdateV1 {
        table_id: params.table_id,
        winning_number,
        settled_count: params.bet_ids.len() as u64,
        house_payout,
        house_new_capital: new_capital,
    };

    msg!("[roulette::settle] Total payout: {}", house_payout);
    Ok(serialize(&update))
}

fn roulette_settle_bets_process_update_v1(cid: ContractId, update: SettleBetsUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;

    let mut table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&update.table_id))?.unwrap())?;

    table.house_capital = update.house_new_capital;

    wasm::db::db_set(tables_db, &serialize(&update.table_id), &serialize(&table))?;
    msg!("[roulette::settle::update] Bets settled, capital updated");

    Ok(())
}

// =============================================================================
// HOUSE CLOSE
// =============================================================================

fn roulette_house_close_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: HouseCloseParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[roulette::house_close] Closing table {:?}", params.table_id);

    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&params.table_id))?.unwrap())?;

    let update = HouseCloseUpdateV1 {
        table_id: params.table_id,
        remaining_capital: table.house_capital,
    };

    msg!("[roulette::house_close] Remaining capital: {}", table.house_capital);
    Ok(serialize(&update))
}

fn roulette_house_close_process_update_v1(cid: ContractId, update: HouseCloseUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;

    let mut table: RouletteTable =
        deserialize(&wasm::db::db_get(tables_db, &serialize(&update.table_id))?.unwrap())?;

    table.state = RouletteTableState::Closed;

    wasm::db::db_set(tables_db, &serialize(&update.table_id), &serialize(&table))?;
    msg!("[roulette::house_close::update] Table closed");

    Ok(())
}
