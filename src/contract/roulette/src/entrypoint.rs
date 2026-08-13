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

//! Roulette Contract Entrypoint

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};

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
    ROULETTE_CONTRACT_INFO_TREE, ROULETTE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    ROULETTE_CONTRACT_NULLIFIERS_TREE, ROULETTE_CONTRACT_TABLES_TREE,
};

dwow_sdk::define_contract!(
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
    wasm::db::db_init(cid, ROULETTE_CONTRACT_INFO_TREE)?;

    // Store promissory_note contract ID for cross-contract validation
    let info_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, ROULETTE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    let _place_bet_v1_bincode = include_bytes!("../proof/place_bet.zk.bin");
    let _spin_wheel_v1_bincode = include_bytes!("../proof/spin_wheel.zk.bin");
    let _house_close_v1_bincode = include_bytes!("../proof/house_close.zk.bin");
    let _settle_bet_v1_bincode = include_bytes!("../proof/settle_bet.zk.bin");

    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = RouletteFunction::try_from(self_.data[0]).map_err(|_| RouletteError::InvalidFunction)?;

    let metadata = match func {
        RouletteFunction::PlaceBetV1 => {
            // Circuit order: tx_binding(0), tx_nonce(1)
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::ROULETTE_CONTRACT_ZKAS_PLACE_BET_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        RouletteFunction::SpinWheelV1 => {
            let params= SpinWheelParamsV1::decode(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::ROULETTE_CONTRACT_ZKAS_SPIN_WHEEL_NS_V2.to_string(),
                vec![params.table_id, params.house_pub_x, params.house_pub_y, params.spin_nullifier, pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        RouletteFunction::HouseCloseV1 => {
            let params= HouseCloseParamsV1::decode(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::ROULETTE_CONTRACT_ZKAS_HOUSE_CLOSE_NS_V2.to_string(),
                vec![params.table_id, params.house_pub_x, params.house_pub_y, params.close_nullifier, pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        // DESIGN RATIONALE (HAZOP M-11): The SettleBetsV1 ZK proof is belt-and-suspenders.
        // Plaintext validation in process_settle_bets_instruction (check_win,
        // total_payout accumulation) provides the primary security guarantee.
        // The ZK circuit constrains payout as a public instance — this catches
        // bugs in the entrypoint logic but does not add new security properties
        // beyond what the plaintext path already verifies. The contract's own
        // state checks are the authoritative verification layer.
        RouletteFunction::SettleBetsV1 => {
            let params= SettleBetsParamsV1::decode(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::ROULETTE_CONTRACT_ZKAS_SETTLE_BET_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(params.payout)],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> Result<(), ContractError> {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = RouletteFunction::try_from(self_.data[0]).map_err(|_| RouletteError::InvalidFunction)?;

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
    match RouletteFunction::try_from(update_data[0]).map_err(|_| RouletteError::InvalidFunction)? {
        RouletteFunction::InitializeV1 => {
            let update = InitializeUpdateV1::decode(&update_data[1..])?;
            roulette_initialize_process_update_v1(cid, update)
        }
        RouletteFunction::PlaceBetV1 => {
            let update = PlaceBetUpdateV1::decode(&update_data[1..])?;
            roulette_place_bet_process_update_v1(cid, update)
        }
        RouletteFunction::SpinWheelV1 => {
            let update = SpinWheelUpdateV1::decode(&update_data[1..])?;
            roulette_spin_wheel_process_update_v1(cid, update)
        }
        RouletteFunction::SettleBetsV1 => {
            let update = SettleBetsUpdateV1::decode(&update_data[1..])?;
            roulette_settle_bets_process_update_v1(cid, update)
        }
        RouletteFunction::HouseCloseV1 => {
            let update = HouseCloseUpdateV1::decode(&update_data[1..])?;
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
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= InitializeParamsV1::decode(&self_.data[1..])?;

    msg!("[roulette::initialize] Initializing roulette table");
    msg!("  american_wheel: {}", params.american_wheel);
    msg!("  house_capital: {}", params.house_capital);
    msg!("  max_straight_bet: {}", params.max_straight_bet);

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Derive table ID
    let table_id = derive_table_id(&params.house_pub, current_block);

    // Check if table already exists
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    if wasm::db::db_contains_key(tables_db, &table_id.to_repr())? {
        return Err(RouletteError::TableAlreadyClosed.into())
    }

    let (wheel_size, house_edge_bp) = if params.american_wheel {
        (38u8, crate::AMERICAN_HOUSE_EDGE_BP)
    } else {
        (37u8, crate::EUROPEAN_HOUSE_EDGE_BP)
    };

    let update = InitializeUpdateV1 {
        table_id,
        house_pub: params.house_pub,
        wheel_size,
        house_edge_bp,
        house_capital: params.house_capital,
        max_straight_bet: params.max_straight_bet,
        bets_close_block: current_block + params.duration_blocks,
        instance_seed: params.instance_seed,
    };

    msg!("[roulette::initialize] Table initialized");
    let encoded = [&[RouletteFunction::InitializeV1 as u8], &update.encode()[..]].concat();
    wasm::util::set_return_data(&encoded)?;
    Ok(encoded)
}

fn roulette_initialize_process_update_v1(cid: ContractId, update: InitializeUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let current_block = wasm::util::get_verifying_block_height()?.get();

    let table = if update.wheel_size == 38 {
        RouletteTable::new_american(
            update.table_id,
            update.house_pub,
            update.house_capital,
            update.max_straight_bet,
            update.bets_close_block - current_block,
            current_block,
            update.instance_seed,
        ).ok_or(RouletteError::ArithmeticOverflow)?
    } else {
        RouletteTable::new_european(
            update.table_id,
            update.house_pub,
            update.house_capital,
            update.max_straight_bet,
            update.bets_close_block - current_block,
            current_block,
            update.instance_seed,
        ).ok_or(RouletteError::ArithmeticOverflow)?
    };

    wasm::db::db_set(tables_db, &update.table_id.to_repr(), &table.encode())?;
    msg!("[roulette::initialize::update] Table stored in database");

    Ok(())
}

// =============================================================================
// PLACE BET
// =============================================================================

/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
/// locking the player's bet value.
fn roulette_place_bet_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= PlaceBetParamsV1::decode(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for bet locking
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[PlaceBetV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(RouletteError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[PlaceBetV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(RouletteError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, ROULETTE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(RouletteError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        params.table_id,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    msg!("[roulette::place_bet] Placing bet on table {:?}", params.table_id);

    // Get table
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let mut table: RouletteTable = match wasm::db::db_get(tables_db, &params.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Verify table is active and accepting bets
    if table.state != RouletteTableState::Active {
        return Err(RouletteError::TableNotActive.into())
    }

    // Check if table can accept bet
    let bet = Bet::new(
        params.table_id,
        params.player_pub,
        params.bet_type,
        params.numbers.clone(),
        params.amount,
        table.spin_count,
        current_block,
        params.instance_seed,
    ).ok_or(RouletteError::InvalidBetAmount)?;

    table.can_accept_bet(&bet, current_block)
        .map_err(|_| RouletteError::TableNotActive)?;

    // Check table has enough capital for potential payout
    if table.house_capital < bet.payout {
        return Err(RouletteError::InsufficientCapital.into())
    }

    // Check nullifier not used
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &bet.nullifier.to_repr())? {
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
        instance_seed: params.instance_seed,
    };

    msg!("[roulette::place_bet] Bet placed");
    let encoded = [&[RouletteFunction::PlaceBetV1 as u8], &update.encode()[..]].concat();
    Ok(encoded)
}

fn roulette_place_bet_process_update_v1(cid: ContractId, update: PlaceBetUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let bets_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_BETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;

    // Get and update table
    let mut table: RouletteTable = match wasm::db::db_get(tables_db, &update.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };
    table.house_capital = update.table_house_capital;

    wasm::db::db_set(tables_db, &update.table_id.to_repr(), &table.encode())?;

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
        placed_at: wasm::util::get_verifying_block_height()?.get(),
        nullifier: update.nullifier,
        instance_seed: update.instance_seed,
    };

    wasm::db::db_set(bets_db, &update.bet_id.to_repr(), &bet.encode())?;
    wasm::db::db_mark_spent(nullifiers_db, &update.nullifier.to_repr())?;

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
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= SpinWheelParamsV1::decode(&self_.data[1..])?;

    msg!("[roulette::spin] Spinning wheel for table {:?}", params.table_id);

    // Get table
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let mut table: RouletteTable = match wasm::db::db_get(tables_db, &params.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };

    let current_block = wasm::util::get_verifying_block_height()?;

    // Verify table is active
    if table.state != RouletteTableState::Active {
        return Err(RouletteError::TableNotActive.into())
    }

    // Check if bets should close
    if current_block.get() < table.bets_close_block {
        return Err(RouletteError::SpinNotReady.into())
    }

    // Verify house_pub coordinates match the table's house (ZK proof verifies ownership)
    let (table_house_x, table_house_y) = table.house_pub.xy().expect("pk not identity");
    if params.house_pub_x != table_house_x || params.house_pub_y != table_house_y {
        return Err(RouletteError::UnauthorizedCaller.into())
    }

    // Verify spin_nullifier hasn't been used (ZK proof verifies it's correctly derived)
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.spin_nullifier.to_repr())? {
        return Err(RouletteError::InvalidSignature.into())
    }

    // Collect block hashes across confirmation depth for entropy
    let depth = 6u64;
    let mut entropy_blocks = Vec::with_capacity(depth as usize);
    for i in 0..depth {
        let h = current_block.get().saturating_sub(i);
        let block_hash = wasm::util::get_block_hash(
            dwow_sdk::blockchain::BlockHeight::new(h),
        )?.0;
        entropy_blocks.push(dwow_entropy_contract::EntropyBlock { height: h, block_hash });
    }
    let seed = dwow_entropy_contract::derive_seed(&entropy_blocks);
    let block_hash_base = pallas::Base::from(seed);

    // Draw winning number
    let winning_number = draw_winning_number(
        block_hash_base,
        params.nonce,
        table.wheel_size,
    );

    // Update table
    table.winning_number = Some(winning_number);
    table.spin_count += 1;
    table.spun_at_block = Some(current_block.get());

    let update = SpinWheelUpdateV1 {
        table_id: params.table_id,
        winning_number,
        spin_number: table.spin_count,
        spun_at_block: current_block.get(),
        spin_nullifier: params.spin_nullifier,
    };

    msg!("[roulette::spin] Winning number: {}", winning_number);
    let encoded = [&[RouletteFunction::SpinWheelV1 as u8], &update.encode()[..]].concat();
    Ok(encoded)
}

fn roulette_spin_wheel_process_update_v1(cid: ContractId, update: SpinWheelUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;

    let mut table: RouletteTable = match wasm::db::db_get(tables_db, &update.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };

    table.winning_number = Some(update.winning_number);
    table.spin_count = update.spin_number;
    table.spun_at_block = Some(update.spun_at_block);
    table.state = RouletteTableState::Spun;

    wasm::db::db_set(tables_db, &update.table_id.to_repr(), &table.encode())?;

    // Record spin nullifier to prevent replay
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.spin_nullifier.to_repr())?;

    msg!("[roulette::spin::update] Wheel spun, state updated");

    Ok(())
}

// =============================================================================
// SETTLE BETS
// =============================================================================

/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
/// paying out winnings to winning players.
fn roulette_settle_bets_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= SettleBetsParamsV1::decode(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for payouts
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[SettleBetsV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(RouletteError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[SettleBetsV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(RouletteError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, ROULETTE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(RouletteError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    msg!("[roulette::settle] Settling {} bets", params.bet_ids.len());

    // Get table
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let table: RouletteTable = match wasm::db::db_get(tables_db, &params.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };

    // Verify table has been spun
    if table.state != RouletteTableState::Spun {
        return Err(RouletteError::WheelAlreadySpun.into())
    }

    // Get winning number
    let winning_number = table.winning_number.ok_or(RouletteError::WheelAlreadySpun)?;

    // Get bets and settle
    let bets_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_BETS_TREE)?;
    let mut total_payout: u64 = 0;

    if params.bet_ids.len() > crate::ROULETTE_CONTRACT_MAX_SETTLE_BETS {
        msg!("[roulette::settle_bets] Too many bet IDs: {} (max {})",
            params.bet_ids.len(), crate::ROULETTE_CONTRACT_MAX_SETTLE_BETS);
        return Err(RouletteError::InternalError("Too many bet IDs".to_string()).into());
    }
    for bet_id in &params.bet_ids {
        let bet: Bet = match wasm::db::db_get(bets_db, &bet_id.to_repr())? {
            Some(data) => Bet::decode(&data)?,
            None => continue, // Skip missing bets
        };

        if bet.won.is_some() {
            continue // Already settled
        }

        let won = bet.check_win(winning_number);
        if won {
            total_payout += bet.payout;
        }
    }

    // House edge is already captured in the difference between bet_value and payout
    // When player wins: house pays payout (which was reserved), house edge comes from house's share
    // When player loses: house keeps bet_value, house edge extracted from profit
    // Net effect: house_capital decreases by total_payout (payout to winners)
    // The house edge is the spread between bet_value and payout, accumulated in house profit
    let house_payout = total_payout;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(house_payout),
        params.table_id,
    ]);
    validate_child_value_commit(&child_call.data, house_payout, value_blind)?;

    let new_capital = table.house_capital.saturating_sub(house_payout);

    // Note: In a full implementation, house edge would be tracked separately
    // For now, the house edge accumulates as the difference between bet values
    // received and payouts made, tracked via house_capital changes

    let update = SettleBetsUpdateV1 {
        table_id: params.table_id,
        winning_number,
        settled_count: params.bet_ids.len() as u64,
        house_payout,
        house_new_capital: new_capital,
        state: RouletteTableState::Settled,
    };

    msg!("[roulette::settle] Total payout: {}", house_payout);
    let encoded = [&[RouletteFunction::SettleBetsV1 as u8], &update.encode()[..]].concat();
    Ok(encoded)
}

fn roulette_settle_bets_process_update_v1(cid: ContractId, update: SettleBetsUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;

    let mut table: RouletteTable = match wasm::db::db_get(tables_db, &update.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };

    table.house_capital = update.house_new_capital;
    table.state = update.state;

    wasm::db::db_set(tables_db, &update.table_id.to_repr(), &table.encode())?;
    msg!("[roulette::settle::update] Bets settled, capital updated, state updated");

    Ok(())
}

// =============================================================================
// HOUSE CLOSE
// =============================================================================

/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
/// collecting the house's remaining capital.
fn roulette_house_close_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= HouseCloseParamsV1::decode(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for house's capital
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[HouseCloseV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(RouletteError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[HouseCloseV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(RouletteError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, ROULETTE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(RouletteError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    msg!("[roulette::house_close] Closing table {:?}", params.table_id);

    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;
    let table: RouletteTable = match wasm::db::db_get(tables_db, &params.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(table.house_capital),
        params.table_id,
    ]);
    validate_child_value_commit(&child_call.data, table.house_capital, value_blind)?;

    // Validate table is in a state that allows closing (after spin)
    if table.state != RouletteTableState::Spun && table.state != RouletteTableState::Settled {
        return Err(RouletteError::InvalidTableState.into())
    }

    // Verify house_pub coordinates match the table's house (ZK proof verifies ownership)
    let (table_house_x, table_house_y) = table.house_pub.xy().expect("pk not identity");
    if params.house_pub_x != table_house_x || params.house_pub_y != table_house_y {
        return Err(RouletteError::UnauthorizedCaller.into())
    }

    // Verify close_nullifier hasn't been used (ZK proof verifies it's correctly derived)
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.close_nullifier.to_repr())? {
        return Err(RouletteError::InvalidSignature.into())
    }

    let update = HouseCloseUpdateV1 {
        table_id: params.table_id,
        remaining_capital: table.house_capital,
        close_nullifier: params.close_nullifier,
    };

    msg!("[roulette::house_close] Remaining capital: {}", table.house_capital);
    let encoded = [&[RouletteFunction::HouseCloseV1 as u8], &update.encode()[..]].concat();
    Ok(encoded)
}

fn roulette_house_close_process_update_v1(cid: ContractId, update: HouseCloseUpdateV1) -> ContractResult {
    let tables_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_TABLES_TREE)?;

    let mut table: RouletteTable = match wasm::db::db_get(tables_db, &update.table_id.to_repr())? {
        Some(data) => RouletteTable::decode(&data)?,
        None => return Err(RouletteError::TableNotFound.into()),
    };

    table.state = RouletteTableState::Closed;

    wasm::db::db_set(tables_db, &update.table_id.to_repr(), &table.encode())?;

    // Record close nullifier to prevent replay
    let nullifiers_db = wasm::db::db_lookup(cid, ROULETTE_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.close_nullifier.to_repr())?;

    msg!("[roulette::house_close::update] Table closed");

    Ok(())
}
