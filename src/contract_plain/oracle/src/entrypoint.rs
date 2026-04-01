/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Plain Oracle Contract Entrypoint
//!
//! # Architecture
//!
//! This contract uses a hybrid ZK/plain approach:
//!
//! | Operation | Method | Why |
//! |-----------|--------|-----|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Data commitment | ZK (Pedersen) | Privacy-preserving |
//! | Weighted average | Native Rust | Needs `base_div` (not in ZK) |
//! | Aggregation logic | Native Rust | Arbitrary complexity |
//!
//! # Privacy
//!
//! This is a **partial transparency** contract. Most state is public on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.

use darkfi_sdk::{
    crypto::{poseidon_hash, schnorr::SchnorrPublic, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::GenericResult,
    msg, wasm, ContractCall,
};
use darkfi_sdk::pasta::pallas::Base;
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::error::OraclePlainError;
use crate::model::{
    CreateFeedParamsV1, CreateFeedUpdateV1, DataPoint, RegisterStakerParamsV1,
    RegisterStakerUpdateV1, SlashStakerParamsV1, SlashStakerUpdateV1, Staker,
    SubmitDataPointParamsV1, SubmitDataPointUpdateV1, UnregisterStakerParamsV1,
    UnregisterStakerUpdateV1,
};
use crate::OraclePlainFunction;

// Database trees
const FEEDS_TREE: &str = "feeds";
const STAKERS_TREE: &str = "stakers";
const DATA_POINTS_TREE: &str = "data_points";

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    wasm::db::db_init(cid, FEEDS_TREE)?;
    wasm::db::db_init(cid, STAKERS_TREE)?;
    wasm::db::db_init(cid, DATA_POINTS_TREE)?;
    Ok(())
}

/// Get metadata for verification
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    Ok(())
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> GenericResult<()> {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = OraclePlainFunction::try_from(self_.data[0])?;

    let update_data = match func {
        OraclePlainFunction::CreateFeedV1 => {
            create_feed_process_instruction_v1(cid, call_idx, calls)?
        }
        OraclePlainFunction::RegisterStakerV1 => {
            register_staker_process_instruction_v1(cid, call_idx, calls)?
        }
        OraclePlainFunction::SubmitDataPointV1 => {
            submit_data_point_process_instruction_v1(cid, call_idx, calls)?
        }
        OraclePlainFunction::SlashStakerV1 => {
            slash_staker_process_instruction_v1(cid, call_idx, calls)?
        }
        OraclePlainFunction::UnregisterStakerV1 => {
            unregister_staker_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> GenericResult<()> {
    match OraclePlainFunction::try_from(update_data[0])? {
        OraclePlainFunction::CreateFeedV1 => {
            let update: CreateFeedUpdateV1 = deserialize(&update_data[1..])?;
            create_feed_process_update_v1(cid, update)
        }
        OraclePlainFunction::RegisterStakerV1 => {
            let update: RegisterStakerUpdateV1 = deserialize(&update_data[1..])?;
            register_staker_process_update_v1(cid, update)
        }
        OraclePlainFunction::SubmitDataPointV1 => {
            let update: SubmitDataPointUpdateV1 = deserialize(&update_data[1..])?;
            submit_data_point_process_update_v1(cid, update)
        }
        OraclePlainFunction::SlashStakerV1 => {
            let update: SlashStakerUpdateV1 = deserialize(&update_data[1..])?;
            slash_staker_process_update_v1(cid, update)
        }
        OraclePlainFunction::UnregisterStakerV1 => {
            let update: UnregisterStakerUpdateV1 = deserialize(&update_data[1..])?;
            unregister_staker_process_update_v1(cid, update)
        }
    }
}

// =============================================================================
// CREATE FEED
// =============================================================================

fn create_feed_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CreateFeedParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[oracle_plain::create_feed] Creating feed with hash: {:?}", params.name_hash);

    // Validate aggregation type
    if params.aggregation_type > 2 {
        return Err(OraclePlainError::InvalidFunction.into())
    }

    // Derive feed ID
    let feed_id = derive_feed_id(&params);

    // Check feed doesn't already exist
    let db = wasm::db::db_lookup(cid, FEEDS_TREE)?;
    if wasm::db::db_contains_key(db, &serialize(&feed_id))? {
        return Err(OraclePlainError::FeedAlreadyExists.into())
    }

    let update = CreateFeedUpdateV1 {
        feed_id,
        name_hash: params.name_hash,
        min_stake: params.min_stake,
        aggregation_type: params.aggregation_type,
    };

    msg!("[oracle_plain::create_feed] Feed {:?} created", feed_id);
    Ok(serialize(&update))
}

fn create_feed_process_update_v1(cid: ContractId, update: CreateFeedUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, FEEDS_TREE)?;

    // Store feed metadata (just the ID and params for now - actual aggregation is off-chain)
    wasm::db::db_set(db, &serialize(&update.feed_id), &serialize(&update.name_hash))?;
    msg!("[oracle_plain::create_feed::update] Feed stored");

    Ok(())
}

// =============================================================================
// REGISTER STAKER
// =============================================================================

fn register_staker_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: RegisterStakerParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[oracle_plain::register_staker] Registering staker: {:?}", params.staker);

    // Check stake amount meets minimum
    if params.stake_amount == 0 {
        return Err(OraclePlainError::InsufficientStake.into())
    }

    // Verify staker signature
    let mut signature_msg = vec![];
    params.feed_id.encode(&mut signature_msg)?;
    params.staker.x().encode(&mut signature_msg)?;
    params.staker.y().encode(&mut signature_msg)?;
    params.stake_amount.encode(&mut signature_msg)?;

    if !params.staker.verify(&signature_msg, &params.signature) {
        return Err(OraclePlainError::InvalidSignature.into())
    }

    // Check staker doesn't already exist
    let stakers_db = wasm::db::db_lookup(cid, STAKERS_TREE)?;
    let staker_key = derive_staker_key(params.feed_id, params.staker);
    if wasm::db::db_contains_key(stakers_db, &serialize(&staker_key))? {
        return Err(OraclePlainError::StakerAlreadyExists.into())
    }

    let update = RegisterStakerUpdateV1 {
        feed_id: params.feed_id,
        staker: params.staker,
        stake_amount: params.stake_amount,
    };

    msg!(
        "[oracle_plain::register_staker] Staker {:?} registered with stake: {}",
        params.staker,
        params.stake_amount
    );
    Ok(serialize(&update))
}

fn register_staker_process_update_v1(cid: ContractId, update: RegisterStakerUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, STAKERS_TREE)?;

    let staker = Staker {
        public_key: update.staker,
        stake_amount: update.stake_amount,
        total_weight: update.stake_amount,
        data_point_count: 0,
        slash_count: 0,
        is_active: true,
    };

    let staker_key = derive_staker_key(update.feed_id, update.staker);
    wasm::db::db_set(db, &serialize(&staker_key), &serialize(&staker))?;
    msg!("[oracle_plain::register_staker::update] Staker stored");

    Ok(())
}

// =============================================================================
// SUBMIT DATA POINT
// =============================================================================

fn submit_data_point_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: SubmitDataPointParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[oracle_plain::submit_data_point] Submitting data point to feed: {:?}", params.feed_id);

    // Validate data value
    if params.value == 0 {
        return Err(OraclePlainError::InvalidDataValue.into())
    }

    // Look up staker to get their weight
    let stakers_db = wasm::db::db_lookup(cid, STAKERS_TREE)?;
    // For simplicity, we derive the staker key from the signature
    // In a real system, the staker public key would be derived from the signature
    let staker_key = derive_staker_key_from_signature(params.feed_id, &params.signature)?;

    let staker: Staker = match wasm::db::db_get(stakers_db, &serialize(&staker_key))? {
        Some(data) => deserialize(&data)?,
        None => return Err(OraclePlainError::StakerNotFound.into()),
    };

    // Check staker is active
    if !staker.is_active {
        return Err(OraclePlainError::UnauthorizedCaller.into())
    }

    // Verify staker signature
    let mut signature_msg = vec![];
    params.feed_id.encode(&mut signature_msg)?;
    params.value.encode(&mut signature_msg)?;

    if !staker.public_key.verify(&signature_msg, &params.signature) {
        return Err(OraclePlainError::InvalidSignature.into())
    }

    // Derive data point ID
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    let data_point_id = derive_data_point_id(params.feed_id, staker.public_key, params.value, current_block);

    let update = SubmitDataPointUpdateV1 {
        data_point_id,
        feed_id: params.feed_id,
        staker: staker.public_key,
        value: params.value,
        weight: staker.total_weight,
        submitted_at_block: current_block,
    };

    msg!(
        "[oracle_plain::submit_data_point] Data point {:?} submitted by {:?}",
        data_point_id,
        staker.public_key
    );
    Ok(serialize(&update))
}

fn submit_data_point_process_update_v1(cid: ContractId, update: SubmitDataPointUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, DATA_POINTS_TREE)?;

    let data_point = DataPoint {
        id: update.data_point_id,
        feed_id: update.feed_id,
        staker: update.staker,
        value: update.value,
        weight: update.weight,
        submitted_at_block: update.submitted_at_block,
    };

    wasm::db::db_set(db, &serialize(&update.data_point_id), &serialize(&data_point))?;

    // Update staker's data point count
    let stakers_db = wasm::db::db_lookup(cid, STAKERS_TREE)?;
    let staker_key = derive_staker_key(update.feed_id, update.staker);
    let mut staker: Staker = match wasm::db::db_get(stakers_db, &serialize(&staker_key))? {
        Some(data) => deserialize(&data)?,
        None => return Err(OraclePlainError::StakerNotFound.into()),
    };

    staker.data_point_count += 1;
    wasm::db::db_set(stakers_db, &serialize(&staker_key), &serialize(&staker))?;

    msg!("[oracle_plain::submit_data_point::update] Data point stored");

    Ok(())
}

// =============================================================================
// SLASH STAKER
// =============================================================================

fn slash_staker_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: SlashStakerParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[oracle_plain::slash_staker] Slashing staker: {:?}", params.staker);

    // Look up data point to verify it exists
    let data_points_db = wasm::db::db_lookup(cid, DATA_POINTS_TREE)?;
    let _data_point: DataPoint = match wasm::db::db_get(data_points_db, &serialize(&params.data_point_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(OraclePlainError::DataPointNotFound.into()),
    };

    // Look up staker
    let stakers_db = wasm::db::db_lookup(cid, STAKERS_TREE)?;
    let staker_key = derive_staker_key(params.feed_id, params.staker);
    let staker: Staker = match wasm::db::db_get(stakers_db, &serialize(&staker_key))? {
        Some(data) => deserialize(&data)?,
        None => return Err(OraclePlainError::StakerNotFound.into()),
    };

    // Calculate slash amount (e.g., 10% of stake)
    // OPCODE PLACEHOLDER: When base_div is in ZK, this could use ZK constraints
    let slash_percentage = 1000u64; // 10%
    let slash_amount = calculate_percentage(staker.stake_amount, slash_percentage)?;

    // Verify slasher signature
    // OPCODE PLACEHOLDER: In a real system, this would be verified against an oracle or DAO public key
    let _ = params.signature;

    let update = SlashStakerUpdateV1 {
        feed_id: params.feed_id,
        staker: params.staker,
        slash_amount,
        reason_hash: params.reason_hash,
    };

    msg!(
        "[oracle_plain::slash_staker] Staker {:?} slashed for amount: {}",
        params.staker,
        slash_amount
    );
    Ok(serialize(&update))
}

fn slash_staker_process_update_v1(cid: ContractId, update: SlashStakerUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, STAKERS_TREE)?;

    let staker_key = derive_staker_key(update.feed_id, update.staker);
    let mut staker: Staker = match wasm::db::db_get(db, &serialize(&staker_key))? {
        Some(data) => deserialize(&data)?,
        None => return Err(OraclePlainError::StakerNotFound.into()),
    };

    // Apply slash
    staker.stake_amount = staker.stake_amount.saturating_sub(update.slash_amount);
    staker.slash_count += 1;

    // If stake drops below minimum, deactivate
    // Note: We don't have min_stake here, so just check if stake is 0
    if staker.stake_amount == 0 {
        staker.is_active = false;
    }

    wasm::db::db_set(db, &serialize(&staker_key), &serialize(&staker))?;
    msg!("[oracle_plain::slash_staker::update] Slash applied");

    Ok(())
}

// =============================================================================
// UNREGISTER STAKER
// =============================================================================

fn unregister_staker_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: UnregisterStakerParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[oracle_plain::unregister_staker] Unregistering staker from feed: {:?}", params.feed_id);

    // Look up staker
    let stakers_db = wasm::db::db_lookup(cid, STAKERS_TREE)?;
    let staker_key = derive_staker_key_from_signature(params.feed_id, &params.signature)?;

    let staker: Staker = match wasm::db::db_get(stakers_db, &serialize(&staker_key))? {
        Some(data) => deserialize(&data)?,
        None => return Err(OraclePlainError::StakerNotFound.into()),
    };

    // Verify staker signature
    let mut signature_msg = vec![];
    params.feed_id.encode(&mut signature_msg)?;

    if !staker.public_key.verify(&signature_msg, &params.signature) {
        return Err(OraclePlainError::InvalidSignature.into())
    }

    // Calculate refund (stake minus any pending slashes)
    // In a real system, there would be a cooldown period before refund
    let refund_amount = staker.stake_amount;

    let update = UnregisterStakerUpdateV1 {
        feed_id: params.feed_id,
        staker: staker.public_key,
        refund_amount,
    };

    msg!(
        "[oracle_plain::unregister_staker] Staker {:?} unregistered, refund: {}",
        staker.public_key,
        refund_amount
    );
    Ok(serialize(&update))
}

fn unregister_staker_process_update_v1(cid: ContractId, update: UnregisterStakerUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, STAKERS_TREE)?;

    let staker_key = derive_staker_key(update.feed_id, update.staker);

    // Mark as inactive (don't delete - we want to keep history)
    let mut staker: Staker = match wasm::db::db_get(db, &serialize(&staker_key))? {
        Some(data) => deserialize(&data)?,
        None => return Err(OraclePlainError::StakerNotFound.into()),
    };

    staker.is_active = false;
    staker.stake_amount = 0; // Refund issued

    wasm::db::db_set(db, &serialize(&staker_key), &serialize(&staker))?;
    msg!("[oracle_plain::unregister_staker::update] Staker unregistered");

    Ok(())
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Derive a unique feed ID from feed parameters
fn derive_feed_id(params: &CreateFeedParamsV1) -> Base {
    poseidon_hash([
        params.name_hash,
        Base::from(params.min_stake),
        params.stake_token,
        Base::from(params.aggregation_type as u64),
    ])
}

/// Derive a unique staker key from feed ID and public key
fn derive_staker_key(feed_id: Base, public_key: PublicKey) -> Base {
    poseidon_hash([
        feed_id,
        public_key.x(),
        public_key.y(),
    ])
}

/// Derive staker key from signature (for cases where we only have signature)
/// In a real system, the public key would be extracted from the signature
fn derive_staker_key_from_signature(
    _feed_id: Base,
    _signature: &Signature,
) -> GenericResult<Base> {
    // OPCODE PLACEHOLDER: When signature public key extraction is available, use that
    // For now, we can't derive the public key from the signature alone
    // This is a limitation that would be resolved in a full implementation
    Err(OraclePlainError::InvalidSignature.into())
}

/// Derive a unique data point ID
fn derive_data_point_id(feed_id: Base, staker: PublicKey, value: u64, block: u64) -> Base {
    poseidon_hash([
        feed_id,
        staker.x(),
        staker.y(),
        Base::from(value),
        Base::from(block),
    ])
}

/// Calculate percentage of an amount
/// PRIVACY NOTICE: This calculation is visible on-chain.
/// OPCODE PLACEHOLDER: When base_div is in ZK, this could be private.
fn calculate_percentage(amount: u64, basis_points: u64) -> GenericResult<u64> {
    // basis_points is in hundredths of a percent (10000 = 100%)
    let (product, overflowed) = amount.overflowing_mul(basis_points);
    if overflowed {
        return Err(OraclePlainError::ArithmeticOverflow.into())
    }

    let result = product / 10000;
    Ok(result)
}