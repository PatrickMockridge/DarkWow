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

//! WASM entrypoint for the OTC swap contract
//!
//! ## OTC Swap Contract Overview
//!
//! Privacy-preserving peer-to-peer token swap. Alice creates a swap proposal,
//! funds it by locking her coins, and Bob completes the atomic exchange.
//!
//! ## Trust Model: Two-Phase Commit with Timeout
//!
//! - **Alice funds first** — locks her coins via child transfer
//! - **Bob executes** — locks his coins and releases both atomically
//! - **Alice can cancel** — before funding, or after timeout in Funded state
//! - A **spent nullifier** prevents double-execute or double-cancel
//!
//! ## Privacy Properties
//!
//! - Amounts hidden in Pedersen commitments
//! - Parties hidden (public keys derived from secrets)
//! - Execute/cancel linkable only via nullifiers

use dwow_sdk::{
    crypto::{
        pasta_prelude::*,
        poseidon_hash, ContractId,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    error::OtcSwapError,
    model::{
        CancelSwapParamsV1, CancelSwapUpdateV1, CreateSwapParamsV1, CreateSwapUpdateV1,
        ExecuteSwapParamsV1, ExecuteSwapUpdateV1, FundSwapParamsV1, FundSwapUpdateV1,
        OtcSwap, SwapState,
    },
    OtcSwapFunction, OTC_SWAP_CONTRACT_INFO_TREE, OTC_SWAP_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    OTC_SWAP_CONTRACT_NULLIFIERS_TREE,
    OTC_SWAP_CONTRACT_SWAPS_TREE,
    OTC_SWAP_CONTRACT_ZKAS_CREATE_NS_V2, OTC_SWAP_CONTRACT_ZKAS_FUND_NS_V2,
    OTC_SWAP_CONTRACT_ZKAS_EXECUTE_NS_V2, OTC_SWAP_CONTRACT_ZKAS_CANCEL_NS_V2,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const OTC_SWAP_DB_VERSION_KEY: &[u8] = b"db_version";

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize OTC swap contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Swaps tree (swap records)
/// - Nullifiers tree (spent nullifiers)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[otc_swap::init_contract] Initializing OTC swap contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, OTC_SWAP_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, OTC_SWAP_DB_VERSION_KEY, &env!("CARGO_PKG_VERSION").as_bytes())?;
    wasm::db::db_set(info_db, OTC_SWAP_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    // Initialize swaps tree
    wasm::db::db_init(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, OTC_SWAP_CONTRACT_NULLIFIERS_TREE)?;

    msg!("[otc_swap::init_contract] OTC swap contract initialized successfully");

    let _create_swap_v1_bincode = include_bytes!("../proof/create_swap.zk.bin");
    let _fund_swap_v1_bincode = include_bytes!("../proof/fund_swap.zk.bin");
    let _execute_swap_v1_bincode = include_bytes!("../proof/execute_swap.zk.bin");
    let _cancel_swap_v1_bincode = include_bytes!("../proof/cancel_swap.zk.bin");

    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = OtcSwapFunction::try_from(self_.data[0])?;

    msg!("[otc_swap::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        OtcSwapFunction::CreateSwapV1 => {
            let params= CreateSwapParamsV1::decode(&self_.data[1..])?;
            swap_create_get_metadata_v1(cid, call_idx, calls, params)?
        }
        OtcSwapFunction::FundSwapV1 => {
            let params= FundSwapParamsV1::decode(&self_.data[1..])?;
            swap_fund_get_metadata_v1(cid, call_idx, calls, params)?
        }
        OtcSwapFunction::ExecuteSwapV1 => {
            let params= ExecuteSwapParamsV1::decode(&self_.data[1..])?;
            swap_execute_get_metadata_v1(cid, call_idx, calls, params)?
        }
        OtcSwapFunction::CancelSwapV1 => {
            let params= CancelSwapParamsV1::decode(&self_.data[1..])?;
            swap_cancel_get_metadata_v1(cid, call_idx, calls, params)?
        }
        OtcSwapFunction::InitializeV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// `get_metadata` for CreateSwapV1
fn swap_create_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // Public inputs for CreateSwap ZK proof (4):
    //   constrain_instance(C) — commitment
    //   constrain_instance(tx_binding) — pass-through
    //   constrain_instance(tx_nonce) — pass-through
    //   constrain_instance(bob_commitment) — H(bob_pub.x, bob_pub.y)
    let (alice_x, alice_y) = params.alice_pubkey.xy().expect("pk not identity");
    let (bob_x, bob_y) = params.bob_pubkey.xy().expect("pk not identity");
    let bob_commitment = poseidon_hash([pallas::Base::from(4), bob_x, bob_y]);
    let commitment = poseidon_hash([
        pallas::Base::from(4),
        alice_x, alice_y, bob_commitment,
        pallas::Base::from(params.send_value), params.send_token_id,
        pallas::Base::from(params.recv_value), params.recv_token_id,
        pallas::Base::from(params.timeout),
    ]);

    zk_public_inputs.push((
        OTC_SWAP_CONTRACT_ZKAS_CREATE_NS_V2.to_string(),
        vec![commitment, pallas::Base::zero(), pallas::Base::zero(), bob_commitment],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for FundSwapV1
fn swap_fund_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: FundSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    let value_coords = params.value_commit.to_affine().coordinates();
    if value_coords.is_none().into() {
        return Err(OtcSwapError::InvalidCommitment.into());
    }
    let value_coords = value_coords.unwrap();

    // FundSwap circuit exposes:
    // - value_commit_x, value_commit_y
    // - swap_id
    // - tx_binding (pass-through)
    // - tx_nonce (pass-through)
    // - merkle_root
    zk_public_inputs.push((
        OTC_SWAP_CONTRACT_ZKAS_FUND_NS_V2.to_string(),
        vec![
            *value_coords.x(),
            *value_coords.y(),
            params.swap_id,
            pallas::Base::zero(),
            pallas::Base::zero(),
            params.merkle_root.inner(),
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for ExecuteSwapV1
fn swap_execute_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: ExecuteSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    let (bob_x, bob_y) = params.bob_recipient.xy().expect("pk not identity");
    let bob_commitment = poseidon_hash([pallas::Base::from(4), bob_x, bob_y]);

    // ExecuteSwap circuit exposes:
    //   constrain_instance(swap_id)
    //   constrain_instance(bob_commitment) — H(bob_pub)
    //   constrain_instance(tx_binding) — pass-through
    //   constrain_instance(tx_nonce) — pass-through
    //   constrain_instance(spent_nullifier)
    zk_public_inputs.push((
        OTC_SWAP_CONTRACT_ZKAS_EXECUTE_NS_V2.to_string(),
        vec![
            params.swap_id,
            bob_commitment,
            pallas::Base::zero(),
            pallas::Base::zero(),
            params.spent_nullifier,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for CancelSwapV1
fn swap_cancel_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CancelSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    let (alice_x, alice_y) = params.recipient_pubkey.xy().expect("pk not identity");

    // CancelSwap circuit exposes:
    //   constrain_instance(swap_id)
    //   constrain_instance(timeout)
    //   constrain_instance(current_block)
    //   constrain_instance(alice_x)
    //   constrain_instance(alice_y)
    //   constrain_instance(tx_binding) — pass-through
    //   constrain_instance(tx_nonce) — pass-through
    //   constrain_instance(spent_nullifier)
    zk_public_inputs.push((
        OTC_SWAP_CONTRACT_ZKAS_CANCEL_NS_V2.to_string(),
        vec![
            params.swap_id,
            pallas::Base::from(params.timeout),
            pallas::Base::from(params.current_block),
            alice_x,
            alice_y,
            pallas::Base::zero(),
            pallas::Base::zero(),
            params.spent_nullifier,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING (state transition verification)
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx];
    let func = OtcSwapFunction::try_from(self_.data.data[0])?;

    msg!("[otc_swap::process_instruction] Processing function: {:?}", func);

    match func {
        OtcSwapFunction::CreateSwapV1 => {
            let params = CreateSwapParamsV1::decode(&self_.data.data[1..])?;
            let update = swap_create_process_instruction_v1(cid, call_idx, calls, params)?;
            wasm::util::set_return_data(&update)?;
        }
        OtcSwapFunction::FundSwapV1 => {
            let params = FundSwapParamsV1::decode(&self_.data.data[1..])?;
            let update = swap_fund_process_instruction_v1(cid, call_idx, calls, params)?;
            wasm::util::set_return_data(&update)?;
        }
        OtcSwapFunction::ExecuteSwapV1 => {
            let params = ExecuteSwapParamsV1::decode(&self_.data.data[1..])?;
            let update = swap_execute_process_instruction_v1(cid, call_idx, calls, params)?;
            wasm::util::set_return_data(&update)?;
        }
        OtcSwapFunction::CancelSwapV1 => {
            let params = CancelSwapParamsV1::decode(&self_.data.data[1..])?;
            let update = swap_cancel_process_instruction_v1(cid, call_idx, calls, params)?;
            wasm::util::set_return_data(&update)?;
        }
        OtcSwapFunction::InitializeV1 => {
            msg!("[otc_swap::process_instruction] InitializeV1 has no instruction data");
            wasm::util::set_return_data(&[])?;
        }
    }

    Ok(())
}

/// `process_instruction` for CreateSwapV1
fn swap_create_process_instruction_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[CreateSwapV1] Processing instruction");

    let swaps_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;

    // Verify the swap doesn't already exist
    if wasm::db::db_contains_key(swaps_db, &params.commitment.to_repr())? {
        msg!("[CreateSwapV1] Error: Swap already exists");
        return Err(OtcSwapError::SwapAlreadyExists("commitment exists".to_string()).into())
    }

    // Create the swap record
    let swap = OtcSwap {
        version: 1,
        id: params.commitment,
        alice_pubkey: params.alice_pubkey,
        bob_pubkey: params.bob_pubkey,
        send_value: params.send_value,
        send_token_id: params.send_token_id,
        recv_value: params.recv_value,
        recv_token_id: params.recv_token_id,
        timeout: params.timeout,
        state: SwapState::Created,
        alice_value_commit: pallas::Point::identity(),
        alice_value_blind: pallas::Scalar::ZERO,
        bob_value_commit: pallas::Point::identity(),
        spent_nullifier: pallas::Base::ZERO,
        created_at: wasm::util::get_verifying_block_height()?.get(),
        funded_at: None,
        instance_seed: params.instance_seed,
    };

    let key = swap.id.to_repr();
    let value = swap.encode();
    wasm::db::db_set(swaps_db, &key, &value)?;

    let update = CreateSwapUpdateV1 { swap_id: swap.id };
    Ok(update.encode())
}

/// `process_instruction` for FundSwapV1
fn swap_fund_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: FundSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[FundSwapV1] Processing instruction for swap {:?}", params.swap_id);

    // Validate child call is promissory_note::transfer_v1 (0x04)
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[FundSwapV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(OtcSwapError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[FundSwapV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(OtcSwapError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, OTC_SWAP_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(OtcSwapError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let swaps_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;

    // Fetch the existing swap
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id.to_repr())?
        .ok_or_else(|| OtcSwapError::SwapNotFound(format!("{:?}", params.swap_id)))?;
    let mut swap: OtcSwap = OtcSwap::decode(&swap_data)?;

    // Verify the swap is in Created state
    if swap.state != SwapState::Created {
        msg!("[FundSwapV1] Error: Swap not in Created state");
        return Err(OtcSwapError::InvalidStateTransition.into())
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(swap.send_value),
        swap.id,
    ]);
    validate_child_value_commit(&child_call.data, swap.send_value, value_blind)?;

    // Update swap with funding details
    swap.alice_value_commit = params.value_commit;
    swap.state = SwapState::Funded;
    swap.funded_at = Some(wasm::util::get_verifying_block_height()?.get());

    let update = FundSwapUpdateV1 { swap_id: swap.id };
    Ok(update.encode())
}

/// `process_instruction` for ExecuteSwapV1
fn swap_execute_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: ExecuteSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[ExecuteSwapV1] Processing instruction for swap {:?}", params.swap_id);

    // Validate child call is promissory_note::transfer_v1 (0x04)
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[ExecuteSwapV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(OtcSwapError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[ExecuteSwapV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(OtcSwapError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, OTC_SWAP_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(OtcSwapError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let swaps_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_NULLIFIERS_TREE)?;

    // Fetch the existing swap
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id.to_repr())?
        .ok_or_else(|| OtcSwapError::SwapNotFound(format!("{:?}", params.swap_id)))?;
    let swap: OtcSwap = OtcSwap::decode(&swap_data)?;

    // Verify the swap is in Funded state
    if swap.state != SwapState::Funded {
        msg!("[ExecuteSwapV1] Error: Swap not in Funded state");
        return Err(OtcSwapError::InvalidStateTransition.into())
    }

    // Verify the swap hasn't already been resolved
    if wasm::db::db_contains_key(nullifiers_db, &params.spent_nullifier.to_repr())? {
        msg!("[ExecuteSwapV1] Error: Swap already resolved (nullifier exists)");
        return Err(OtcSwapError::AlreadySpent.into())
    }

    // Verify the nullifier matches what we expect
    let expected_nullifier = poseidon_hash([pallas::Base::from(1), swap.id, params.bob_secret]);
    if expected_nullifier != params.spent_nullifier {
        msg!("[ExecuteSwapV1] Error: Nullifier mismatch");
        return Err(OtcSwapError::InvalidNullifier.into())
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(swap.send_value),
        pallas::Base::from(swap.recv_value),
        swap.id,
    ]);
    validate_child_value_commit(&child_call.data, swap.send_value, value_blind)?;

    let update = ExecuteSwapUpdateV1 {
        swap_id: swap.id,
        spent_nullifier: params.spent_nullifier,
    };
    Ok(update.encode())
}

/// `process_instruction` for CancelSwapV1
fn swap_cancel_process_instruction_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CancelSwapParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[CancelSwapV1] Processing instruction for swap {:?}", params.swap_id);

    let swaps_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_NULLIFIERS_TREE)?;

    // Fetch the existing swap
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id.to_repr())?
        .ok_or_else(|| OtcSwapError::SwapNotFound(format!("{:?}", params.swap_id)))?;
    let swap: OtcSwap = OtcSwap::decode(&swap_data)?;

    // Verify the swap hasn't already been resolved
    if wasm::db::db_contains_key(nullifiers_db, &params.spent_nullifier.to_repr())? {
        msg!("[CancelSwapV1] Error: Swap already resolved (nullifier exists)");
        return Err(OtcSwapError::AlreadySpent.into())
    }

    // Can cancel from Created state without timeout
    if swap.state == SwapState::Created {
        // Verify Alice is the caller (nullifier from alice_secret)
        let expected_nullifier = poseidon_hash([pallas::Base::from(1), swap.id, params.alice_secret]);
        if expected_nullifier != params.spent_nullifier {
            msg!("[CancelSwapV1] Error: Nullifier mismatch");
            return Err(OtcSwapError::InvalidNullifier.into())
        }
    } else if swap.state == SwapState::Funded {
        // CRITICAL: Verify timelock has passed
        let current_block = wasm::util::get_verifying_block_height()?;
        if current_block.get() < swap.timeout {
            msg!(
                "[CancelSwapV1] Error: Timelock not reached (current: {}, timeout: {})",
                current_block,
                swap.timeout
            );
            return Err(OtcSwapError::TimelockNotExpired.into())
        }

        // Verify Alice is the caller
        let expected_nullifier = poseidon_hash([pallas::Base::from(1), swap.id, params.alice_secret]);
        if expected_nullifier != params.spent_nullifier {
            msg!("[CancelSwapV1] Error: Nullifier mismatch");
            return Err(OtcSwapError::InvalidNullifier.into())
        }
    } else {
        msg!("[CancelSwapV1] Error: Swap in terminal state");
        return Err(OtcSwapError::InvalidStateTransition.into())
    }

    let update = CancelSwapUpdateV1 {
        swap_id: swap.id,
        spent_nullifier: params.spent_nullifier,
    };
    Ok(update.encode())
}

// ============================================================================
// STATE UPDATE (write new state)
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = OtcSwapFunction::try_from(update_data[0])?;

    match func {
        OtcSwapFunction::CreateSwapV1 => {
            let update = CreateSwapUpdateV1::decode(&update_data[1..])?;
            swap_create_process_update_v1(cid, update)
        }
        OtcSwapFunction::FundSwapV1 => {
            let update = FundSwapUpdateV1::decode(&update_data[1..])?;
            swap_fund_process_update_v1(cid, update)
        }
        OtcSwapFunction::ExecuteSwapV1 => {
            let update = ExecuteSwapUpdateV1::decode(&update_data[1..])?;
            swap_execute_process_update_v1(cid, update)
        }
        OtcSwapFunction::CancelSwapV1 => {
            let update = CancelSwapUpdateV1::decode(&update_data[1..])?;
            swap_cancel_process_update_v1(cid, update)
        }
        OtcSwapFunction::InitializeV1 => {
            msg!("[otc_swap::process_update] InitializeV1 has no update data");
            Ok(())
        }
    }
}

/// `process_update` for CreateSwapV1
fn swap_create_process_update_v1(_cid: ContractId, update: CreateSwapUpdateV1) -> ContractResult {
    msg!("[CreateSwapV1] Swap {:?} created", update.swap_id);
    Ok(())
}

/// `process_update` for FundSwapV1
fn swap_fund_process_update_v1(cid: ContractId, update: FundSwapUpdateV1) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;

    let swap_data = wasm::db::db_get(swaps_db, &update.swap_id.to_repr())?
        .ok_or_else(|| OtcSwapError::SwapNotFound(format!("{:?}", update.swap_id)))?;
    let mut swap: OtcSwap = OtcSwap::decode(&swap_data)?;

    swap.state = SwapState::Funded;
    swap.funded_at = Some(wasm::util::get_verifying_block_height()?.get());

    wasm::db::db_set(swaps_db, &swap.id.to_repr(), &swap.encode())?;
    msg!("[FundSwapV1] Swap {:?} funded and state updated to Funded", update.swap_id);
    Ok(())
}

/// `process_update` for ExecuteSwapV1
fn swap_execute_process_update_v1(cid: ContractId, update: ExecuteSwapUpdateV1) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_NULLIFIERS_TREE)?;

    let swap_data = wasm::db::db_get(swaps_db, &update.swap_id.to_repr())?
        .ok_or_else(|| OtcSwapError::SwapNotFound(format!("{:?}", update.swap_id)))?;
    let mut swap: OtcSwap = OtcSwap::decode(&swap_data)?;

    swap.state = SwapState::Executed;
    swap.spent_nullifier = update.spent_nullifier;

    wasm::db::db_set(swaps_db, &swap.id.to_repr(), &swap.encode())?;

    // Record the spent nullifier to prevent double-spend
    wasm::db::db_mark_spent(nullifiers_db, &update.spent_nullifier.to_repr())?;

    msg!(
        "[ExecuteSwapV1] Swap {:?} executed, nullifier {:?} recorded",
        update.swap_id,
        update.spent_nullifier
    );
    Ok(())
}

/// `process_update` for CancelSwapV1
fn swap_cancel_process_update_v1(cid: ContractId, update: CancelSwapUpdateV1) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_SWAPS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, OTC_SWAP_CONTRACT_NULLIFIERS_TREE)?;

    let swap_data = wasm::db::db_get(swaps_db, &update.swap_id.to_repr())?
        .ok_or_else(|| OtcSwapError::SwapNotFound(format!("{:?}", update.swap_id)))?;
    let mut swap: OtcSwap = OtcSwap::decode(&swap_data)?;

    swap.state = SwapState::Cancelled;
    swap.spent_nullifier = update.spent_nullifier;

    wasm::db::db_set(swaps_db, &swap.id.to_repr(), &swap.encode())?;

    // Record the spent nullifier to prevent double-spend
    wasm::db::db_mark_spent(nullifiers_db, &update.spent_nullifier.to_repr())?;

    msg!(
        "[CancelSwapV1] Swap {:?} cancelled, nullifier {:?} recorded",
        update.swap_id,
        update.spent_nullifier
    );
    Ok(())
}
