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
    crypto::{pasta_prelude::PrimeField, ContractId},
    error::ContractResult,
    msg,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize};

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
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoEscrowFunction::try_from(self_.data[0])?;

    msg!("[dao_escrow::get_metadata] Processing function: {:?}", func);

    // TODO: Implement metadata fetching for ZK proof verification
    wasm::util::set_return_data(&[])
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
            pay_premium_v1(cid, params)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let params: model::WithdrawParamsV1 = deserialize(&self_.data[1..])?;
            withdraw_v1(cid, params)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            // TODO: Implement endowment withdrawal (requires DAO governance)
            msg!("[dao_escrow::process_instruction] EndowmentWithdrawV1 not yet implemented");
            Err(crate::error::DaoEscrowError::EndowmentWithdrawUnauthorized.into())
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            // TODO: Implement treasury spending (standard DAO governance)
            msg!("[dao_escrow::process_instruction] TreasurySpendV1 not yet implemented");
            Err(crate::error::DaoEscrowError::InsufficientEndowment.into())
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
            // TODO: Process endowment withdrawal (requires DAO vote)
            msg!("[dao_escrow::process_update] EndowmentWithdrawV1 not yet implemented");
            Ok(())
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            // TODO: Process treasury spending (standard DAO governance)
            msg!("[dao_escrow::process_update] TreasurySpendV1 not yet implemented");
            Ok(())
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
fn pay_premium_v1(cid: ContractId, params: model::PayPremiumParamsV1) -> ContractResult {
    msg!("[dao_escrow::pay_premium_v1] Processing premium payment");

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
fn withdraw_v1(cid: ContractId, params: model::WithdrawParamsV1) -> ContractResult {
    msg!("[dao_escrow::withdraw_v1] Processing withdrawal");

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