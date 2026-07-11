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

//! WASM entrypoint for the escrow contract
//!
//! ## Escrow Contract Overview
//!
//! Privacy-preserving conditional payment contract. Funds are locked in a
//! commitment and released to the seller upon proof of knowledge of a secret,
//! or returned to the buyer after a timeout.
//!
//! ## Trust Model: Hashed Timelock (Variant 3)
//!
//! - **Seller claims** by proving knowledge of `seller_secret`
//! - **Buyer refunds** after `timeout` by proving knowledge of `buyer_secret`
//! - A **spent flag** prevents both claim and refund from succeeding
//!
//! ## Privacy Properties
//!
//! - Amount hidden in Pedersen commitment
//! - Parties hidden (public keys derived from secrets)
//! - Claim/refund linkable only via nullifiers

use dwow_sdk::{
    crypto::{
        pasta_prelude::*,
        poseidon_hash, ContractId,
        BOX_CONTRACT_ID, PURSE_CONTRACT_ID,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::{validate_child_contract_id, validate_child_value_commit};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::EscrowError,
    model::{
        CancelEscrowParamsV1, CancelEscrowUpdateV1, ClaimEscrowParamsV1, ClaimEscrowUpdateV1,
        CreateEscrowParamsV1, CreateEscrowUpdateV1, Escrow, EscrowState, FundEscrowParamsV1,
        FundEscrowUpdateV1, RefundEscrowParamsV1, RefundEscrowUpdateV1,
    },
    EscrowFunction, ESCROW_CONTRACT_ESCROWS_TREE, ESCROW_CONTRACT_INFO_TREE,
    ESCROW_CONTRACT_NULLIFIERS_TREE, ESCROW_CONTRACT_SPENT_FLAGS_TREE,
    PROMISSORY_NOTE_CONTRACT_ID_KEY, PURSE_CONTRACT_ID_KEY, BOX_CONTRACT_ID_KEY,
    ESCROW_CONTRACT_ZKAS_CLAIM_NS_V1, ESCROW_CONTRACT_ZKAS_CREATE_NS_V1,
    ESCROW_CONTRACT_ZKAS_CANCEL_NS_V1, ESCROW_CONTRACT_ZKAS_FUND_NS_V1,
    ESCROW_CONTRACT_ZKAS_REFUND_NS_V1,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const ESCROW_DB_VERSION_KEY: &[u8] = b"db_version";

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize escrow contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Escrows tree (escrow records)
/// - Nullifiers tree (spent nullifiers)
/// - Spent flags tree (prevents double-claim/refund)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[escrow::init_contract] Initializing escrow contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, ESCROW_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, ESCROW_DB_VERSION_KEY, &env!("CARGO_PKG_VERSION").as_bytes())?;
    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY, &[0u8; 32])?;
    wasm::db::db_set(info_db, PURSE_CONTRACT_ID_KEY, &PURSE_CONTRACT_ID.to_bytes())?;
    wasm::db::db_set(info_db, BOX_CONTRACT_ID_KEY, &BOX_CONTRACT_ID.to_bytes())?;

    // Initialize escrows tree
    wasm::db::db_init(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, ESCROW_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize spent flags tree
    wasm::db::db_init(cid, ESCROW_CONTRACT_SPENT_FLAGS_TREE)?;

    msg!("[escrow::init_contract] Escrow contract initialized successfully");

    let claim_v1_bincode = include_bytes!("../proof/claim_v1.zk.bin");
    wasm::db::zkas_db_set(&claim_v1_bincode[..])?;
    let create_escrow_v1_bincode = include_bytes!("../proof/create_escrow_v1.zk.bin");
    wasm::db::zkas_db_set(&create_escrow_v1_bincode[..])?;
    let fund_v1_bincode = include_bytes!("../proof/fund_v1.zk.bin");
    wasm::db::zkas_db_set(&fund_v1_bincode[..])?;
    let refund_v1_bincode = include_bytes!("../proof/refund_v1.zk.bin");
    wasm::db::zkas_db_set(&refund_v1_bincode[..])?;

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
    let func = EscrowFunction::try_from(self_.data[0])?;

    msg!("[escrow::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        EscrowFunction::CreateEscrowV1 => {
            let params: CreateEscrowParamsV1 = deserialize(&self_.data[1..])?;
            escrow_create_get_metadata_v1(cid, call_idx, calls, params)?
        }
        EscrowFunction::FundV1 => {
            let params: FundEscrowParamsV1 = deserialize(&self_.data[1..])?;
            escrow_fund_get_metadata_v1(cid, call_idx, calls, params)?
        }
        EscrowFunction::ClaimV1 => {
            let params: ClaimEscrowParamsV1 = deserialize(&self_.data[1..])?;
            escrow_claim_get_metadata_v1(cid, call_idx, calls, params)?
        }
        EscrowFunction::RefundV1 => {
            let params: RefundEscrowParamsV1 = deserialize(&self_.data[1..])?;
            escrow_refund_get_metadata_v1(cid, call_idx, calls, params)?
        }
        EscrowFunction::CancelV1 => {
            let params: CancelEscrowParamsV1 = deserialize(&self_.data[1..])?;
            escrow_cancel_get_metadata_v1(cid, call_idx, calls, params)?
        }
        EscrowFunction::InitializeV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// `get_metadata` for CreateEscrowV1
fn escrow_create_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    // Public inputs for CreateEscrow ZK proof
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // Circuit constrain_instance calls (2):
    //   constrain_instance(C) — commitment = H(buyer_x, buyer_y, H(seller), value, token_id, timeout)
    //   constrain_instance(seller_commitment) — H(seller_x, seller_y)
    let (buyer_x, buyer_y) = params.buyer_pubkey.xy().expect("pk not identity");
    let (seller_x, seller_y) = params.seller_pubkey.xy().expect("pk not identity");
    let seller_commitment = poseidon_hash([seller_x, seller_y]);
    let commitment = poseidon_hash([
        buyer_x, buyer_y, seller_commitment,
        pallas::Base::from(params.value), params.token_id,
        pallas::Base::from(params.timeout),
    ]);

    zk_public_inputs.push((
        ESCROW_CONTRACT_ZKAS_CREATE_NS_V1.to_string(),
        vec![commitment, seller_commitment],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for FundV1
fn escrow_fund_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: FundEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    // Public inputs for FundEscrow ZK proof
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // Get value commitment coordinates
    let value_coords = params.value_commit.to_affine().coordinates();
    if value_coords.is_none().into() {
        return Err(EscrowError::InvalidCommitment.into());
    }
    let value_coords = value_coords.unwrap();

    // FundEscrow circuit expects:
    // - value_commit_x, value_commit_y (Pedersen commitment coordinates)
    // - escrow_id
    // - merkle_root
    zk_public_inputs.push((
        ESCROW_CONTRACT_ZKAS_FUND_NS_V1.to_string(),
        vec![
            *value_coords.x(),
            *value_coords.y(),
            params.escrow_id,
            params.merkle_root.inner(),
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for ClaimV1
fn escrow_claim_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: ClaimEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    // Public inputs for ClaimEscrow ZK proof
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // Circuit constrain_instance calls (3):
    //   constrain_instance(escrow_id)
    //   constrain_instance(escrow_seller_commitment) — H(seller_pub_x, seller_pub_y)
    //   constrain_instance(spent_nullifier)
    let (seller_x, seller_y) = params.recipient_pubkey.xy().expect("pk not identity");
    let escrow_seller_commitment = poseidon_hash([seller_x, seller_y]);

    zk_public_inputs.push((
        ESCROW_CONTRACT_ZKAS_CLAIM_NS_V1.to_string(),
        vec![
            params.escrow_id,
            escrow_seller_commitment,
            params.spent_nullifier,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for RefundV1
fn escrow_refund_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: RefundEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    // Public inputs for RefundEscrow ZK proof
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // Circuit constrain_instance calls (6):
    //   constrain_instance(escrow_id)
    //   constrain_instance(timeout)
    //   constrain_instance(current_block)
    //   constrain_instance(input_buyer_pub_x)
    //   constrain_instance(input_buyer_pub_y)
    //   constrain_instance(spent_nullifier)
    let (buyer_x, buyer_y) = params.recipient_pubkey.xy().expect("pk not identity");

    zk_public_inputs.push((
        ESCROW_CONTRACT_ZKAS_REFUND_NS_V1.to_string(),
        vec![
            params.escrow_id,
            pallas::Base::from(params.timeout),
            pallas::Base::from(params.current_block),
            buyer_x,
            buyer_y,
            params.spent_nullifier,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for CancelV1
fn escrow_cancel_get_metadata_v1(
    _cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CancelEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // Circuit constrain_instance calls (5):
    //   constrain_instance(escrow_id)
    //   constrain_instance(buyer_pub_x)
    //   constrain_instance(buyer_pub_y)
    //   constrain_instance(tx_commitment)
    //   constrain_instance(cancel_nullifier)
    let (buyer_x, buyer_y) = params.buyer_pubkey.xy().expect("pk not identity");

    zk_public_inputs.push((
        ESCROW_CONTRACT_ZKAS_CANCEL_NS_V1.to_string(),
        vec![
            params.escrow_id,
            buyer_x,
            buyer_y,
            pallas::Base::zero(), // tx_commitment — verified by host
            params.cancel_nullifier,
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
    let func = EscrowFunction::try_from(self_.data.data[0])?;

    msg!("[escrow::process_instruction] Processing function: {:?}", func);

    match func {
        EscrowFunction::CreateEscrowV1 => {
            let params: CreateEscrowParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = escrow_create_process_instruction_v1(cid, call_idx, calls, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        EscrowFunction::FundV1 => {
            let params: FundEscrowParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = escrow_fund_process_instruction_v1(cid, call_idx, calls, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        EscrowFunction::ClaimV1 => {
            let params: ClaimEscrowParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = escrow_claim_process_instruction_v1(cid, call_idx, calls, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        EscrowFunction::RefundV1 => {
            let params: RefundEscrowParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = escrow_refund_process_instruction_v1(cid, call_idx, calls, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        EscrowFunction::CancelV1 => {
            let params: CancelEscrowParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = escrow_cancel_process_instruction_v1(cid, call_idx, calls, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        EscrowFunction::InitializeV1 => {
            msg!("[escrow::process_instruction] InitializeV1 has no instruction data");
            let _ = wasm::util::set_return_data(&[]);
        }
    }

    Ok(())
}

/// `process_instruction` for CreateEscrowV1
fn escrow_create_process_instruction_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[CreateEscrowV1] Processing instruction");

    // Access the escrows database
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;

    // Verify the escrow doesn't already exist
    if wasm::db::db_contains_key(escrows_db, &serialize(&params.commitment))? {
        msg!("[CreateEscrowV1] Error: Escrow already exists");
        return Err(EscrowError::EscrowAlreadyExists("commitment exists".to_string()).into())
    }

    // Create the escrow record
    let escrow = Escrow {
        version: 1,
        id: params.commitment,
        buyer_pubkey: params.buyer_pubkey,
        seller_pubkey: params.seller_pubkey,
        value: params.value,
        token_id: params.token_id,
        timeout: params.timeout,
        state: EscrowState::Created,
        value_commit: pallas::Point::identity(), // Set during Fund
        value_blind: pallas::Scalar::ZERO,      // Set during Fund
        spent_nullifier: pallas::Base::ZERO,    // Set during Claim/Refund
        created_at: wasm::util::get_verifying_block_height()?.into(),
        funded_at: None,
        instance_seed: params.instance_seed,
    };

    // Store the escrow directly since CreateEscrowUpdateV1 only has escrow_id
    let key = serialize(&escrow.id);
    let value = serialize(&escrow);
    wasm::db::db_set(escrows_db, &key, &value)?;

    let update = CreateEscrowUpdateV1 { escrow_id: escrow.id };
    Ok(serialize(&update))
}

/// `process_instruction` for FundV1
fn escrow_fund_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: FundEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[FundV1] Processing instruction for escrow {:?}", params.escrow_id);

    // Validate child calls: (1) PN::TransferV1 to move tokens, (2) Purse::DepositV1 to lock funds
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 2 {
        msg!("[FundV1] Error: Expected 2 child calls (PN::transfer_v1 + Purse::deposit_v1), got {}",
             this_call.children_indexes.len());
        return Err(EscrowError::InvalidChildrenIndexes.into())
    }
    // Child 0: PN transfer
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[FundV1] Error: Expected PN::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(EscrowError::InvalidChildCall.into())
    }
    let info_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(EscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    // Child 1: Purse deposit — locks the funded amount in a genesis Purse
    let purse_idx = this_call.children_indexes[1];
    let purse_call = &calls[purse_idx].data;
    if purse_call.data[0] != 0x01 {
        msg!("[FundV1] Error: Expected Purse::deposit_v1 (0x01), got 0x{:02x}", purse_call.data[0]);
        return Err(EscrowError::InvalidChildCall.into())
    }
    validate_child_contract_id(&purse_call.contract_id, &*PURSE_CONTRACT_ID)?;

    // Access the escrows database
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;

    // Fetch the existing escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&params.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", params.escrow_id)))?;
    let mut escrow: Escrow = deserialize(&escrow_data)?;

    // Verify the escrow is in Created state
    if escrow.state != EscrowState::Created {
        msg!("[FundV1] Error: Escrow not in Created state");
        return Err(EscrowError::InvalidStateTransition.into())
    }

    // Update escrow with funding details
    escrow.value_commit = params.value_commit;
    escrow.state = EscrowState::Funded;
    escrow.funded_at = Some(wasm::util::get_verifying_block_height()?.into());

    let update = FundEscrowUpdateV1 { escrow_id: escrow.id };
    Ok(serialize(&update))
}

/// `process_instruction` for ClaimV1
fn escrow_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: ClaimEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[ClaimV1] Processing instruction for escrow {:?}", params.escrow_id);

    // Validate child calls: (1) PN::TransferV1 to release funds, (2) Box::TakeV1 to consume claim capability
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 2 {
        msg!("[ClaimV1] Error: Expected 2 child calls (PN::transfer_v1 + Box::take_v1), got {}",
             this_call.children_indexes.len());
        return Err(EscrowError::InvalidChildrenIndexes.into())
    }
    // Child 0: PN transfer
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[ClaimV1] Error: Expected PN::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(EscrowError::InvalidChildCall.into())
    }
    let info_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(EscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    // Child 1: Box take — consumes the seller's claim Box (nullifier prevents double-claim)
    let box_idx = this_call.children_indexes[1];
    let box_call = &calls[box_idx].data;
    if box_call.data[0] != 0x02 {
        msg!("[ClaimV1] Error: Expected Box::take_v1 (0x02), got 0x{:02x}", box_call.data[0]);
        return Err(EscrowError::InvalidChildCall.into())
    }
    validate_child_contract_id(&box_call.contract_id, &*BOX_CONTRACT_ID)?;

    // Access databases
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_SPENT_FLAGS_TREE)?;

    // Fetch the existing escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&params.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", params.escrow_id)))?;
    let escrow: Escrow = deserialize(&escrow_data)?;

    // CRITICAL: Verify the escrow is in Funded state
    if escrow.state != EscrowState::Funded {
        msg!("[ClaimV1] Error: Escrow not in Funded state - possible double-claim");
        return Err(EscrowError::InvalidStateTransition.into())
    }

    // CRITICAL: Verify the escrow hasn't already been spent
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[ClaimV1] Error: Escrow already spent (nullifier exists)");
        return Err(EscrowError::AlreadySpent.into())
    }

    // Verify the nullifier matches what we expect
    let expected_nullifier = poseidon_hash([escrow.id, params.seller_secret]);
    if expected_nullifier != params.spent_nullifier {
        msg!("[ClaimV1] Error: Nullifier mismatch");
        return Err(EscrowError::InvalidNullifier.into())
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(escrow.value),
        escrow.id,
    ]);
    validate_child_value_commit(&child_call.data, escrow.value, value_blind)?;

    let update = ClaimEscrowUpdateV1 {
        escrow_id: escrow.id,
        spent_nullifier: params.spent_nullifier,
    };
    Ok(serialize(&update))
}

/// `process_instruction` for RefundV1
fn escrow_refund_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: RefundEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[RefundV1] Processing instruction for escrow {:?}", params.escrow_id);

    // Validate child call is promissory_note::transfer_v1 (0x04)
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[RefundV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(EscrowError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[RefundV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(EscrowError::InvalidChildCall.into())
    }
    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(EscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    if promissory_note_cid != ContractId::ZERO {
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    }

    // Access databases
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_SPENT_FLAGS_TREE)?;

    // Fetch the existing escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&params.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", params.escrow_id)))?;
    let escrow: Escrow = deserialize(&escrow_data)?;

    // CRITICAL: Verify the escrow is in Funded state
    if escrow.state != EscrowState::Funded {
        msg!("[RefundV1] Error: Escrow not in Funded state");
        return Err(EscrowError::InvalidStateTransition.into())
    }

    // CRITICAL: Verify the escrow hasn't already been spent
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[RefundV1] Error: Escrow already spent (nullifier exists)");
        return Err(EscrowError::AlreadySpent.into())
    }

    // CRITICAL: Verify timelock has passed
    let current_block = wasm::util::get_verifying_block_height()?;
    if u64::from(current_block) < escrow.timeout {
        msg!(
            "[RefundV1] Error: Timelock not reached (current: {}, timeout: {})",
            current_block,
            escrow.timeout
        );
        return Err(EscrowError::TimelockNotExpired.into())
    }

    // Verify the nullifier matches what we expect
    let expected_nullifier = poseidon_hash([escrow.id, params.buyer_secret]);
    if expected_nullifier != params.spent_nullifier {
        msg!("[RefundV1] Error: Nullifier mismatch");
        return Err(EscrowError::InvalidNullifier.into())
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(escrow.value),
        escrow.id,
    ]);
    validate_child_value_commit(&child_call.data, escrow.value, value_blind)?;

    let update = RefundEscrowUpdateV1 {
        escrow_id: escrow.id,
        spent_nullifier: params.spent_nullifier,
    };
    Ok(serialize(&update))
}

/// `process_instruction` for CancelV1
fn escrow_cancel_process_instruction_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CancelEscrowParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[CancelV1] Processing instruction for escrow {:?}", params.escrow_id);

    // Access the escrows database
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;

    // Fetch the existing escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&params.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", params.escrow_id)))?;
    let escrow: Escrow = deserialize(&escrow_data)?;

    // Verify buyer pubkey matches the escrow's stored buyer pubkey
    // (the ZK proof already proves knowledge of the secret for this pubkey)
    if params.buyer_pubkey != escrow.buyer_pubkey {
        msg!("[CancelV1] Error: Caller pubkey does not match escrow buyer");
        return Err(ContractError::InvalidFunction)
    }

    // Verify cancel nullifier hasn't been used (prevent double-cancel)
    let spent_flags_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.cancel_nullifier))? {
        msg!("[CancelV1] Error: Cancel nullifier already spent");
        return Err(EscrowError::AlreadySpent.into())
    }

    // CRITICAL: Verify the escrow is in Created state (can only cancel before funding)
    if escrow.state != EscrowState::Created {
        msg!("[CancelV1] Error: Can only cancel escrows in Created state");
        return Err(EscrowError::InvalidStateTransition.into())
    }

    let update = CancelEscrowUpdateV1 {
        escrow_id: escrow.id,
        cancel_nullifier: params.cancel_nullifier,
    };
    Ok(serialize(&update))
}

// ============================================================================
// STATE UPDATE (write new state)
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = EscrowFunction::try_from(update_data[0])?;

    match func {
        EscrowFunction::CreateEscrowV1 => {
            let update: CreateEscrowUpdateV1 = deserialize(&update_data[1..])?;
            escrow_create_process_update_v1(cid, update)
        }
        EscrowFunction::FundV1 => {
            let update: FundEscrowUpdateV1 = deserialize(&update_data[1..])?;
            escrow_fund_process_update_v1(cid, update)
        }
        EscrowFunction::ClaimV1 => {
            let update: ClaimEscrowUpdateV1 = deserialize(&update_data[1..])?;
            escrow_claim_process_update_v1(cid, update)
        }
        EscrowFunction::RefundV1 => {
            let update: RefundEscrowUpdateV1 = deserialize(&update_data[1..])?;
            escrow_refund_process_update_v1(cid, update)
        }
        EscrowFunction::CancelV1 => {
            let update: CancelEscrowUpdateV1 = deserialize(&update_data[1..])?;
            escrow_cancel_process_update_v1(cid, update)
        }
        EscrowFunction::InitializeV1 => {
            msg!("[escrow::process_update] InitializeV1 has no update data");
            Ok(())
        }
    }
}

/// `process_update` for CreateEscrowV1
fn escrow_create_process_update_v1(_cid: ContractId, update: CreateEscrowUpdateV1) -> ContractResult {
    // Escrow was already stored in process_instruction
    msg!("[CreateEscrowV1] Escrow {:?} created", update.escrow_id);
    Ok(())
}

/// `process_update` for FundV1
fn escrow_fund_process_update_v1(cid: ContractId, update: FundEscrowUpdateV1) -> ContractResult {
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;

    // Fetch and update the escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&update.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", update.escrow_id)))?;
    let mut escrow: Escrow = deserialize(&escrow_data)?;

    escrow.state = EscrowState::Funded;
    escrow.funded_at = Some(wasm::util::get_verifying_block_height()?.into());

    wasm::db::db_set(escrows_db, &serialize(&escrow.id), &serialize(&escrow))?;
    msg!("[FundV1] Escrow {:?} funded and state updated to Funded", update.escrow_id);
    Ok(())
}

/// `process_update` for ClaimV1
fn escrow_claim_process_update_v1(cid: ContractId, update: ClaimEscrowUpdateV1) -> ContractResult {
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_SPENT_FLAGS_TREE)?;

    // Fetch and update the escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&update.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", update.escrow_id)))?;
    let mut escrow: Escrow = deserialize(&escrow_data)?;

    escrow.state = EscrowState::Claimed;
    escrow.spent_nullifier = update.spent_nullifier;

    wasm::db::db_set(escrows_db, &serialize(&escrow.id), &serialize(&escrow))?;

    // Record the spent nullifier to prevent double-spend
    wasm::db::db_set(spent_flags_db, &serialize(&update.spent_nullifier), &[])?;

    msg!(
        "[ClaimV1] Escrow {:?} claimed, nullifier {:?} recorded",
        update.escrow_id,
        update.spent_nullifier
    );
    Ok(())
}

/// `process_update` for RefundV1
fn escrow_refund_process_update_v1(cid: ContractId, update: RefundEscrowUpdateV1) -> ContractResult {
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_SPENT_FLAGS_TREE)?;

    // Fetch and update the escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&update.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", update.escrow_id)))?;
    let mut escrow: Escrow = deserialize(&escrow_data)?;

    escrow.state = EscrowState::Refunded;
    escrow.spent_nullifier = update.spent_nullifier;

    wasm::db::db_set(escrows_db, &serialize(&escrow.id), &serialize(&escrow))?;

    // Record the spent nullifier to prevent double-spend
    wasm::db::db_set(spent_flags_db, &serialize(&update.spent_nullifier), &[])?;

    msg!(
        "[RefundV1] Escrow {:?} refunded, nullifier {:?} recorded",
        update.escrow_id,
        update.spent_nullifier
    );
    Ok(())
}

/// `process_update` for CancelV1
fn escrow_cancel_process_update_v1(cid: ContractId, update: CancelEscrowUpdateV1) -> ContractResult {
    let escrows_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_ESCROWS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, ESCROW_CONTRACT_NULLIFIERS_TREE)?;

    // Fetch and update the escrow
    let escrow_data = wasm::db::db_get(escrows_db, &serialize(&update.escrow_id))?
        .ok_or_else(|| EscrowError::EscrowNotFound(format!("{:?}", update.escrow_id)))?;
    let mut escrow: Escrow = deserialize(&escrow_data)?;

    escrow.state = EscrowState::Cancelled;

    wasm::db::db_set(escrows_db, &serialize(&escrow.id), &serialize(&escrow))?;

    // Record cancel nullifier to prevent double-cancel
    wasm::db::db_set(spent_flags_db, &serialize(&update.cancel_nullifier), &[])?;
    msg!("[CancelV1] Escrow {:?} cancelled", update.escrow_id);
    Ok(())
}
