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

//! Slot Contract Entrypoint
//!
//! A composable slot machine contract with modular design.
//!
//! Flow:
//! 1. Player commits to a spin via Slot::CommitSpinV1 (locks bet value)
//! 2. Block entropy reveals random positions via Slot::RevealSpinV1
//! 3. Payout calculated and winner determined via Slot::SettleSpinV1
//! 4. House can close abandoned spins via Slot::CancelSpinV1
//!
//! Modular Design:
//! - Paytables define winning combinations (swappable per game)
//! - Reel strips define symbol layouts (configurable)
//! - Extension traits for bonus rounds (future work)

use dwow_sdk::{
    crypto::{poseidon_hash, pasta_prelude::PrimeField, ContractId},
    dark_tree::DarkLeaf,
    error::{GenericResult, ContractError},
    msg, wasm,
    ContractCall,
};
use pasta_curves::{arithmetic::CurveAffine, group::Curve};
use dwow_sdk::pasta::pallas::Base;
use dwow_serial::{deserialize, serialize, Encodable};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};

use crate::error::SlotError;
use crate::model::{
    video_paytable, CommitSpinParamsV1, CommitSpinUpdateV1, GameConfig, Spin, SpinId, SpinState,
};
use crate::SlotFunction;
use crate::{
    SLOT_CONTRACT_INFO_TREE, SLOT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    SLOT_CONTRACT_ZKAS_COMMIT_NS, SLOT_CONTRACT_ZKAS_SETTLE_NS,
};

// Database trees
const SPINS_TREE: &str = "spins";
const CONFIG_TREE: &str = "config";
const HOUSE_TREE: &str = "house";

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    wasm::db::db_init(cid, SPINS_TREE)?;
    wasm::db::db_init(cid, CONFIG_TREE)?;
    wasm::db::db_init(cid, HOUSE_TREE)?;
    wasm::db::db_init(cid, SLOT_CONTRACT_INFO_TREE)?;

    // Store promissory_note contract ID for cross-contract validation
    let info_db = wasm::db::db_lookup(cid, SLOT_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, SLOT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    let commit_bet_v1_bincode = include_bytes!("../proof/commit_bet_v1.zk.bin");
    wasm::db::zkas_db_set(&commit_bet_v1_bincode[..])?;
    let settle_bet_v1_bincode = include_bytes!("../proof/settle_bet_v1.zk.bin");
    wasm::db::zkas_db_set(&settle_bet_v1_bincode[..])?;

    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> GenericResult<()> {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = SlotFunction::try_from(self_.data[0])?;

    let metadata = match func {
        SlotFunction::CommitSpinV1 => {
            let params: CommitSpinParamsV1 = deserialize(&self_.data[1..])?;
            slot_commit_bet_get_metadata_v1(params)?
        }
        SlotFunction::SettleSpinV1 => {
            let params: crate::model::SettleSpinParamsV1 = deserialize(&self_.data[1..])?;
            slot_settle_bet_get_metadata_v1(params)?
        }
        // No ZK circuits for Initialize, RevealSpin, CancelSpin
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn slot_commit_bet_get_metadata_v1(
    params: CommitSpinParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<Base>)> = vec![];
    let (px, py) = params.player_pub.xy();
    let spin_id = poseidon_hash([
        px,
        py,
        Base::from(params.bet_value),
        Base::from(params.paylines_played as u64),
        params.secret_nonce,
        params.blind,
        params.token_id,
    ]);
    let vc_affine = params.value_commit.to_affine();
    let coords = vc_affine.coordinates();
    if coords.is_none().into() {
        Ok(vec![])
    } else {
    let vc_coords = coords.unwrap();
    let (vc_x, vc_y) = (*vc_coords.x(), *vc_coords.y());
    zk_public_inputs.push((
        SLOT_CONTRACT_ZKAS_COMMIT_NS.to_string(),
        vec![spin_id, vc_x, vc_y],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
    }
}

fn slot_settle_bet_get_metadata_v1(
    params: crate::model::SettleSpinParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<Base>)> = vec![];
    zk_public_inputs.push((
        SLOT_CONTRACT_ZKAS_SETTLE_NS.to_string(),
        vec![params.spin_id, Base::from(params.payout)],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> GenericResult<()> {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = SlotFunction::try_from(func_byte)?;

    let update_data = match func {
        SlotFunction::InitializeV1 => {
            initialize_process_instruction_v1(cid, call_idx, calls)?
        }
        SlotFunction::CommitSpinV1 => commit_spin_process_instruction_v1(cid, call_idx, calls)?,
        SlotFunction::RevealSpinV1 => reveal_spin_process_instruction_v1(cid, call_idx, calls)?,
        SlotFunction::SettleSpinV1 => settle_spin_process_instruction_v1(cid, call_idx, calls)?,
        SlotFunction::CancelSpinV1 => cancel_spin_process_instruction_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&[&[func_byte], &update_data[..]].concat())
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> GenericResult<()> {
    match SlotFunction::try_from(update_data[0])? {
        SlotFunction::InitializeV1 => {
            // No state update needed for initialize
            Ok(())
        }
        SlotFunction::CommitSpinV1 => {
            let update: CommitSpinUpdateV1 = deserialize(&update_data[1..])?;
            commit_spin_process_update_v1(cid, update)
        }
        SlotFunction::RevealSpinV1 => {
            let update: crate::model::RevealSpinUpdateV1 = deserialize(&update_data[1..])?;
            reveal_spin_process_update_v1(cid, update)
        }
        SlotFunction::SettleSpinV1 => {
            let update: crate::model::SettleSpinUpdateV1 = deserialize(&update_data[1..])?;
            settle_spin_process_update_v1(cid, update)
        }
        SlotFunction::CancelSpinV1 => {
            let update: crate::model::CancelSpinUpdateV1 = deserialize(&update_data[1..])?;
            cancel_spin_process_update_v1(cid, update)
        }
    }
}

// =============================================================================
// INITIALIZE (Set up game configuration)
// =============================================================================

fn initialize_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let _self_ = &calls[call_idx].data;
    // Initialize takes no params, just sets up config
    // In a full implementation, this would include game type selection

    msg!("[slot::initialize] Initializing slot contract");

    // For now, default to video slot (5 reels, 3 rows, 9 paylines)
    // This could be extended to support multiple game types
    let config = GameConfig {
        version: 1,
        reel_count: 5,
        row_count: 3,
        reels: video_paytable::default_reels(),
        paylines: create_3x5_paylines(),
        house_edge: 500, // 5% house edge
    };

    // Store config
    let config_db = wasm::db::db_lookup(cid, CONFIG_TREE)?;
    wasm::db::db_set(config_db, b"config", &serialize(&config))?;

    msg!("[slot::initialize] Slot contract initialized with video slot config");
    Ok(vec![])
}

// =============================================================================
// COMMIT SPIN
// =============================================================================

/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
/// locking the player's bet value.
fn commit_spin_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CommitSpinParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for bet locking
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[CommitSpinV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(SlotError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[CommitSpinV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(SlotError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, SLOT_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, SLOT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(SlotError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::from_bytes([0u8; 32]).unwrap() {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    msg!(
        "[slot::commit_spin] Committing spin for player {:?}, bet: {}",
        params.player_pub,
        params.bet_value
    );

    // Validate bet value
    if params.bet_value == 0 {
        return Err(SlotError::InvalidBetValue.into())
    }

    // Get config to validate paylines
    let config_db = wasm::db::db_lookup(cid, CONFIG_TREE)?;
    let config_data = wasm::db::db_get(config_db, b"config")?;
    let config: GameConfig = match config_data {
        Some(data) => deserialize(&data)?,
        None => {
            // Auto-initialize if not set
            return Err(SlotError::HouseNotInitialized.into())
        }
    };

    // Validate paylines
    if params.paylines_played == 0 ||
        params.paylines_played > config.paylines.len() as u32
    {
        return Err(SlotError::InvalidPayline.into())
    }

    // Derive spin ID
    let spin_id = crate::model::derive_spin_id(
        &params.player_pub,
        params.bet_value,
        params.paylines_played,
        params.secret_nonce,
        params.blind,
        params.token_id,
    );

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        Base::from(params.bet_value),
        spin_id,
    ]);
    validate_child_value_commit(&child_call.data, params.bet_value, value_blind)?;

    // Check spin doesn't already exist
    let spins_db = wasm::db::db_lookup(cid, SPINS_TREE)?;
    if wasm::db::db_contains_key(spins_db, &serialize(&spin_id))? {
        return Err(SlotError::SpinAlreadyExists.into())
    }

    // Calculate settle block
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    let settle_block = current_block + params.confirmation_depth as u64 + 1;

    let update = CommitSpinUpdateV1 {
        spin_id,
        player_pub: params.player_pub,
        bet_value: params.bet_value,
        paylines_played: params.paylines_played,
        secret_nonce: params.secret_nonce,
        blind: params.blind,
        house_edge: params.house_edge,
        confirmation_depth: params.confirmation_depth,
        token_id: params.token_id,
        value_commit: params.value_commit,
        settle_block,
        nullifier: spin_id, // Initially same as ID
        state: SpinState::Committed,
        created_at: current_block,
        instance_seed: params.instance_seed,
    };

    msg!(
        "[slot::commit_spin] Spin {:?} committed, settle at block {}",
        spin_id,
        settle_block
    );
    Ok(serialize(&update))
}

fn commit_spin_process_update_v1(cid: ContractId, update: CommitSpinUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, SPINS_TREE)?;

    let spin = Spin {
        version: 1,
        id: update.spin_id,
        player_pub: update.player_pub,
        bet_value: update.bet_value,
        paylines_played: update.paylines_played,
        secret_nonce: update.secret_nonce,
        blind: update.blind,
        result: None,
        wins: vec![],
        payout: 0,
        state: update.state,
        house_edge: update.house_edge,
        confirmation_depth: update.confirmation_depth,
        created_at: update.created_at,
        settle_block: update.settle_block,
        value_commit: update.value_commit,
        token_id: update.token_id,
        nullifier: update.nullifier,
        instance_seed: update.instance_seed,
    };

    wasm::db::db_set(db, &serialize(&update.spin_id), &serialize(&spin))?;
    msg!("[slot::commit_spin::update] Spin stored");

    Ok(())
}

// =============================================================================
// REVEAL SPIN (Determine random positions)
// =============================================================================

fn reveal_spin_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: crate::model::RevealSpinParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[slot::reveal_spin] Revealing spin {:?}", params.spin_id);

    // Look up spin
    let spins_db = wasm::db::db_lookup(cid, SPINS_TREE)?;
    let mut spin: Spin = match wasm::db::db_get(spins_db, &serialize(&params.spin_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(SlotError::SpinNotFound.into()),
    };

    // Check spin is in correct state
    if spin.state != SpinState::Committed {
        return Err(SlotError::InvalidSpinState.into())
    }

    // Verify secret nonce matches
    if spin.secret_nonce != params.secret_nonce {
        return Err(SlotError::InvalidSignature.into())
    }

    // Get config for reel count
    let config_db = wasm::db::db_lookup(cid, CONFIG_TREE)?;
    let config_data = wasm::db::db_get(config_db, b"config")?;
    let config: GameConfig = match config_data {
        Some(data) => deserialize(&data)?,
        None => return Err(SlotError::HouseNotInitialized.into()),
    };

    // Get block hash for entropy (unpredictable RandomX PoW output)
    let block_hash = wasm::util::get_block_hash(wasm::util::get_verifying_block_height()?)?.0;

    let positions = derive_positions_from_entropy(
        block_hash,
        spin.id,
        spin.secret_nonce,
        config.reel_count,
    );

    // Store result
    spin.result = Some(crate::model::SpinResult::new(positions.clone()));
    spin.state = SpinState::Revealed;

    let update = crate::model::RevealSpinUpdateV1 {
        spin_id: spin.id,
        positions,
        state: spin.state,
    };

    wasm::db::db_set(spins_db, &serialize(&spin.id), &serialize(&spin))?;
    msg!("[slot::reveal_spin] Spin {:?} revealed", spin.id);
    Ok(serialize(&update))
}

fn reveal_spin_process_update_v1(
    _cid: ContractId,
    update: crate::model::RevealSpinUpdateV1,
) -> GenericResult<()> {
    // State already updated in process_instruction
    msg!("[slot::reveal_spin::update] Reveal confirmed for spin {:?}", update.spin_id);
    Ok(())
}

// =============================================================================
// SETTLE SPIN (Calculate payouts)
// =============================================================================

/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
/// paying out winnings to the player.
fn settle_spin_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: crate::model::SettleSpinParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for payouts
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[SettleSpinV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(SlotError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[SettleSpinV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(SlotError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, SLOT_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, SLOT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(SlotError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::from_bytes([0u8; 32]).unwrap() {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    msg!("[slot::settle_spin] Settling spin {:?}", params.spin_id);

    // Look up spin
    let spins_db = wasm::db::db_lookup(cid, SPINS_TREE)?;
    let mut spin: Spin = match wasm::db::db_get(spins_db, &serialize(&params.spin_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(SlotError::SpinNotFound.into()),
    };

    // Check spin is in correct state
    if spin.state != SpinState::Revealed {
        return Err(SlotError::InvalidSpinState.into())
    }

    // Get config
    let config_db = wasm::db::db_lookup(cid, CONFIG_TREE)?;
    let config_data = wasm::db::db_get(config_db, b"config")?;
    let config: GameConfig = match config_data {
        Some(data) => deserialize(&data)?,
        None => return Err(SlotError::HouseNotInitialized.into()),
    };

    // Get paytable (could be selected based on game type)
    let paytable = video_paytable::create();

    // Calculate wins
    let result = spin.result.as_ref().ok_or(SlotError::InvalidSpinState)?;
    let active_paylines: Vec<_> = config.paylines.iter().take(spin.paylines_played as usize).cloned().collect();
    let wins = crate::model::calculate_wins(result, &config.reels, &active_paylines, &paytable);

    // Calculate payout
    let payout = crate::model::calculate_payout(spin.bet_value, &wins, spin.house_edge);

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        Base::from(payout),
        params.spin_id,
    ]);
    validate_child_value_commit(&child_call.data, payout, value_blind)?;

    // Verify ZK proof public input matches computed payout
    if params.payout != payout {
        msg!("[slot::settle_spin] ERROR: Payout mismatch: computed={}, claimed={}", payout, params.payout);
        return Err(SlotError::InvalidBetValue.into())
    }

    // Update spin
    spin.wins = wins.clone();
    spin.payout = payout;
    spin.state = SpinState::Settled;

    let update = crate::model::SettleSpinUpdateV1 {
        spin_id: spin.id,
        wins,
        payout,
        state: spin.state,
    };

    wasm::db::db_set(spins_db, &serialize(&spin.id), &serialize(&spin))?;
    msg!(
        "[slot::settle_spin] Spin {:?} settled, payout: {}",
        spin.id,
        payout
    );
    Ok(serialize(&update))
}

fn settle_spin_process_update_v1(
    _cid: ContractId,
    update: crate::model::SettleSpinUpdateV1,
) -> GenericResult<()> {
    msg!(
        "[slot::settle_spin::update] Settlement confirmed for spin {:?}, payout: {}",
        update.spin_id,
        update.payout
    );
    Ok(())
}

// =============================================================================
// CANCEL SPIN (Timeout/abandoned)
// =============================================================================

/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be bundled for
/// collecting the house's share of the bet.
fn cancel_spin_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: crate::model::CancelSpinParamsV1 = deserialize(&self_.data[1..])?;

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled for house's share
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[CancelSpinV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(SlotError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[CancelSpinV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(SlotError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, SLOT_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, SLOT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(SlotError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::from_bytes([0u8; 32]).unwrap() {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    msg!("[slot::cancel_spin] Cancelling spin {:?}", params.spin_id);

    // Look up spin
    let spins_db = wasm::db::db_lookup(cid, SPINS_TREE)?;
    let mut spin: Spin = match wasm::db::db_get(spins_db, &serialize(&params.spin_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(SlotError::SpinNotFound.into()),
    };

    // Check spin is still committed or revealed (not already settled)
    if spin.state == SpinState::Settled || spin.state == SpinState::Cancelled {
        return Err(SlotError::InvalidSpinState.into())
    }

    // Check timeout (current block past settle_block)
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block < spin.settle_block {
        return Err(SlotError::InvalidSpinState.into())
    }

    // Calculate house take
    let house_take = crate::model::calculate_house_take(spin.bet_value, spin.house_edge);

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        Base::from(house_take),
        params.spin_id,
    ]);
    validate_child_value_commit(&child_call.data, house_take, value_blind)?;

    // Update spin
    spin.state = SpinState::Cancelled;

    let update = crate::model::CancelSpinUpdateV1 {
        spin_id: spin.id,
        house_take,
        state: spin.state,
    };

    wasm::db::db_set(spins_db, &serialize(&spin.id), &serialize(&spin))?;
    msg!("[slot::cancel_spin] Spin {:?} cancelled, house takes: {}", spin.id, house_take);
    Ok(serialize(&update))
}

fn cancel_spin_process_update_v1(
    _cid: ContractId,
    update: crate::model::CancelSpinUpdateV1,
) -> GenericResult<()> {
    msg!(
        "[slot::cancel_spin::update] Cancellation confirmed for spin {:?}",
        update.spin_id
    );
    Ok(())
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Create standard paylines for a 3x5 slot display
fn create_3x5_paylines() -> Vec<crate::model::Payline> {
    vec![
        // Horizontal lines (top, middle, bottom)
        crate::model::Payline::horizontal_top(5),
        crate::model::Payline::horizontal_middle(5),
        crate::model::Payline::horizontal_bottom(5),
        // V shapes
        crate::model::Payline::new(3, vec![0, 1, 2, 1, 0]),
        crate::model::Payline::new(4, vec![2, 1, 0, 1, 2]),
        // More complex patterns could be added
    ]
}

/// Derive reel positions from block hash entropy
/// Uses full 32-byte block hash for unpredictability
fn derive_positions_from_entropy(
    block_hash: [u8; 32],
    spin_id: SpinId,
    secret_nonce: Base,
    num_reels: usize,
) -> Vec<u64> {
    let a = u64::from_le_bytes(block_hash[0..8].try_into().unwrap());
    let b = u64::from_le_bytes(block_hash[8..16].try_into().unwrap());
    let c = u64::from_le_bytes(block_hash[16..24].try_into().unwrap());
    let d = u64::from_le_bytes(block_hash[24..32].try_into().unwrap());

    let mut positions = Vec::with_capacity(num_reels);

    for i in 0..num_reels {
        // Create entropy for this reel using all 32 bytes of block hash
        let entropy = poseidon_hash([
            spin_id,
            secret_nonce,
            Base::from(a),
            Base::from(b),
            Base::from(c),
            Base::from(d),
            Base::from(i as u64),
        ]);

        let bytes = entropy.to_repr();
        let seed = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or_default());
        let position = seed % 100;
        positions.push(position);
    }

    positions
}