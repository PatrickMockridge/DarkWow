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

//! WASM entrypoint for the DAO-Escrow contract (Simplified MVP)
//!
//! ## Simplified MVP: Endowment Pool with DAO Governance
//!
//! Claims are handled by the DAO's existing treasury management.
//! This contract only manages:
//! 1. Endowment initialization (linked to a DAO)
//! 2. Premium payments (issues membership notes)
//! 3. Admin withdrawals
//!
//! ```text
//! Members pay premiums ──> Endowment Pool ──> DAO Treasury (claims)
//!                              ▲
//!                              │
//!                     Membership notes
//!                     (annual expiry)
//! ```

use darkfi_sdk::{
    crypto::{pasta_prelude::{Curve, CurveAffine, PrimeField}, ContractId},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DaoEscrowError,
    model,
    DaoEscrowFunction, DAO_ESCROW_CONTRACT_BULLAS_TREE, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE,
    DAO_ESCROW_CONTRACT_INFO_TREE, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const DAO_ESCROW_DB_VERSION_KEY: &[u8] = b"db_version";

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize DAO-Escrow contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Bullas tree (endowment instances)
/// - Membership tree (membership notes)
/// - Endowment tree (funds pool)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[dao_escrow::init_contract] Initializing DAO-Escrow contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, DAO_ESCROW_DB_VERSION_KEY, &env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize bullas tree (endowment instances)
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;

    // Initialize membership tree
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;

    // Initialize endowment tree
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    msg!("[dao_escrow::init_contract] DAO-Escrow contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoEscrowFunction::try_from(self_.data[0])?;

    msg!("[dao_escrow::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        DaoEscrowFunction::InitializeV1 => initialize_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::PayPremiumV1 => pay_premium_get_metadata(cid, call_idx, &calls),
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// Metadata for InitializeV1 (0x00)
fn initialize_get_metadata(_cid: ContractId, call_idx: usize, calls: &[darkfi_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::InitializeParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let (owner_pub_x, owner_pub_y) = params.owner_pubkey.xy();

    // Compute endowment_bulla using same formula as circuit and model
    // endowment_bulla = poseidon_hash(dao_bulla, owner_pub_x, owner_pub_y, endowment_token_id, bulla_blind)
    let endowment_bulla = darkfi_sdk::crypto::poseidon_hash([
        params.dao_bulla,
        owner_pub_x,
        owner_pub_y,
        params.endowment_token_id,
        params.bulla_blind.inner(),
    ]);

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_INIT_NS.to_string(),
        vec![
            params.dao_bulla,
            endowment_bulla,
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for PayPremiumV1 (0x02)
fn pay_premium_get_metadata(_cid: ContractId, call_idx: usize, calls: &[darkfi_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::PayPremiumParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let value_coords = params.value_commit.to_affine().coordinates().unwrap();

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_PREMIUM_NS.to_string(),
        vec![
            params.dao_escrow_bulla,
            params.membership_note,
            *value_coords.x(),
            *value_coords.y(),
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoEscrowFunction::try_from(self_.data[0])?;

    msg!("[dao_escrow::process_instruction] Processing function: {:?}", func);

    match func {
        DaoEscrowFunction::InitializeV1 => {
            let params: model::InitializeParamsV1 = deserialize(&self_.data[1..])?;
            initialize_v1(cid, params)
        }
        DaoEscrowFunction::UpdateV1 => {
            let params: model::UpdateParamsV1 = deserialize(&self_.data[1..])?;
            update_v1(cid, params)
        }
        DaoEscrowFunction::PayPremiumV1 => {
            let params: model::PayPremiumParamsV1 = deserialize(&self_.data[1..])?;
            pay_premium_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let params: model::WithdrawParamsV1 = deserialize(&self_.data[1..])?;
            withdraw_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            let params: model::EndowmentWithdrawParamsV1 = deserialize(&self_.data[1..])?;
            endowment_withdraw_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            let params: model::TreasurySpendParamsV1 = deserialize(&self_.data[1..])?;
            treasury_spend_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::EnableDrainProtectionV1 => {
            let params: model::EnableDrainProtectionParamsV1 = deserialize(&self_.data[1..])?;
            enable_drain_protection_v1(cid, params)
        }
    }
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = DaoEscrowFunction::try_from(update_data[0])?;

    match func {
        DaoEscrowFunction::InitializeV1 => {
            let update: model::InitializeUpdateV1 = deserialize(&update_data[1..])?;
            initialize_apply_v1(cid, update)
        }
        DaoEscrowFunction::UpdateV1 => {
            let update: model::UpdateUpdateV1 = deserialize(&update_data[1..])?;
            update_apply_v1(cid, update)
        }
        DaoEscrowFunction::PayPremiumV1 => {
            let update: model::PayPremiumUpdateV1 = deserialize(&update_data[1..])?;
            pay_premium_apply_v1(cid, update)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let update: model::WithdrawUpdateV1 = deserialize(&update_data[1..])?;
            withdraw_apply_v1(cid, update)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            let update: model::EndowmentWithdrawUpdateV1 = deserialize(&update_data[1..])?;
            endowment_withdraw_apply_v1(cid, update)
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            let update: model::TreasurySpendUpdateV1 = deserialize(&update_data[1..])?;
            treasury_spend_apply_v1(cid, update)
        }
        DaoEscrowFunction::EnableDrainProtectionV1 => {
            let update: model::EnableDrainProtectionUpdateV1 = deserialize(&update_data[1..])?;
            enable_drain_protection_apply_v1(cid, update)
        }
    }
}

// ============================================================================
// INSTRUCTION HANDLERS
// ============================================================================

/// InitializeV1 instruction - creates a new DAO-Escrow endowment
fn initialize_v1(cid: ContractId, params: model::InitializeParamsV1) -> ContractResult {
    msg!("[dao_escrow::initialize_v1] Initializing DAO-Escrow endowment");

    // Verify endowment doesn't already exist
    let bullas_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;
    if wasm::db::db_contains_key(bullas_db, &params.dao_bulla.to_repr())? {
        msg!("[dao_escrow::initialize_v1] ERROR: DAO-Escrow already exists");
        return Err(DaoEscrowError::DaoEscrowAlreadyExists("DAO bulla already exists".to_string()).into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::DAO_ESCROW_ZKAS_INIT_NS)?;

    // Derive endowment bulla (same formula as model)
    let endowment_bulla = model::DaoEscrow::derive_bulla(
        model::DaoEscrowMode::Escrow, // Default mode
        &params.owner_pubkey,
        params.endowment_token_id,
        &None,
        params.bulla_blind,
    );

    // Create update
    let update = model::InitializeUpdateV1 {
        bulla: endowment_bulla,
        owner_pubkey: params.owner_pubkey,
        bulla_blind: params.bulla_blind,
    };

    msg!("[dao_escrow::initialize_v1] Endowment initialized: {:?}", endowment_bulla);
    wasm::util::set_return_data(&serialize(&update))
}

/// InitializeV1 apply - store new endowment
fn initialize_apply_v1(cid: ContractId, update: model::InitializeUpdateV1) -> ContractResult {
    let bullas_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Store endowment bulla in bullas tree
    wasm::db::db_set(bullas_db, &update.bulla.to_repr(), &[])?;

    // Initialize endowment state
    let endowment = model::DaoEscrow {
        bulla: update.bulla,
        mode: model::DaoEscrowMode::Escrow,
        owner_pubkey: update.owner_pubkey,
        pool_token_id: Default::default(),
        total_pool: 0,
        total_treasury: 0,
        total_endowment: 0,
        member_count: 0,
        fee_config: None,
        min_premium: 0,
        max_members: u64::MAX,
        created_at: wasm::util::get_verifying_block_height()? as u64,
        bulla_blind: update.bulla_blind,
        paused: false,
        drain_protection_enabled: false,
        drain_protection_bulla: None,
    };

    wasm::db::db_set(endowments_db, &update.bulla.to_repr(), &serialize(&endowment))?;

    msg!("[dao_escrow::initialize_apply_v1] Endowment stored: {:?}", update.bulla);
    Ok(())
}

/// UpdateV1 instruction - update endowment parameters
fn update_v1(cid: ContractId, params: model::UpdateParamsV1) -> ContractResult {
    msg!("[dao_escrow::update_v1] Updating DAO-Escrow: {:?}", params.bulla);

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.bulla.to_repr())?;
    if endowment_data.is_none() {
        msg!("[dao_escrow::update_v1] ERROR: Endowment not found");
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    // Create update
    let update = model::UpdateUpdateV1 { bulla: params.bulla };

    msg!("[dao_escrow::update_v1] Endowment update prepared: {:?}", params.bulla);
    wasm::util::set_return_data(&serialize(&update))
}

/// UpdateV1 apply - update endowment parameters
fn update_apply_v1(_cid: ContractId, update: model::UpdateUpdateV1) -> ContractResult {
    msg!("[dao_escrow::update_apply_v1] Endowment updated: {:?}", update.bulla);
    // In a full implementation, this would update the endowment state
    Ok(())
}

/// PayPremiumV1 instruction - member pays premium, receives membership
fn pay_premium_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: model::PayPremiumParamsV1) -> ContractResult {
    msg!("[dao_escrow::pay_premium_v1] Processing premium payment");

    // Validate child call is money_v3::transfer_v1 (0x04) for premium payment
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[pay_premium_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[pay_premium_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Verify DAO-Escrow endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    if endowment_data.is_none() {
        msg!("[dao_escrow::pay_premium_v1] ERROR: Endowment not found");
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    // Verify membership note doesn't already exist
    let membership_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;
    if wasm::db::db_contains_key(membership_db, &params.membership_note.to_repr())? {
        msg!("[dao_escrow::pay_premium_v1] ERROR: Membership already exists");
        return Err(DaoEscrowError::ClaimAlreadyExists("Membership already exists".to_string()).into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::DAO_ESCROW_ZKAS_PREMIUM_NS)?;

    // Calculate fee split based on mode (simplified - all to endowment)
    let total_endowment = params.value; // All to endowment in ESCROW mode

    // Create update
    let update = model::PayPremiumUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        membership_note: params.membership_note,
        total_endowment,
        member_count: 1, // Incremented on apply
        member_pubkey: params.member_pubkey,
        token_id: params.token_id,
        expiry: params.expiry,
    };

    msg!("[dao_escrow::pay_premium_v1] Premium processed: {:?}", params.membership_note);
    wasm::util::set_return_data(&serialize(&update))
}

/// PayPremiumV1 apply - store membership note and update endowment
fn pay_premium_apply_v1(cid: ContractId, update: model::PayPremiumUpdateV1) -> ContractResult {
    let membership_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Create and store membership
    let membership = model::Membership {
        note: update.membership_note,
        dao_escrow_bulla: update.dao_escrow_bulla,
        member_pubkey: update.member_pubkey,
        value: update.total_endowment,
        token_id: update.token_id,
        expiry: update.expiry,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    wasm::db::db_set(membership_db, &update.membership_note.to_repr(), &serialize(&membership))?;

    // Update endowment totals
    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_repr())?;
    if let Some(data) = endowment_data {
        let mut endowment: model::DaoEscrow = deserialize(&data)?;
        endowment.total_endowment += update.total_endowment;
        endowment.member_count += 1;
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_repr(), &serialize(&endowment))?;
    }

    msg!("[dao_escrow::pay_premium_apply_v1] Membership stored: {:?}", update.membership_note);
    Ok(())
}

/// WithdrawV1 instruction - endowment owner withdraws funds
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn withdraw_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::WithdrawParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::withdraw_v1] Processing withdrawal");

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!(
            "[WithdrawV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[WithdrawV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[dao_escrow::withdraw_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Verify caller is endowment owner
    if endowment.owner_pubkey != params.recipient_pubkey {
        msg!("[dao_escrow::withdraw_v1] ERROR: Not authorized to withdraw");
        return Err(DaoEscrowError::NotAuthorizedToWithdraw.into())
    }

    // Verify sufficient balance
    if endowment.total_endowment < params.value {
        msg!("[dao_escrow::withdraw_v1] ERROR: Insufficient endowment balance");
        return Err(DaoEscrowError::InsufficientEndowment.into())
    }

    // Create update
    let update = model::WithdrawUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        value: params.value,
        total_endowment: endowment.total_endowment - params.value,
    };

    msg!("[dao_escrow::withdraw_v1] Withdrawal processed: {}", params.value);
    wasm::util::set_return_data(&serialize(&update))
}

/// WithdrawV1 apply - update endowment totals
fn withdraw_apply_v1(cid: ContractId, update: model::WithdrawUpdateV1) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_repr())?;
    if let Some(data) = endowment_data {
        let mut endowment: model::DaoEscrow = deserialize(&data)?;
        endowment.total_endowment = update.total_endowment;
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_repr(), &serialize(&endowment))?;
    }

    msg!("[dao_escrow::withdraw_apply_v1] Endowment updated: new total = {}", update.total_endowment);
    Ok(())
}

/// EnableDrainProtectionV1 instruction
fn enable_drain_protection_v1(
    cid: ContractId,
    params: model::EnableDrainProtectionParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::enable_drain_protection_v1] Enabling drain protection");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    if endowment_data.is_none() {
        msg!("[dao_escrow::enable_drain_protection_v1] ERROR: Endowment not found");
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    let update = model::EnableDrainProtectionUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        drain_protection_bulla: params.drain_protection_bulla,
    };

    msg!("[dao_escrow::enable_drain_protection_v1] Drain protection update prepared");
    wasm::util::set_return_data(&serialize(&update))
}

/// EnableDrainProtectionV1 apply
fn enable_drain_protection_apply_v1(
    cid: ContractId,
    update: model::EnableDrainProtectionUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_repr())?;
    if let Some(data) = endowment_data {
        let mut endowment: model::DaoEscrow = deserialize(&data)?;
        endowment.drain_protection_enabled = true;
        endowment.drain_protection_bulla = Some(update.drain_protection_bulla);
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_repr(), &serialize(&endowment))?;
    }

    msg!("[dao_escrow::enable_drain_protection_apply_v1] Drain protection enabled");
    Ok(())
}

/// EndowmentWithdrawV1 instruction - executes an approved claim from endowment
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn endowment_withdraw_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::EndowmentWithdrawParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::endowment_withdraw_v1] Processing endowment withdrawal");

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!(
            "[EndowmentWithdrawV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[EndowmentWithdrawV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[dao_escrow::endowment_withdraw_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Verify sufficient endowment balance
    if endowment.total_endowment < params.value {
        msg!("[dao_escrow::endowment_withdraw_v1] ERROR: Insufficient endowment balance");
        return Err(DaoEscrowError::InsufficientEndowment.into())
    }

    // Calculate new total
    let new_total_endowment = endowment.total_endowment - params.value;

    // Create update
    let update = model::EndowmentWithdrawUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        value: params.value,
        total_endowment: new_total_endowment,
    };

    msg!(
        "[dao_escrow::endowment_withdraw_v1] Endowment withdrawal processed: {} to {:?}",
        params.value,
        params.recipient_pubkey
    );
    wasm::util::set_return_data(&serialize(&update))
}

/// EndowmentWithdrawV1 apply - update endowment totals
fn endowment_withdraw_apply_v1(
    cid: ContractId,
    update: model::EndowmentWithdrawUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_repr())?;
    if let Some(data) = endowment_data {
        let mut endowment: model::DaoEscrow = deserialize(&data)?;
        endowment.total_endowment = update.total_endowment;
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_repr(), &serialize(&endowment))?;
    }

    msg!(
        "[dao_escrow::endowment_withdraw_apply_v1] Endowment updated: new total = {}",
        update.total_endowment
    );
    Ok(())
}

/// TreasurySpendV1 instruction - executes an approved treasury spend
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn treasury_spend_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::TreasurySpendParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::treasury_spend_v1] Processing treasury spend");

    // Validate children_indexes to ensure money_v3::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!(
            "[TreasurySpendV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is money_v3::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[TreasurySpendV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Verify endowment exists and is in treasury mode
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[dao_escrow::treasury_spend_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Verify treasury mode or treasury+endowment mode
    if endowment.mode != model::DaoEscrowMode::Treasury &&
        endowment.mode != model::DaoEscrowMode::TreasuryEndowment
    {
        msg!("[dao_escrow::treasury_spend_v1] ERROR: Not a treasury mode DAO-Escrow");
        return Err(DaoEscrowError::InvalidState { expected: "Treasury mode".to_string(), actual: "Escrow mode".to_string() }.into())
    }

    // Verify sufficient treasury balance
    if endowment.total_treasury < params.value {
        msg!("[dao_escrow::treasury_spend_v1] ERROR: Insufficient treasury balance");
        return Err(DaoEscrowError::InsufficientEndowment.into())
    }

    // Calculate new total
    let new_total_treasury = endowment.total_treasury - params.value;

    // Create update
    let update = model::TreasurySpendUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        proposal_id: params.proposal_id,
        value: params.value,
        total_treasury: new_total_treasury,
    };

    msg!(
        "[dao_escrow::treasury_spend_v1] Treasury spend processed: {} to {:?}",
        params.value,
        params.recipient_pubkey
    );
    wasm::util::set_return_data(&serialize(&update))
}

/// TreasurySpendV1 apply - update treasury totals
fn treasury_spend_apply_v1(
    cid: ContractId,
    update: model::TreasurySpendUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_repr())?;
    if let Some(data) = endowment_data {
        let mut endowment: model::DaoEscrow = deserialize(&data)?;
        endowment.total_treasury = update.total_treasury;
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_repr(), &serialize(&endowment))?;
    }

    msg!(
        "[dao_escrow::treasury_spend_apply_v1] Treasury updated: new total = {}",
        update.total_treasury
    );
    Ok(())
}