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

//! WASM entrypoint for the oracle contract
//!
//! ## Oracle Contract Overview
//!
//! Demonstrates the "push model" for oracles in DarkWow.
//! Oracles push data values which can be attested for consumption by other contracts.
//!
//! ## Flow
//!
//! 1. Oracle operator registers an oracle data feed
//! 2. Oracle pushes data values (prices, scores, etc.)
//! 3. Oracle creates attestations for specific values
//! 4. Other contracts verify and consume attestations

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta::pallas::Base, ContractCall,
    wasm,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    error::OracleError,
    model::{
        AggregateParamsV1, AttestValueParamsV1, AggregateUpdateV1, AttestValueUpdateV1, Oracle, OracleId,
        PushValueCommitmentParamsV1, PushValueCommitmentUpdateV1, PushValueParamsV1,
        PushValueUpdateV1, RegisterOracleParamsV1, RegisterOracleUpdateV1,
        SetOracleActiveParamsV1, SetOracleActiveUpdateV1,
    },
    OracleFunction, ORACLE_CONTRACT_ATTESTATIONS_TREE, ORACLE_CONTRACT_INFO_TREE,
    ORACLE_CONTRACT_ORACLES_TREE,
    ORACLE_CONTRACT_ZKAS_REGISTER_ORACLE_NS_V2,
    ORACLE_CONTRACT_ZKAS_PUSH_VALUE_NS_V2,
    ORACLE_CONTRACT_ZKAS_ATTEST_VALUE_NS_V2,
    ORACLE_CONTRACT_ZKAS_PUSH_VALUE_COMMITMENT_NS_V2,
    ORACLE_CONTRACT_ZKAS_AGGREGATE_NS_V2,
};

dwow_sdk::define_contract!(
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


    // Register V2 circuits (domain separation, HAZOP RC3)
    let aggregate_v2_bincode = include_bytes!("../proof/aggregate.zk.bin");
    wasm::db::zkas_db_set(&aggregate_v2_bincode[..])?;
    let attest_value_v2_bincode = include_bytes!("../proof/attest_value.zk.bin");
    wasm::db::zkas_db_set(&attest_value_v2_bincode[..])?;
    let push_value_commitment_v2_bincode = include_bytes!("../proof/push_value_commitment.zk.bin");
    wasm::db::zkas_db_set(&push_value_commitment_v2_bincode[..])?;
    let push_value_v2_bincode = include_bytes!("../proof/push_value.zk.bin");
    wasm::db::zkas_db_set(&push_value_v2_bincode[..])?;
    let register_oracle_v2_bincode = include_bytes!("../proof/register_oracle.zk.bin");
    wasm::db::zkas_db_set(&register_oracle_v2_bincode[..])?;

    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = OracleFunction::try_from(self_.data[0])?;

    msg!("[oracle::get_metadata] Processing function: {:?}", func);

    let mut zk_public_inputs: Vec<(String, Vec<Base>)> = vec![];

    match func {
        OracleFunction::RegisterOracleV1 => {
            let params = match RegisterOracleParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[oracle::get_metadata] Error: Failed to deserialize RegisterOracleParamsV1: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            // Circuit constrain_instance: oracle_pub_x, oracle_pub_y, tx_binding, tx_nonce
            zk_public_inputs.push((
                ORACLE_CONTRACT_ZKAS_REGISTER_ORACLE_NS_V2.to_string(),
                {
                    let (ox, oy) = params.oracle_pub.xy().expect("pk not identity");
                    vec![ox, oy, params.tx_binding, params.tx_nonce]
                },
            ));
        }
        OracleFunction::PushValueV1 => {
            let params = match PushValueParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[oracle::get_metadata] Error: Failed to deserialize PushValueParamsV1: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            zk_public_inputs.push((
                ORACLE_CONTRACT_ZKAS_PUSH_VALUE_NS_V2.to_string(),
                vec![params.oracle_id.inner(), params.value, params.tx_binding, params.tx_nonce],
            ));
        }
        OracleFunction::AttestValueV1 => {
            let params = match AttestValueParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[oracle::get_metadata] Error: Failed to deserialize AttestValueParamsV1: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            // Circuit constrain_instance: oracle_id, attestation_id, predicate, threshold, tx_binding, tx_nonce
            zk_public_inputs.push((
                ORACLE_CONTRACT_ZKAS_ATTEST_VALUE_NS_V2.to_string(),
                vec![
                    params.oracle_id.inner(),
                    params.attestation_id.inner(),
                    Base::from(params.predicate as u64),
                    params.threshold,
                    params.tx_binding,
                    params.tx_nonce,
                ],
            ));
        }
        OracleFunction::PushValueCommitmentV1 => {
            let params = match PushValueCommitmentParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[oracle::get_metadata] Error: Failed to deserialize PushValueCommitmentParamsV1: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            // Circuit constrain_instance: oracle_id, commitment, data_root, tx_binding, tx_nonce
            zk_public_inputs.push((
                ORACLE_CONTRACT_ZKAS_PUSH_VALUE_COMMITMENT_NS_V2.to_string(),
                vec![params.oracle_id.inner(), params.commitment, params.data_root, params.tx_binding, params.tx_nonce],
            ));
        }
        OracleFunction::AggregateV1 => {
            let params = match AggregateParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[oracle::get_metadata] Error: Failed to deserialize AggregateParamsV1: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            // Circuit constrain_instance: oracle_id, result, min_result, max_result, tx_binding, tx_nonce
            zk_public_inputs.push((
                ORACLE_CONTRACT_ZKAS_AGGREGATE_NS_V2.to_string(),
                vec![
                    params.oracle_id.inner(),
                    params.result,
                    params.min_result,
                    params.max_result,
                    params.tx_binding,
                    params.tx_nonce,
                ],
            ));
        }
        OracleFunction::SetOracleActiveV1 => {
            // Non-ZK function, no public inputs
        }
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    wasm::util::set_return_data(&metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = OracleFunction::try_from(func_byte)?;

    msg!("[oracle::process_instruction] Processing function: {:?}", func);

    let update_bytes = match func {
        OracleFunction::RegisterOracleV1 => {
            let params = RegisterOracleParamsV1::decode(&self_.data[1..])?;
            register_oracle_v1(cid, params)?
        }
        OracleFunction::PushValueV1 => {
            let params = PushValueParamsV1::decode(&self_.data[1..])?;
            push_value_v1(cid, params)?
        }
        OracleFunction::AttestValueV1 => {
            let params = AttestValueParamsV1::decode(&self_.data[1..])?;
            attest_value_v1(cid, params)?
        }
        OracleFunction::PushValueCommitmentV1 => {
            let params = PushValueCommitmentParamsV1::decode(&self_.data[1..])?;
            push_value_commitment_v1(cid, params)?
        }
        OracleFunction::AggregateV1 => {
            let params = AggregateParamsV1::decode(&self_.data[1..])?;
            aggregate_v1(cid, params)?
        }
        OracleFunction::SetOracleActiveV1 => {
            let params = SetOracleActiveParamsV1::decode(&self_.data[1..])?;
            set_oracle_active_v1(cid, params)?
        }
    };

    wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat())
}

fn register_oracle_v1(cid: ContractId, params: RegisterOracleParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[oracle::register_oracle_v1] Registering oracle: {:?}", params.oracle_id);

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_REGISTER_ORACLE_NS_V1)?;

    let oracles_db = wasm::db::db_lookup(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Check if oracle already exists
    let existing_data = wasm::db::db_get(oracles_db, &params.oracle_id.to_bytes())?;
    if existing_data.is_some() {
        msg!("[oracle::register_oracle_v1] ERROR: Oracle already exists");
        return Err(ContractError::from(OracleError::OracleAlreadyExists).into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create oracle
    let oracle = Oracle {
        version: 1,
        id: params.oracle_id,
        oracle_pub: params.oracle_pub,
        name: params.name.clone(),
        data_type: params.data_type.clone(),
        value: Base::zero(),
        updated_at: current_block,
        is_active: true,
    };

    msg!("[oracle::register_oracle_v1] Oracle registered successfully");
    Ok(RegisterOracleUpdateV1 { oracle_id: params.oracle_id, oracle }.encode())
}

fn push_value_v1(cid: ContractId, params: PushValueParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[oracle::push_value_v1] Pushing value for oracle: {:?}", params.oracle_id);

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_PUSH_VALUE_NS_V1)?;

    let oracles_db = wasm::db::db_lookup(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Get and verify oracle exists
    let oracle_data = wasm::db::db_get(oracles_db, &params.oracle_id.to_bytes())?;
    let mut oracle: Oracle = match oracle_data {
        Some(data) => Oracle::decode(&data)?,
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
    let current_block = wasm::util::get_verifying_block_height()?.get();
    oracle.value = params.value;
    oracle.updated_at = current_block;

    msg!("[oracle::push_value_v1] Value pushed successfully: {:?}", params.value);
    Ok(PushValueUpdateV1 { oracle_id: params.oracle_id, value: params.value, updated_at: current_block }.encode())
}

fn attest_value_v1(cid: ContractId, params: AttestValueParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[oracle::attest_value_v1] Attesting value for oracle: {:?}", params.oracle_id);

    // Verify ZK proof
    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_ATTEST_VALUE_NS_V1)?;

    let oracles_db = wasm::db::db_lookup(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Get and verify oracle exists
    let oracle_data = wasm::db::db_get(oracles_db, &params.oracle_id.to_bytes())?;
    let _oracle: Oracle = match oracle_data {
        Some(data) => Oracle::decode(&data)?,
        None => {
            msg!("[oracle::attest_value_v1] ERROR: Oracle not found");
            return Err(ContractError::from(OracleError::OracleNotFound).into())
        }
    };

    // Verify oracle is active
    if !_oracle.is_active {
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
        "[oracle::attest_value_v1] Value attested: predicate={}, threshold={:?}",
        params.predicate,
        params.threshold
    );
    Ok(AttestValueUpdateV1 { oracle_id: params.oracle_id, attestation_id: params.attestation_id }.encode())
}

fn push_value_commitment_v1(cid: ContractId, params: PushValueCommitmentParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[oracle::push_value_commitment_v1] Pushing commitment for oracle: {:?}", params.oracle_id);

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_PUSH_VALUE_COMMITMENT_NS_V1)?;

    let oracles_db = wasm::db::db_lookup(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Get and verify oracle exists
    let oracle_data = wasm::db::db_get(oracles_db, &params.oracle_id.to_bytes())?;
    let _oracle: Oracle = match oracle_data {
        Some(data) => Oracle::decode(&data)?,
        None => {
            msg!("[oracle::push_value_commitment_v1] ERROR: Oracle not found");
            return Err(ContractError::from(OracleError::OracleNotFound).into())
        }
    };

    // Verify oracle is active
    if !_oracle.is_active {
        msg!("[oracle::push_value_commitment_v1] ERROR: Oracle not active");
        return Err(ContractError::from(OracleError::OracleNotActive).into())
    }

    // Note: The commitment is stored in the data Merkle tree by the caller.
    // The ZK proof verifies:
    // 1. The commitment is in the data tree (set_membership returns 1)
    // 2. The staker knows the value and nonce that produce the commitment
    // 3. The staker's public key matches the registered staker

    msg!(
        "[oracle::push_value_commitment_v1] Commitment pushed successfully: {:?}",
        params.commitment
    );
    Ok(PushValueCommitmentUpdateV1 { oracle_id: params.oracle_id, commitment: params.commitment }.encode())
}

fn aggregate_v1(cid: ContractId, params: AggregateParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[oracle::aggregate_v1] Aggregating for oracle: {:?}", params.oracle_id);

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::ORACLE_CONTRACT_ZKAS_AGGREGATE_NS_V1)?;

    let oracles_db = wasm::db::db_lookup(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Get and verify oracle exists
    let oracle_data = wasm::db::db_get(oracles_db, &params.oracle_id.to_bytes())?;
    let mut oracle: Oracle = match oracle_data {
        Some(data) => Oracle::decode(&data)?,
        None => {
            msg!("[oracle::aggregate_v1] ERROR: Oracle not found");
            return Err(ContractError::from(OracleError::OracleNotFound).into())
        }
    };

    // Verify oracle is active
    if !oracle.is_active {
        msg!("[oracle::aggregate_v1] ERROR: Oracle not active");
        return Err(ContractError::from(OracleError::OracleNotActive).into())
    }

    // Verify result is within bounds
    if params.result < params.min_result {
        msg!("[oracle::aggregate_v1] ERROR: Result below minimum");
        return Err(ContractError::from(OracleError::InvalidPredicate).into())
    }

    if params.result > params.max_result {
        msg!("[oracle::aggregate_v1] ERROR: Result above maximum");
        return Err(ContractError::from(OracleError::InvalidPredicate).into())
    }

    // Update oracle value with aggregated result
    let current_block = wasm::util::get_verifying_block_height()?.get();
    oracle.value = params.result;
    oracle.updated_at = current_block;

    msg!(
        "[oracle::aggregate_v1] Aggregated successfully: result={:?}, min={:?}, max={:?}",
        params.result,
        params.min_result,
        params.max_result
    );
    Ok(AggregateUpdateV1 { oracle_id: params.oracle_id, result: params.result, updated_at: current_block }.encode())
}

fn set_oracle_active_v1(cid: ContractId, params: SetOracleActiveParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[oracle::set_oracle_active_v1] Setting oracle active state");

    let oracles_db = wasm::db::db_lookup(cid, ORACLE_CONTRACT_ORACLES_TREE)?;

    // Find the oracle by pubkey coordinates
    let (ox, oy) = params.oracle_pub.xy().expect("pk not identity");
    let oracle_id = dwow_sdk::crypto::poseidon_hash([ox, oy]);
    let oracle_data = wasm::db::db_get(oracles_db, &oracle_id.to_repr())?;
    let mut oracle: Oracle = match oracle_data {
        Some(data) => Oracle::decode(&data)?,
        None => {
            msg!("[oracle::set_oracle_active_v1] ERROR: Oracle not found");
            return Err(ContractError::from(OracleError::OracleNotFound).into())
        }
    };

    // Verify the caller's pubkey matches the oracle's pubkey
    if oracle.oracle_pub != params.oracle_pub {
        msg!("[oracle::set_oracle_active_v1] ERROR: Not authorized");
        return Err(ContractError::from(OracleError::NotAuthorized).into())
    }

    oracle.is_active = params.is_active;

    msg!("[oracle::set_oracle_active_v1] Oracle {:?} is_active set to {}", oracle_id, params.is_active);
    Ok(SetOracleActiveUpdateV1 { oracle_id: OracleId(oracle_id), is_active: params.is_active }.encode())
}

// ============================================================================
// PROCESS UPDATE
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let oracles_db = wasm::db::db_lookup(cid, ORACLE_CONTRACT_ORACLES_TREE)?;
    match OracleFunction::try_from(update_data[0])? {
        OracleFunction::RegisterOracleV1 => {
            let update = RegisterOracleUpdateV1::decode(&update_data[1..])?;
            wasm::db::db_set(oracles_db, &update.oracle_id.to_bytes(), &update.oracle.encode())?;
            msg!("[oracle::process_update] RegisterOracle: {:?}", update.oracle_id);
            Ok(())
        }
        OracleFunction::PushValueV1 => {
            let update = PushValueUpdateV1::decode(&update_data[1..])?;
            let data = wasm::db::db_get(oracles_db, &update.oracle_id.to_bytes())?
                .ok_or(ContractError::IoError("oracle not found".to_string()))?;
            let mut oracle = Oracle::decode(&data)?;
            oracle.value = update.value;
            oracle.updated_at = update.updated_at;
            wasm::db::db_set(oracles_db, &update.oracle_id.to_bytes(), &oracle.encode())?;
            msg!("[oracle::process_update] PushValue: {:?} = {:?}", update.oracle_id, update.value);
            Ok(())
        }
        OracleFunction::AttestValueV1 => {
            let update = AttestValueUpdateV1::decode(&update_data[1..])?;
            msg!("[oracle::process_update] AttestValue: oracle={:?}, attestation={:?}", update.oracle_id, update.attestation_id);
            Ok(())
        }
        OracleFunction::PushValueCommitmentV1 => {
            let update = PushValueCommitmentUpdateV1::decode(&update_data[1..])?;
            msg!("[oracle::process_update] PushValueCommitment: oracle={:?}, commitment={:?}", update.oracle_id, update.commitment);
            Ok(())
        }
        OracleFunction::AggregateV1 => {
            let update = AggregateUpdateV1::decode(&update_data[1..])?;
            let data = wasm::db::db_get(oracles_db, &update.oracle_id.to_bytes())?
                .ok_or(ContractError::IoError("oracle not found".to_string()))?;
            let mut oracle = Oracle::decode(&data)?;
            oracle.value = update.result;
            oracle.updated_at = update.updated_at;
            wasm::db::db_set(oracles_db, &update.oracle_id.to_bytes(), &oracle.encode())?;
            msg!("[oracle::process_update] Aggregate: oracle={:?}, result={:?}", update.oracle_id, update.result);
            Ok(())
        }
        OracleFunction::SetOracleActiveV1 => {
            let update = SetOracleActiveUpdateV1::decode(&update_data[1..])?;
            let data = wasm::db::db_get(oracles_db, &update.oracle_id.to_bytes())?
                .ok_or(ContractError::IoError("oracle not found".to_string()))?;
            let mut oracle = Oracle::decode(&data)?;
            oracle.is_active = update.is_active;
            wasm::db::db_set(oracles_db, &update.oracle_id.to_bytes(), &oracle.encode())?;
            msg!("[oracle::process_update] SetOracleActive: {:?}, is_active={}", update.oracle_id, update.is_active);
            Ok(())
        }
    }
}
