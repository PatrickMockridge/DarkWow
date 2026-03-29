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

//! WASM entrypoint for the oracle contract
//!
//! ## Oracle Contract Overview
//!
//! Demonstrates the "push model" for oracles in DarkFi.
//! Oracles push data values which can be attested for consumption by other contracts.
//!
//! ## Flow
//!
//! 1. Oracle operator registers an oracle data feed
//! 2. Oracle pushes data values (prices, scores, etc.)
//! 3. Oracle creates attestations for specific values
//! 4. Other contracts verify and consume attestations

use darkfi_sdk::{
    crypto::pasta_prelude::*,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::OracleError,
    model::{AttestValueParamsV1, Oracle, PushValueParamsV1, RegisterOracleParamsV1},
    OracleFunction, ORACLE_CONTRACT_ATTESTATIONS_TREE, ORACLE_CONTRACT_INFO_TREE,
    ORACLE_CONTRACT_ORACLES_TREE,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize oracle contract state
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[oracle::init_contract] Initializing oracle contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, ORACLE_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, b"db_version", &env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize oracles tree
    wasm::db::db_init(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Initialize attestations tree
    wasm::db::db_init(cid, ORACLE_CONTRACT_ATTESTATIONS_TREE)?;

    msg!("[oracle::init_contract] Oracle contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = OracleFunction::try_from(self_.data[0])?;

    msg!("[oracle::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        OracleFunction::RegisterOracleV1 => {
            let params: RegisterOracleParamsV1 = deserialize(&self_.data[1..])?;
            register_oracle_get_metadata_v1(cid, call_idx, calls, params)?
        }
        OracleFunction::PushValueV1 => {
            let params: PushValueParamsV1 = deserialize(&self_.data[1..])?;
            push_value_get_metadata_v1(cid, call_idx, calls, params)?
        }
        OracleFunction::AttestValueV1 => {
            let params: AttestValueParamsV1 = deserialize(&self_.data[1..])?;
            attest_value_get_metadata_v1(cid, call_idx, calls, params)?
        }
    };

    wasm::util::set_return_data(&metadata)
}

fn register_oracle_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: RegisterOracleParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[oracle::register_oracle_get_metadata_v1] Registering oracle: {:?}", params.oracle_id);

    // Public inputs: oracle public key coordinates
    let mut public_inputs = vec![
        params.oracle_pub_x,
        params.oracle_pub_y,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    (crate::ORACLE_CONTRACT_ZKAS_REGISTER_ORACLE_NS_V1, &public_inputs).encode(&mut metadata)?;

    Ok(metadata)
}

fn push_value_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: PushValueParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[oracle::push_value_get_metadata_v1] Pushing value for oracle: {:?}", params.oracle_id);

    // Public inputs: oracle_id and value
    let mut public_inputs = vec![
        params.oracle_id,
        params.value,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    (crate::ORACLE_CONTRACT_ZKAS_PUSH_VALUE_NS_V1, &public_inputs).encode(&mut metadata)?;

    Ok(metadata)
}

fn attest_value_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: AttestValueParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[oracle::attest_value_get_metadata_v1] Attesting value for oracle: {:?}", params.oracle_id);

    // Public inputs: oracle_id, attestation_id, predicate, threshold
    let mut public_inputs = vec![
        params.oracle_id,
        params.attestation_id,
        pallas::Base::from(params.predicate),
        params.threshold,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    (crate::ORACLE_CONTRACT_ZKAS_ATTEST_VALUE_NS_V1, &public_inputs).encode(&mut metadata)?;

    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = OracleFunction::try_from(self_.data[0])?;

    msg!("[oracle::process_instruction] Processing function: {:?}", func);

    match func {
        OracleFunction::RegisterOracleV1 => {
            let params: RegisterOracleParamsV1 = deserialize(&self_.data[1..])?;
            register_oracle_v1(cid, params)
        }
        OracleFunction::PushValueV1 => {
            let params: PushValueParamsV1 = deserialize(&self_.data[1..])?;
            push_value_v1(cid, params)
        }
        OracleFunction::AttestValueV1 => {
            let params: AttestValueParamsV1 = deserialize(&self_.data[1..])?;
            attest_value_v1(cid, params)
        }
    }
}

fn register_oracle_v1(cid: ContractId, params: RegisterOracleParamsV1) -> ContractResult {
    msg!("[oracle::register_oracle_v1] Registering oracle: {:?}", params.oracle_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_REGISTER_ORACLE_NS_V1)?;

    let oracles_db = wasm::db::db_get(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Check if oracle already exists
    let existing: Option<Oracle> = wasm::db::db_get(oracles_db, &serialize(&params.oracle_id))?;
    if existing.is_some() {
        msg!("[oracle::register_oracle_v1] ERROR: Oracle already exists");
        return Err(ContractError::from(OracleError::OracleAlreadyExists).into())
    }

    // Get current block
    let current_block = wasm::chain::get_block_height()?;

    // Create oracle
    let oracle = Oracle {
        id: params.oracle_id,
        oracle_pubkey: [params.oracle_pub_x, params.oracle_pub_y],
        name: params.name.clone(),
        data_type: params.data_type.clone(),
        value: pallas::Base::zero(),
        updated_at: current_block,
        is_active: true,
    };

    wasm::db::db_set(oracles_db, &serialize(&params.oracle_id), &serialize(&oracle))?;

    msg!("[oracle::register_oracle_v1] Oracle registered successfully");
    Ok(())
}

fn push_value_v1(cid: ContractId, params: PushValueParamsV1) -> ContractResult {
    msg!("[oracle::push_value_v1] Pushing value for oracle: {:?}", params.oracle_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_PUSH_VALUE_NS_V1)?;

    let oracles_db = wasm::db::db_get(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Get and verify oracle exists
    let mut oracle: Oracle = match wasm::db::db_get(oracles_db, &serialize(&params.oracle_id))? {
        Some(o) => o,
        None => {
            msg!("[oracle::push_value_v1] ERROR: Oracle not found");
            return Err(ContractError::from(OracleError::OracleNotFound).into())
        }
    };

    // Verify oracle is active
    if !oracle.is_active {
        msg!("[oracle::push_value_v1] ERROR: Oracle not active");
        return Err(ContractError::from(OracleError::OracleNotActive).into())
    }

    // Update value
    let current_block = wasm::chain::get_block_height()?;
    oracle.value = params.value;
    oracle.updated_at = current_block;

    wasm::db::db_set(oracles_db, &serialize(&params.oracle_id), &serialize(&oracle))?;

    msg!("[oracle::push_value_v1] Value pushed successfully: {:?}", params.value);
    Ok(())
}

fn attest_value_v1(cid: ContractId, params: AttestValueParamsV1) -> ContractResult {
    msg!("[oracle::attest_value_v1] Attesting value for oracle: {:?}", params.oracle_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_ATTEST_VALUE_NS_V1)?;

    let oracles_db = wasm::db::db_get(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Get and verify oracle exists
    let oracle: Oracle = match wasm::db::db_get(oracles_db, &serialize(&params.oracle_id))? {
        Some(o) => o,
        None => {
            msg!("[oracle::attest_value_v1] ERROR: Oracle not found");
            return Err(ContractError::from(OracleError::OracleNotFound).into())
        }
    };

    // Verify oracle is active
    if !oracle.is_active {
        msg!("[oracle::attest_value_v1] ERROR: Oracle not active");
        return Err(ContractError::from(OracleError::OracleNotActive).into())
    }

    // Validate predicate type
    if params.predicate > 2 {
        msg!("[oracle::attest_value_v1] ERROR: Invalid predicate type");
        return Err(ContractError::from(OracleError::InvalidPredicate).into())
    }

    // Note: The actual attestation is created in the attestation contract.
    // This function verifies the oracle data and prepares for attestation.
    // The attestation_id references the attestation contract's record.

    msg!(
        "[oracle::attest_value_v1] Value attested: predicate={}, threshold={}",
        params.predicate,
        params.threshold
    );
    Ok(())
}

// ============================================================================
// PROCESS UPDATE
// ============================================================================

fn process_update(cid: ContractId, updates: &[u8]) -> ContractResult {
    let updates: Vec<DarkLeaf<pallas::Base>> = deserialize(updates)?;
    msg!("[oracle::process_update] Applying {} updates", updates.len());

    for update in updates {
        match update.data[0] {
            0 => {
                let oracle_id = update.data[1];
                msg!("[oracle::process_update] RegisterOracle: {:?}", oracle_id);
            }
            1 => {
                let oracle_id = update.data[1];
                let value = update.data[2];
                msg!("[oracle::process_update] PushValue: {:?} = {:?}", oracle_id, value);
            }
            2 => {
                let oracle_id = update.data[1];
                let attestation_id = update.data[2];
                msg!(
                    "[oracle::process_update] AttestValue: oracle={:?}, attestation={:?}",
                    oracle_id,
                    attestation_id
                );
            }
            _ => {
                msg!("[oracle::process_update] ERROR: Unknown update type");
                return Err(ContractError::InvalidInstruction.into())
            }
        }
    }

    msg!("[oracle::process_update] All updates applied successfully");
    Ok(())
}
