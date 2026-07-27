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

//! Betting Stake Contract Entrypoint

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta::pallas, wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};
use pasta_curves::group::Curve;
use pasta_curves::arithmetic::CurveAffine;

use crate::error::BettingStakeError;
use crate::model::{
    ClaimEarningsParamsV1, ClaimEarningsUpdateV1, InitializeParamsV1, InitializeUpdateV1,
    Stake, StakeParamsV1, StakeUpdateV1, TableStakeRegistry, UnstakeParamsV1, UnstakeUpdateV1,
    UpdateRiskParamsV1, UpdateRiskUpdateV1,
};
use crate::BettingStakeFunction;
use crate::{
    BETTING_STAKE_EARNINGS_TREE, BETTING_STAKE_INFO_TREE,
    BETTING_STAKE_NULLIFIERS_TREE,
    BETTING_STAKE_PROMISSORY_NOTE_CONTRACT_ID, BETTING_STAKE_REGISTRY_TREE,
    BETTING_STAKE_STAKES_TREE,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    let init_v1_bincode = include_bytes!("../proof/init_v1.zk.bin");
    let stake_v1_bincode = include_bytes!("../proof/stake_v1.zk.bin");
    let unstake_v1_bincode = include_bytes!("../proof/unstake_v1.zk.bin");
    let claim_v1_bincode = include_bytes!("../proof/claim_v1.zk.bin");
    let update_risk_v1_bincode = include_bytes!("../proof/update_risk_v1.zk.bin");

    wasm::db::zkas_db_set(&init_v1_bincode[..])?;
    wasm::db::zkas_db_set(&stake_v1_bincode[..])?;
    wasm::db::zkas_db_set(&unstake_v1_bincode[..])?;
    wasm::db::zkas_db_set(&claim_v1_bincode[..])?;
    wasm::db::zkas_db_set(&update_risk_v1_bincode[..])?;

    // Initialize database trees
    wasm::db::db_init(cid, BETTING_STAKE_REGISTRY_TREE)?;
    wasm::db::db_init(cid, BETTING_STAKE_STAKES_TREE)?;
    wasm::db::db_init(cid, BETTING_STAKE_EARNINGS_TREE)?;
    wasm::db::db_init(cid, BETTING_STAKE_INFO_TREE)?;
    wasm::db::db_init(cid, BETTING_STAKE_NULLIFIERS_TREE)?;

    // Store promissory_note contract ID for cross-contract validation
    let info_db = wasm::db::db_lookup(cid, BETTING_STAKE_INFO_TREE)?;
    wasm::db::db_set(info_db, BETTING_STAKE_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BettingStakeFunction::try_from(self_.data[0]).map_err(|_| BettingStakeError::InvalidFunction)?;

    let metadata = match func {
        BettingStakeFunction::InitializeV1 => {
            let params= crate::model::InitializeParamsV1::decode(&self_.data[1..])?;
            let table_id = poseidon_hash([params.betting_contract_id, params.nonce]);
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BETTING_STAKE_ZKAS_INIT_NS_V2.to_string(),
                vec![table_id],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        BettingStakeFunction::StakeV1 => {
            let params= crate::model::StakeParamsV1::decode(&self_.data[1..])?;
            let staker_x = params.staker_pub.x().expect("pk not identity");
            let staker_y = params.staker_pub.y().expect("pk not identity");
            let stake_id = poseidon_hash([
                params.table_id,
                staker_x,
                staker_y,
                pallas::Base::from(params.amount),
                params.nonce,
            ]);
            let vc_affine = params.value_commit.to_affine();
            let coords = vc_affine.coordinates();
            if coords.is_none().into() {
                vec![]
            } else {
            let vc_coords = coords.unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BETTING_STAKE_ZKAS_STAKE_NS_V2.to_string(),
                vec![stake_id, *vc_coords.x(), *vc_coords.y(), params.staker_nullifier, pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
            }
        }
        BettingStakeFunction::UpdateRiskV1 => {
            let params= crate::model::UpdateRiskParamsV1::decode(&self_.data[1..])?;
            let table_id = poseidon_hash([params.betting_contract_id, params.nonce]);
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BETTING_STAKE_ZKAS_UPDATE_RISK_NS_V2.to_string(),
                vec![table_id],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        BettingStakeFunction::UnstakeV1 => {
            let params= crate::model::UnstakeParamsV1::decode(&self_.data[1..])?;
            let staker_x = params.staker_pub.x().expect("pk not identity");
            let staker_y = params.staker_pub.y().expect("pk not identity");
            let stake_id = poseidon_hash([
                params.table_id,
                staker_x,
                staker_y,
                pallas::Base::from(params.original_amount),
                params.nonce,
            ]);
            let vc_affine = params.value_commit.to_affine();
            let coords = vc_affine.coordinates();
            if coords.is_none().into() {
                vec![]
            } else {
            let vc_coords = coords.unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BETTING_STAKE_ZKAS_UNSTAKE_NS_V2.to_string(),
                vec![stake_id, *vc_coords.x(), *vc_coords.y(), params.staker_nullifier, pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
            }
        }
        BettingStakeFunction::ClaimEarningsV1 => {
            let params= crate::model::ClaimEarningsParamsV1::decode(&self_.data[1..])?;
            let staker_x = params.staker_pub.x().expect("pk not identity");
            let staker_y = params.staker_pub.y().expect("pk not identity");
            let stake_id = poseidon_hash([
                params.table_id,
                staker_x,
                staker_y,
                pallas::Base::from(params.current_amount),
                params.nonce,
            ]);
            let vc_affine = params.value_commit.to_affine();
            let coords = vc_affine.coordinates();
            if coords.is_none().into() {
                vec![]
            } else {
            let vc_coords = coords.unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BETTING_STAKE_ZKAS_CLAIM_NS_V2.to_string(),
                vec![stake_id, *vc_coords.x(), *vc_coords.y(), params.staker_nullifier, pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
            }
        }
    };

    wasm::util::set_return_data(&metadata)
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BettingStakeFunction::try_from(self_.data[0]).map_err(|_| BettingStakeError::InvalidFunction)?;

    let update_data = match func {
        BettingStakeFunction::InitializeV1 => {
            staking_initialize_process_instruction_v1(cid, call_idx, calls)?
        }
        BettingStakeFunction::StakeV1 => staking_stake_process_instruction_v1(cid, call_idx, calls)?,
        BettingStakeFunction::UnstakeV1 => {
            staking_unstake_process_instruction_v1(cid, call_idx, calls)?
        }
        BettingStakeFunction::ClaimEarningsV1 => {
            staking_claim_earnings_process_instruction_v1(cid, call_idx, calls)?
        }
        BettingStakeFunction::UpdateRiskV1 => {
            staking_update_risk_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)?;
    Ok(())
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match BettingStakeFunction::try_from(update_data[0]).map_err(|_| BettingStakeError::InvalidFunction)? {
        BettingStakeFunction::InitializeV1 => {
            let update = decode_initialize_update_v1(&update_data[1..])?;
            staking_initialize_process_update_v1(cid, update)
        }
        BettingStakeFunction::StakeV1 => {
            let update = decode_stake_update_v1(&update_data[1..])?;
            staking_stake_process_update_v1(cid, update)
        }
        BettingStakeFunction::UnstakeV1 => {
            let update = decode_unstake_update_v1(&update_data[1..])?;
            staking_unstake_process_update_v1(cid, update)
        }
        BettingStakeFunction::ClaimEarningsV1 => {
            let update = decode_claim_earnings_update_v1(&update_data[1..])?;
            staking_claim_earnings_process_update_v1(cid, update)
        }
        BettingStakeFunction::UpdateRiskV1 => {
            let update = decode_update_risk_update_v1(&update_data[1..])?;
            staking_update_risk_process_update_v1(cid, update)
        }
    }
}

// =============================================================================
// RHO-CALCULUS EXPLICIT BRIDGE ENCODE/DECODE
// =============================================================================
// Per type-system.md §2.2: bytes round-trip across module boundaries is forbidden.
// Per contract-wasm-type-system.md §3.1: bridge SHALL use explicit per-type encode/decode.
// These replace serialize(&(BettingStakeFunction::XxxV1 as u8, update)) on the exec side
// and deserialize(&update_data[1..]) on the apply side.

fn encode_initialize_update_v1(update: &InitializeUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(BettingStakeFunction::InitializeV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_initialize_update_v1(data: &[u8]) -> Result<InitializeUpdateV1, ContractError> {
    InitializeUpdateV1::decode(data)
}

fn encode_stake_update_v1(update: &StakeUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(BettingStakeFunction::StakeV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_stake_update_v1(data: &[u8]) -> Result<StakeUpdateV1, ContractError> {
    StakeUpdateV1::decode(data)
}

fn encode_unstake_update_v1(update: &UnstakeUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(BettingStakeFunction::UnstakeV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_unstake_update_v1(data: &[u8]) -> Result<UnstakeUpdateV1, ContractError> {
    UnstakeUpdateV1::decode(data)
}

fn encode_claim_earnings_update_v1(update: &ClaimEarningsUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(BettingStakeFunction::ClaimEarningsV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_claim_earnings_update_v1(data: &[u8]) -> Result<ClaimEarningsUpdateV1, ContractError> {
    ClaimEarningsUpdateV1::decode(data)
}

fn encode_update_risk_update_v1(update: &UpdateRiskUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(BettingStakeFunction::UpdateRiskV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_update_risk_update_v1(data: &[u8]) -> Result<UpdateRiskUpdateV1, ContractError> {
    UpdateRiskUpdateV1::decode(data)
}

// =============================================================================
// INITIALIZE
// =============================================================================

fn staking_initialize_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= InitializeParamsV1::decode(&self_.data[1..])?;

    msg!("[betting_stake::initialize] Initializing staking for contract");

    // Validate house edge
    if params.house_edge_bp > 10000 {
        return Err(BettingStakeError::InvalidEarnings.into())
    }

    // Derive table ID
    let table_id = derive_table_id(params.betting_contract_id, 0);

    // Check if table already exists
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;
    if wasm::db::db_contains_key(registry_db, &table_id.to_repr())? {
        return Err(BettingStakeError::TableNotFound.into())
    }

    let update = InitializeUpdateV1 {
        instance_seed: params.instance_seed,
        table_id,
        betting_contract_id: params.betting_contract_id,
        house_edge_bp: params.house_edge_bp,
        risk_profile: params.risk_profile,
    };

    msg!("[betting_stake::initialize] Table initialized");
    wasm::util::set_return_data(&encode_initialize_update_v1(&update))?;
    Ok(encode_initialize_update_v1(&update))
}

fn staking_initialize_process_update_v1(cid: ContractId, update: InitializeUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;

    let registry = TableStakeRegistry {
        version: 1,
        betting_contract_id: update.betting_contract_id,
        total_stake: 0,
        accumulated_earnings: 0,
        accumulated_losses: 0,
        staker_count: 0,
        house_edge_bp: update.house_edge_bp,
        risk_profile: update.risk_profile,
    };

    wasm::db::db_set(registry_db, &update.table_id.to_repr(), &registry.encode())?;
    msg!("[betting_stake::initialize::update] Registry stored");

    Ok(())
}

// =============================================================================
// STAKE
// =============================================================================

fn staking_stake_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[betting_stake::StakeV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(BettingStakeError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[betting_stake::StakeV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(BettingStakeError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, BETTING_STAKE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, BETTING_STAKE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(BettingStakeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    let self_ = &calls[call_idx].data;
    let params= StakeParamsV1::decode(&self_.data[1..])?;

    msg!("[betting_stake::stake] Staking {} against table", params.amount);

    // Get registry
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;
    let mut table: TableStakeRegistry = match wasm::db::db_get(registry_db, &params.table_id.to_repr())? {
        Some(data) => TableStakeRegistry::decode(&data)?,
        None => return Err(BettingStakeError::TableNotFound.into()),
    };

    // Validate stake amount
    if params.amount < crate::MIN_STAKE_AMOUNT {
        return Err(BettingStakeError::StakeTooSmall.into())
    }

    // Verify staker nullifier hasn't been used (ZK proof verifies identity)
    let nullifiers_db = wasm::db::db_lookup(cid, BETTING_STAKE_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.staker_nullifier.to_repr())? {
        return Err(BettingStakeError::DuplicateNullifier.into())
    }

    // Generate stake ID
    let stake_id =
        derive_stake_id(params.table_id, &params.staker_pub, params.amount, wasm::util::get_verifying_block_height()?.get());

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        stake_id,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    // Check if stake already exists
    let stakes_db = wasm::db::db_lookup(cid, BETTING_STAKE_STAKES_TREE)?;
    if wasm::db::db_contains_key(stakes_db, &stake_id.to_repr())? {
        return Err(BettingStakeError::StakeNotFound.into())
    }

    // Update registry
    table.total_stake += params.amount;
    table.staker_count += 1;

    let update = StakeUpdateV1 {
        instance_seed: params.instance_seed,
        stake_id,
        table_id: params.table_id,
        staker_pub: params.staker_pub,
        amount: params.amount,
        total_stake: table.total_stake,
        staker_count: table.staker_count,
        staker_nullifier: params.staker_nullifier,
    };

    msg!("[betting_stake::stake] Stake created");
    Ok(encode_stake_update_v1(&update))
}

fn staking_stake_process_update_v1(cid: ContractId, update: StakeUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;
    let stakes_db = wasm::db::db_lookup(cid, BETTING_STAKE_STAKES_TREE)?;

    // Get and update registry
    let mut table: TableStakeRegistry = match wasm::db::db_get(registry_db, &update.table_id.to_repr())? {
        Some(data) => TableStakeRegistry::decode(&data)?,
        None => return Err(BettingStakeError::TableNotFound.into()),
    };
    table.total_stake = update.total_stake;
    table.staker_count = update.staker_count;

    wasm::db::db_set(registry_db, &update.table_id.to_repr(), &table.encode())?;

    // Create stake
    let stake = Stake {
        version: 1,
        instance_seed: update.instance_seed,
        stake_id: update.stake_id,
        table_id: update.table_id,
        staker_pub: update.staker_pub,
        original_amount: update.amount,
        current_amount: update.amount,
        accumulated_earnings: 0,
        created_at: wasm::util::get_verifying_block_height()?.get(),
        unstake_requested_at: None,
        is_active: true,
    };

    wasm::db::db_set(stakes_db, &update.stake_id.to_repr(), &stake.encode())?;

    // Record staker nullifier for replay protection
    let nullifiers_db = wasm::db::db_lookup(cid, BETTING_STAKE_NULLIFIERS_TREE)?;
    wasm::db::db_set(nullifiers_db, &update.staker_nullifier.to_repr(), &[])?;

    msg!("[betting_stake::stake::update] Stake stored in database");

    Ok(())
}

// =============================================================================
// UNSTAKE
// =============================================================================

fn staking_unstake_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[betting_stake::UnstakeV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(BettingStakeError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[betting_stake::UnstakeV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(BettingStakeError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, BETTING_STAKE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, BETTING_STAKE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(BettingStakeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    let self_ = &calls[call_idx].data;
    let params= UnstakeParamsV1::decode(&self_.data[1..])?;

    msg!("[betting_stake::unstake] Unstaking request");

    // Get stake
    let stakes_db = wasm::db::db_lookup(cid, BETTING_STAKE_STAKES_TREE)?;
    let stake: Stake = match wasm::db::db_get(stakes_db, &params.stake_id.to_repr())? {
        Some(data) => Stake::decode(&data)?,
        None => return Err(BettingStakeError::StakeNotFound.into()),
    };

    if !stake.is_active {
        return Err(BettingStakeError::StakeLocked.into())
    }

    // Verify lock period has expired before unstake
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if !stake.can_unstake(crate::UNSTAKE_LOCK_PERIOD, current_block) {
        msg!("[betting_stake::unstake] Error: Unstake lock period not expired");
        return Err(BettingStakeError::StakeLocked.into())
    }

    // Verify caller identity via ZK proof: staker_pub must match stored stake
    if params.staker_pub != stake.staker_pub {
        return Err(BettingStakeError::UnauthorizedCaller.into())
    }

    // Verify staker nullifier hasn't been used (ZK proof verifies identity)
    let nullifiers_db = wasm::db::db_lookup(cid, BETTING_STAKE_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.staker_nullifier.to_repr())? {
        return Err(BettingStakeError::DuplicateNullifier.into())
    }

    // Get table for final settlement
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;
    let table: TableStakeRegistry = match wasm::db::db_get(registry_db, &stake.table_id.to_repr())? {
        Some(data) => TableStakeRegistry::decode(&data)?,
        None => return Err(BettingStakeError::TableNotFound.into()),
    };

    // Calculate final payout
    // FIX: Handle division by zero if total_stake is 0
    let stake_share = if table.total_stake == 0 {
        0
    } else {
        (stake.current_amount * 10000) / table.total_stake
    };
    let loss_share = table.accumulated_losses.checked_mul(stake_share).ok_or(BettingStakeError::ArithmeticOverflow)? / 10000;
    let earnings_share = table.accumulated_earnings.checked_mul(stake_share).ok_or(BettingStakeError::ArithmeticOverflow)? / 10000;

    let payout_amount = stake.current_amount.saturating_sub(loss_share) + earnings_share;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(payout_amount),
        params.stake_id,
    ]);
    validate_child_value_commit(&child_call.data, payout_amount, value_blind)?;

    let unstake_penalty = 0; // No penalty in this simple version

    let update = UnstakeUpdateV1 { stake_id: params.stake_id, payout_amount, unstake_penalty, staker_nullifier: params.staker_nullifier };

    msg!("[betting_stake::unstake] Payout: {}", payout_amount);
    Ok(encode_unstake_update_v1(&update))
}

fn staking_unstake_process_update_v1(cid: ContractId, update: UnstakeUpdateV1) -> ContractResult {
    let stakes_db = wasm::db::db_lookup(cid, BETTING_STAKE_STAKES_TREE)?;

    // Get and update stake
    let mut stake: Stake = match wasm::db::db_get(stakes_db, &update.stake_id.to_repr())? {
        Some(data) => Stake::decode(&data)?,
        None => return Err(BettingStakeError::StakeNotFound.into()),
    };

    stake.is_active = false;
    stake.current_amount = 0;
    stake.accumulated_earnings = 0;

    wasm::db::db_set(stakes_db, &update.stake_id.to_repr(), &stake.encode())?;

    // Record staker nullifier for replay protection
    let nullifiers_db = wasm::db::db_lookup(cid, BETTING_STAKE_NULLIFIERS_TREE)?;
    wasm::db::db_set(nullifiers_db, &update.staker_nullifier.to_repr(), &[])?;

    msg!("[betting_stake::unstake::update] Stake deactivated");

    Ok(())
}

// =============================================================================
// CLAIM EARNINGS
// =============================================================================

fn staking_claim_earnings_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!(
            "[betting_stake::ClaimEarningsV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len()
        );
        return Err(BettingStakeError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[betting_stake::ClaimEarningsV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(BettingStakeError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, BETTING_STAKE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, BETTING_STAKE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(BettingStakeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    let self_ = &calls[call_idx].data;
    let params= ClaimEarningsParamsV1::decode(&self_.data[1..])?;

    msg!("[betting_stake::claim] Claiming earnings");

    // Get stake
    let stakes_db = wasm::db::db_lookup(cid, BETTING_STAKE_STAKES_TREE)?;
    let stake: Stake = match wasm::db::db_get(stakes_db, &params.stake_id.to_repr())? {
        Some(data) => Stake::decode(&data)?,
        None => return Err(BettingStakeError::StakeNotFound.into()),
    };

    // Verify caller identity via ZK proof: staker_pub must match stored stake
    if params.staker_pub != stake.staker_pub {
        return Err(BettingStakeError::UnauthorizedCaller.into())
    }

    // Verify staker nullifier hasn't been used (ZK proof verifies identity)
    let nullifiers_db = wasm::db::db_lookup(cid, BETTING_STAKE_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.staker_nullifier.to_repr())? {
        return Err(BettingStakeError::DuplicateNullifier.into())
    }

    // Get table
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;
    let table: TableStakeRegistry = match wasm::db::db_get(registry_db, &stake.table_id.to_repr())? {
        Some(data) => TableStakeRegistry::decode(&data)?,
        None => return Err(BettingStakeError::TableNotFound.into()),
    };

    // Calculate claimable earnings
    // FIX: Handle division by zero if total_stake is 0
    let stake_share = if table.total_stake == 0 {
        0
    } else {
        (stake.current_amount * 10000) / table.total_stake
    };
    let total_earnings = table.accumulated_earnings.checked_mul(stake_share).ok_or(BettingStakeError::ArithmeticOverflow)? / 10000;
    let claimable = total_earnings.saturating_sub(stake.accumulated_earnings);

    if claimable == 0 {
        return Err(BettingStakeError::NoEarnings.into())
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(claimable),
        params.stake_id,
    ]);
    validate_child_value_commit(&child_call.data, claimable, value_blind)?;

    let update = ClaimEarningsUpdateV1 {
        stake_id: params.stake_id,
        claimed_amount: claimable,
        remaining_earnings: stake.accumulated_earnings + claimable,
        staker_nullifier: params.staker_nullifier,
    };

    msg!("[betting_stake::claim] Claimed: {}", claimable);
    Ok(encode_claim_earnings_update_v1(&update))
}

fn staking_claim_earnings_process_update_v1(cid: ContractId, update: ClaimEarningsUpdateV1) -> ContractResult {
    let stakes_db = wasm::db::db_lookup(cid, BETTING_STAKE_STAKES_TREE)?;

    // Get and update stake
    let mut stake: Stake = match wasm::db::db_get(stakes_db, &update.stake_id.to_repr())? {
        Some(data) => Stake::decode(&data)?,
        None => return Err(BettingStakeError::StakeNotFound.into()),
    };

    stake.accumulated_earnings = update.remaining_earnings;

    wasm::db::db_set(stakes_db, &update.stake_id.to_repr(), &stake.encode())?;

    // Record staker nullifier for replay protection
    let nullifiers_db = wasm::db::db_lookup(cid, BETTING_STAKE_NULLIFIERS_TREE)?;
    wasm::db::db_set(nullifiers_db, &update.staker_nullifier.to_repr(), &[])?;

    msg!("[betting_stake::claim::update] Earnings updated");

    Ok(())
}

// =============================================================================
// UPDATE RISK (called by betting contracts)
// =============================================================================

fn staking_update_risk_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= UpdateRiskParamsV1::decode(&self_.data[1..])?;

    msg!("[betting_stake::update_risk] Processing payout of {}", params.payout_amount);

    // Get registry
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;
    let mut table: TableStakeRegistry = match wasm::db::db_get(registry_db, &params.table_id.to_repr())? {
        Some(data) => TableStakeRegistry::decode(&data)?,
        None => return Err(BettingStakeError::TableNotFound.into()),
    };

    // Calculate staker's share of loss
    // Stakers absorb: payout_amount - house_share
    let staker_loss = params.payout_amount.saturating_sub(params.house_share);

    // House edge earnings
    let house_edge_earnings = (params.payout_amount * table.house_edge_bp as u64) / 10000;
    table.accumulated_earnings += house_edge_earnings;
    table.accumulated_losses += staker_loss;

    // If losses exceed stake, stakers get wiped out (clawback would be needed)
    if table.accumulated_losses > table.total_stake {
        msg!("[betting_stake::update_risk] WARNING: Losses exceed stake!");
    }

    let update = UpdateRiskUpdateV1 {
        table_id: params.table_id,
        total_payout: params.payout_amount,
        staker_loss,
        staker_count: table.staker_count,
        new_total_stake: table.total_stake.saturating_sub(staker_loss),
    };

    msg!("[betting_stake::update_risk] Staker loss: {}", staker_loss);
    Ok(encode_update_risk_update_v1(&update))
}

fn staking_update_risk_process_update_v1(cid: ContractId, update: UpdateRiskUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, BETTING_STAKE_REGISTRY_TREE)?;

    // Get and update registry
    let mut table: TableStakeRegistry = match wasm::db::db_get(registry_db, &update.table_id.to_repr())? {
        Some(data) => TableStakeRegistry::decode(&data)?,
        None => return Err(BettingStakeError::TableNotFound.into()),
    };

    table.total_stake = update.new_total_stake;

    wasm::db::db_set(registry_db, &update.table_id.to_repr(), &table.encode())?;
    msg!("[betting_stake::update_risk::update] Table risk updated");

    Ok(())
}

// =============================================================================
// HELPERS
// =============================================================================

fn derive_table_id(betting_contract_id: pallas::Base, nonce: u64) -> pallas::Base {
    poseidon_hash([betting_contract_id, pallas::Base::from(nonce)])
}

fn derive_stake_id(table_id: pallas::Base, staker_pub: &PublicKey, amount: u64, nonce: u64) -> pallas::Base {
    poseidon_hash([table_id, staker_pub.x().expect("pk not identity"), staker_pub.y().expect("pk not identity"), pallas::Base::from(amount), pallas::Base::from(nonce)])
}
