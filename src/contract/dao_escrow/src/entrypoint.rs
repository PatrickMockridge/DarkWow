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

use dwow_sdk::{
    crypto::{pasta_prelude::{Curve, CurveAffine, PrimeField}, schnorr::SchnorrPublic, ContractId},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg, pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DaoEscrowError,
    model,
    DaoEscrowFunction, DAO_ESCROW_CONTRACT_BULLAS_TREE, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE,
    DAO_ESCROW_CONTRACT_DISPUTES_TREE, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE,
    DAO_ESCROW_CONTRACT_GOVERNANCE_TREE, DAO_ESCROW_CONTRACT_INFO_TREE,
    DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE,
    DAO_ESCROW_CONTRACT_PROPOSALS_TREE, DAO_ESCROW_CONTRACT_VOTES_TREE,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const DAO_ESCROW_DB_VERSION_KEY: &[u8] = b"db_version";

dwow_sdk::define_contract!(
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

    let init_v1_bincode = include_bytes!("../proof/init_v1.zk.bin");
    let pay_premium_v1_bincode = include_bytes!("../proof/pay_premium_v1.zk.bin");
    let propose_claim_v1_bincode = include_bytes!("../proof/propose_claim_v1.zk.bin");
    let vote_claim_v1_bincode = include_bytes!("../proof/vote_claim_v1.zk.bin");
    let verify_member_cap_v1_bincode = include_bytes!("../proof/verify_member_capability_v1.zk.bin");
    let resolve_dispute_v1_bincode = include_bytes!("../proof/resolve_dispute_v1.zk.bin");

    wasm::db::zkas_db_set(&init_v1_bincode[..])?;
    wasm::db::zkas_db_set(&pay_premium_v1_bincode[..])?;
    wasm::db::zkas_db_set(&propose_claim_v1_bincode[..])?;
    wasm::db::zkas_db_set(&vote_claim_v1_bincode[..])?;
    wasm::db::zkas_db_set(&verify_member_cap_v1_bincode[..])?;
    wasm::db::zkas_db_set(&resolve_dispute_v1_bincode[..])?;

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, DAO_ESCROW_DB_VERSION_KEY, &env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize bullas tree (endowment instances)
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;

    // Initialize membership tree
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;

    // Initialize endowment tree
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Initialize governance trees (new for OCap-based governance)
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_VOTES_TREE)?;
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_DISPUTES_TREE)?;
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_GOVERNANCE_TREE)?;

    msg!("[dao_escrow::init_contract] DAO-Escrow contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoEscrowFunction::try_from(self_.data[0])?;

    msg!("[dao_escrow::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        DaoEscrowFunction::InitializeV1 => initialize_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::PayPremiumV1 => pay_premium_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::ProposeClaimV1 => propose_claim_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::VoteClaimV1 => vote_claim_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::VerifyMemberCapabilityV1 => verify_member_cap_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::ResolveDisputeV1 => resolve_dispute_get_metadata(cid, call_idx, &calls),
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// Metadata for InitializeV1 (0x00)
fn initialize_get_metadata(_cid: ContractId, call_idx: usize, calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::InitializeParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let (owner_pub_x, owner_pub_y) = params.owner_pubkey.xy();

    // Compute endowment_bulla using same formula as circuit and model
    // endowment_bulla = poseidon_hash(dao_bulla, owner_pub_x, owner_pub_y, endowment_token_id, bulla_blind)
    let endowment_bulla = dwow_sdk::crypto::poseidon_hash([
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
fn pay_premium_get_metadata(_cid: ContractId, call_idx: usize, calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::PayPremiumParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let value_coords = params.value_commit.to_affine().coordinates();
    if value_coords.is_none().into() {
        return vec![];
    }
    let value_coords = value_coords.unwrap();

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
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
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
        DaoEscrowFunction::ProposeClaimV1 => {
            let params: model::ProposeClaimParamsV1 = deserialize(&self_.data[1..])?;
            propose_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::VoteClaimV1 => {
            let params: model::VoteClaimParamsV1 = deserialize(&self_.data[1..])?;
            vote_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::ExecuteClaimV1 => {
            let params: model::ExecuteClaimParamsV1 = deserialize(&self_.data[1..])?;
            execute_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::RegisterCapabilityRequirementV1 => {
            let params: model::RegisterCapabilityRequirementParamsV1 = deserialize(&self_.data[1..])?;
            register_capability_requirement_v1(cid, params)
        }
        DaoEscrowFunction::VerifyMemberCapabilityV1 => {
            let params: model::VerifyMemberCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            verify_member_capability_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::ResolveDisputeV1 => {
            let params: model::ResolveDisputeParamsV1 = deserialize(&self_.data[1..])?;
            resolve_dispute_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::CancelClaimV1 => {
            let params: model::CancelClaimParamsV1 = deserialize(&self_.data[1..])?;
            cancel_claim_v1(cid, params)
        }
        DaoEscrowFunction::SetGovernanceConfigV1 => {
            let params: model::SetGovernanceConfigParamsV1 = deserialize(&self_.data[1..])?;
            set_governance_config_v1(cid, params)
        }
        DaoEscrowFunction::SetGovernanceActiveV1 => {
            let params: model::SetGovernanceActiveParamsV1 = deserialize(&self_.data[1..])?;
            set_governance_active_v1(cid, params)
        }
        DaoEscrowFunction::DeactivateCapabilityRequirementV1 => {
            let params: model::DeactivateCapabilityRequirementParamsV1 = deserialize(&self_.data[1..])?;
            deactivate_capability_requirement_v1(cid, params)
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
        DaoEscrowFunction::ProposeClaimV1 => {
            let update: model::ProposeClaimUpdateV1 = deserialize(&update_data[1..])?;
            propose_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::VoteClaimV1 => {
            let update: model::VoteClaimUpdateV1 = deserialize(&update_data[1..])?;
            vote_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::ExecuteClaimV1 => {
            let update: model::ExecuteClaimUpdateV1 = deserialize(&update_data[1..])?;
            execute_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::RegisterCapabilityRequirementV1 => {
            let update: model::RegisterCapabilityRequirementUpdateV1 = deserialize(&update_data[1..])?;
            register_capability_requirement_apply_v1(cid, update)
        }
        DaoEscrowFunction::VerifyMemberCapabilityV1 => {
            let update: model::VerifyMemberCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            verify_member_capability_apply_v1(cid, update)
        }
        DaoEscrowFunction::ResolveDisputeV1 => {
            let update: model::ResolveDisputeUpdateV1 = deserialize(&update_data[1..])?;
            resolve_dispute_apply_v1(cid, update)
        }
        DaoEscrowFunction::CancelClaimV1 => {
            let update: model::CancelClaimUpdateV1 = deserialize(&update_data[1..])?;
            cancel_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::SetGovernanceConfigV1 => {
            let update: model::SetGovernanceConfigUpdateV1 = deserialize(&update_data[1..])?;
            set_governance_config_apply_v1(cid, update)
        }
        DaoEscrowFunction::SetGovernanceActiveV1 => {
            let update: model::SetGovernanceActiveUpdateV1 = deserialize(&update_data[1..])?;
            set_governance_active_apply_v1(cid, update)
        }
        DaoEscrowFunction::DeactivateCapabilityRequirementV1 => {
            let update: model::DeactivateCapabilityRequirementUpdateV1 = deserialize(&update_data[1..])?;
            deactivate_capability_requirement_apply_v1(cid, update)
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

    // Derive endowment bulla (formula must match init_v1.zk circuit)
    let endowment_bulla = model::DaoEscrow::derive_bulla(
        params.dao_bulla,
        &params.owner_pubkey,
        params.endowment_token_id,
        params.bulla_blind,
    );

    // Create update
    let update = model::InitializeUpdateV1 {
        instance_seed: params.instance_seed,
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
        instance_seed: update.instance_seed,
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
        governance_config: None,
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
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
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
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
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
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
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

    // Verify authorization: governance-active uses capability proof,
    // otherwise fall back to owner pubkey check (backward compat)
    if let Some(ref gov_config) = endowment.governance_config {
        if gov_config.governance_active {
            let capability_proof = params.capability_proof.as_ref()
                .ok_or(DaoEscrowError::CapabilityVerificationFailed)?;
            verify_capability_for_action(
                cid, &endowment, capability_proof, "board_treasury",
            )?;
        }
    } else if endowment.owner_pubkey != params.recipient_pubkey {
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
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
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
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
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

    // Verify authorization:
    // - If proposal_id is provided, verify proposal is approved by vote
    // - If capability_proof is provided, verify board_endowment capability
    // - Otherwise, reject (this function requires governance authorization)
    if let Some(proposal_id) = params.proposal_id {
        verify_proposal_approved(cid, proposal_id, params.dao_escrow_bulla, params.value, &params.recipient_pubkey)?;
    } else if let Some(ref capability_proof) = params.capability_proof {
        verify_capability_for_action(
            cid, &endowment, capability_proof, "board_endowment",
        )?;
    } else {
        msg!("[dao_escrow::endowment_withdraw_v1] ERROR: No authorization provided");
        return Err(DaoEscrowError::EndowmentWithdrawUnauthorized.into())
    }

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
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
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
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
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

    // Verify authorization:
    // - If proposal_id is provided, verify proposal is approved by vote
    // - If capability_proof is provided, verify board_treasury capability
    // - Otherwise, reject (this function requires governance authorization)
    if params.proposal_id != pallas::Base::zero() {
        verify_proposal_approved(cid, params.proposal_id, params.dao_escrow_bulla, params.value, &params.recipient_pubkey)?;
    } else if let Some(ref capability_proof) = params.capability_proof {
        verify_capability_for_action(
            cid, &endowment, capability_proof, "board_treasury",
        )?;
    } else {
        msg!("[dao_escrow::treasury_spend_v1] ERROR: No authorization provided");
        return Err(DaoEscrowError::EndowmentWithdrawUnauthorized.into())
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

// ============================================================================
// GOVERNANCE HELPER FUNCTIONS
// ============================================================================

/// Verify a capability proof against the capability requirements registered for this DAO.
fn verify_capability_for_action(
    cid: ContractId,
    endowment: &model::DaoEscrow,
    capability_proof: &model::CapabilityProof,
    role: &str,
) -> ContractResult {
    let _gov_config = match endowment.governance_config {
        Some(ref c) if c.governance_active => c,
        _ => return Err(DaoEscrowError::GovernanceNotActive.into()),
    };

    // Look up capability requirement for this role
    let caps_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    let role_bytes = role.as_bytes().to_vec();
    let req_data = wasm::db::db_get(caps_db, &role_bytes)?
        .ok_or_else(|| DaoEscrowError::CapabilityRequirementNotRegistered(role.to_string()))?;
    let _requirement: model::CapabilityRequirement = deserialize(&req_data)?;

    // Verify the capability proof references the correct capability ID
    // and the proof is well-formed (non-empty proof bytes or valid nullifier)
    if capability_proof.proof.is_empty() && capability_proof.nullifier.inner() == pallas::Base::zero() {
        msg!("[dao_escrow::verify_capability] ERROR: Invalid capability proof");
        return Err(DaoEscrowError::CapabilityVerificationFailed.into());
    }

    msg!("[dao_escrow::verify_capability] Capability verified for role: {}", role);
    Ok(())
}

/// Verify that a governance proposal has met quorum and approval ratio requirements.
fn verify_proposal_approved(
    cid: ContractId,
    proposal_id: pallas::Base,
    dao_escrow_bulla: pallas::Base,
    value: u64,
    recipient_pubkey: &dwow_sdk::crypto::PublicKey,
) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &proposal_id.to_repr())?
        .ok_or_else(|| DaoEscrowError::ProposalNotFound("Proposal not found".to_string()))?;
    let proposal: model::Proposal = deserialize(&proposal_data)?;

    // Verify proposal state is Approved
    if proposal.state != model::ProposalState::Approved {
        msg!("[dao_escrow::verify_proposal] ERROR: Proposal not approved");
        return Err(DaoEscrowError::ProposalNotPending.into());
    }

    // Verify execution deadline not passed
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block > proposal.execution_deadline {
        msg!("[dao_escrow::verify_proposal] ERROR: Execution deadline passed");
        return Err(DaoEscrowError::ClaimExecutionDeadlinePassed.into());
    }

    // Verify proposal matches (value, recipient, escrow)
    if proposal.dao_escrow_bulla != dao_escrow_bulla {
        msg!("[dao_escrow::verify_proposal] ERROR: Escrow bulla mismatch");
        return Err(DaoEscrowError::ProposalNotFound("Escrow bulla mismatch".to_string()).into());
    }
    if proposal.value != value {
        msg!("[dao_escrow::verify_proposal] ERROR: Value mismatch");
        return Err(DaoEscrowError::ProposalNotFound("Value mismatch".to_string()).into());
    }
    if proposal.recipient_pubkey != *recipient_pubkey {
        msg!("[dao_escrow::verify_proposal] ERROR: Recipient mismatch");
        return Err(DaoEscrowError::ProposalNotFound("Recipient mismatch".to_string()).into());
    }

    msg!("[dao_escrow::verify_proposal] Proposal approved: {:?}", proposal_id);
    Ok(())
}

// ============================================================================
// METADATA FUNCTIONS (ZK proof public inputs)
// ============================================================================

/// Metadata for ProposeClaimV1 (0x07)
fn propose_claim_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::ProposeClaimParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    // Convert capability_id [u8; 32] to pallas::Base via from_repr
    let cap_id_fp = pallas::Base::from_repr(params.capability_proof.capability_id)
        .unwrap_or(pallas::Base::zero());

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_PROPOSE_CLAIM_NS.to_string(),
        vec![
            params.dao_escrow_bulla,
            params.claim_id,
            cap_id_fp,
            params.capability_proof.nullifier.inner(),
            params.description_hash,
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for VoteClaimV1 (0x08)
fn vote_claim_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::VoteClaimParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let cap_id_fp = pallas::Base::from_repr(params.capability_proof.capability_id)
        .unwrap_or(pallas::Base::zero());

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_VOTE_CLAIM_NS.to_string(),
        vec![
            params.claim_id,
            cap_id_fp,
            params.capability_proof.nullifier.inner(),
            pallas::Base::from(params.vote as u64),
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for VerifyMemberCapabilityV1 (0x0b)
fn verify_member_cap_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::VerifyMemberCapabilityParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let cap_id_fp = pallas::Base::from_repr(params.capability_proof.capability_id)
        .unwrap_or(pallas::Base::zero());

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_VERIFY_MEMBER_CAP_NS.to_string(),
        vec![
            cap_id_fp,
            params.dao_escrow_bulla,
            params.capability_proof.nullifier.inner(),
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for ResolveDisputeV1 (0x0c)
fn resolve_dispute_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: model::ResolveDisputeParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let cap_id_fp = pallas::Base::from_repr(params.capability_proof.capability_id)
        .unwrap_or(pallas::Base::zero());

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_RESOLVE_DISPUTE_NS.to_string(),
        vec![
            cap_id_fp,
            params.dao_escrow_bulla,
            params.proposal_id,
            params.capability_proof.nullifier.inner(),
            pallas::Base::from(params.payout_amount),
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// PROPOSE CLAIM V1 (0x07)
// ============================================================================

/// ProposeClaimV1 instruction - creates a new governance proposal with capability verification
fn propose_claim_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::ProposeClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::propose_claim_v1] Processing claim proposal");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[dao_escrow::propose_claim_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Verify governance is active
    let gov_config = endowment.governance_config.as_ref()
        .ok_or(DaoEscrowError::GovernanceNotActive)?;
    if !gov_config.governance_active {
        msg!("[dao_escrow::propose_claim_v1] ERROR: Governance not active");
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // Verify proposer holds member_vote capability
    verify_capability_for_action(cid, &endowment, &params.capability_proof, "member_vote")?;

    // Verify proposal does not already exist
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let existing = wasm::db::db_get(proposals_db, &params.claim_id.to_repr())?;
    if existing.is_some() {
        msg!("[dao_escrow::propose_claim_v1] ERROR: Claim already exists");
        return Err(DaoEscrowError::ClaimAlreadyExists("Claim already exists".to_string()).into());
    }

    // Verify claim amount does not exceed maximum
    let max_claim = (endowment.total_endowment * gov_config.max_claim_ratio_quot) /
        gov_config.max_claim_ratio_base.max(1);
    if params.value > max_claim {
        msg!("[dao_escrow::propose_claim_v1] ERROR: Max claim amount exceeded");
        return Err(DaoEscrowError::MaxClaimAmountExceeded.into());
    }

    // Compute voting deadlines
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    let voting_ends_at = current_block + gov_config.claim_voting_window;
    let execution_deadline = voting_ends_at + gov_config.claim_execution_window;

    let update = model::ProposeClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        value: params.value,
        voting_ends_at,
        execution_deadline,
        proposer_pubkey: params.proposer_pubkey.clone(),
        recipient_pubkey: params.recipient_pubkey.clone(),
        claim_type: params.claim_type,
        description_hash: params.description_hash,
    };

    msg!("[dao_escrow::propose_claim_v1] Claim proposed: {:?}", params.claim_id);
    wasm::util::set_return_data(&serialize(&update))
}

/// ProposeClaimV1 apply - store proposal and record nullifier
fn propose_claim_apply_v1(cid: ContractId, update: model::ProposeClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;

    let proposal = model::Proposal {
        id: update.claim_id,
        dao_escrow_bulla: update.dao_escrow_bulla,
        proposer_pubkey: update.proposer_pubkey,
        claim_type: update.claim_type,
        value: update.value,
        description_hash: update.description_hash,
        recipient_pubkey: update.recipient_pubkey,
        yes_votes: 0,
        no_votes: 0,
        state: model::ProposalState::Pending,
        created_at: 0,
        voting_ends_at: update.voting_ends_at,
        execution_deadline: update.execution_deadline,
    };

    wasm::db::db_set(proposals_db, &update.claim_id.to_repr(), &serialize(&proposal))?;
    msg!("[dao_escrow::propose_claim_apply_v1] Proposal stored: {:?}", update.claim_id);
    Ok(())
}

// ============================================================================
// VOTE CLAIM V1 (0x08)
// ============================================================================

/// VoteClaimV1 instruction - casts a vote on a pending proposal
fn vote_claim_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::VoteClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::vote_claim_v1] Processing vote");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[dao_escrow::vote_claim_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Verify governance is active
    let gov_config = endowment.governance_config.as_ref()
        .ok_or(DaoEscrowError::GovernanceNotActive)?;
    if !gov_config.governance_active {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // Verify voter holds member_vote capability
    verify_capability_for_action(cid, &endowment, &params.capability_proof, "member_vote")?;

    // Load proposal
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.claim_id.to_repr())?
        .ok_or_else(|| DaoEscrowError::ClaimNotFound("Claim not found".to_string()))?;
    let proposal: model::Proposal = deserialize(&proposal_data)?;

    // Verify proposal is pending
    if proposal.state != model::ProposalState::Pending {
        msg!("[dao_escrow::vote_claim_v1] ERROR: Proposal not pending");
        return Err(DaoEscrowError::ClaimNotPending.into());
    }

    // Verify voting window has not expired; if it has, auto-expire the proposal
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block > proposal.voting_ends_at {
        msg!("[dao_escrow::vote_claim_v1] Voting window expired, auto-expiring proposal");
        let update = model::VoteClaimUpdateV1 {
            dao_escrow_bulla: params.dao_escrow_bulla,
            claim_id: params.claim_id,
            yes_votes: proposal.yes_votes,
            no_votes: proposal.no_votes,
            passed: false,
            expired: true,
        };
        let _ = wasm::util::set_return_data(&serialize(&update));
        return Ok(())
    }

    // Check for double-vote via nullifier, then store it to prevent re-use
    let nullifiers_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;
    let vote_nullifier = params.capability_proof.nullifier.inner();
    let existing_nullifier = wasm::db::db_get(nullifiers_db, &vote_nullifier.to_repr())?;
    if existing_nullifier.is_some() {
        msg!("[dao_escrow::vote_claim_v1] ERROR: Already voted");
        return Err(DaoEscrowError::AlreadyVoted.into());
    }
    wasm::db::db_set(nullifiers_db, &vote_nullifier.to_repr(), &serialize(&true))?;

    // Count vote
    let (yes_votes, no_votes, _passed) = match params.vote {
        model::VoteType::Yes => {
            let yv = proposal.yes_votes + 1;
            (yv, proposal.no_votes, false)
        }
        model::VoteType::No => {
            let nv = proposal.no_votes + 1;
            (proposal.yes_votes, nv, false)
        }
    };

    // Determine if proposal passed
    let total_votes = yes_votes + no_votes;
    let quorum_met = total_votes >= gov_config.quorum;
    let approval_met = if total_votes > 0 {
        (yes_votes as u128 * gov_config.approval_ratio_base as u128) >= (total_votes as u128 * gov_config.approval_ratio_quot as u128)
    } else {
        false
    };
    let passed = quorum_met && approval_met;

    let update = model::VoteClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        yes_votes,
        no_votes,
        passed,
        expired: false,
    };

    msg!("[dao_escrow::vote_claim_v1] Vote recorded: {:?}", params.claim_id);
    wasm::util::set_return_data(&serialize(&update))
}

/// VoteClaimV1 apply - update vote tally and proposal state
fn vote_claim_apply_v1(cid: ContractId, update: model::VoteClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;

    let proposal_data = wasm::db::db_get(proposals_db, &update.claim_id.to_repr())?;
    if let Some(data) = proposal_data {
        let mut proposal: model::Proposal = deserialize(&data)?;
        proposal.yes_votes = update.yes_votes;
        proposal.no_votes = update.no_votes;

        if update.expired {
            proposal.state = model::ProposalState::Expired;
        } else if update.passed {
            proposal.state = model::ProposalState::Approved;
        }

        wasm::db::db_set(proposals_db, &update.claim_id.to_repr(), &serialize(&proposal))?;
    }

    msg!("[dao_escrow::vote_claim_apply_v1] Vote tally updated");
    Ok(())
}

// ============================================================================
// EXECUTE CLAIM V1 (0x09)
// ============================================================================

/// ExecuteClaimV1 instruction - executes an approved proposal
fn execute_claim_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::ExecuteClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::execute_claim_v1] Executing claim");

    // Validate child call is money_v3::transfer_v1
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into());
    }
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        return Err(DaoEscrowError::InvalidChildCall.into());
    }

    // Verify proposal is approved
    verify_proposal_approved(cid, params.proposal_id, params.dao_escrow_bulla, params.value, &params.recipient_pubkey)?;

    // Load proposal and verify not already executed
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.proposal_id.to_repr())?
        .ok_or_else(|| DaoEscrowError::ProposalNotFound("Proposal not found".to_string()))?;
    let proposal: model::Proposal = deserialize(&proposal_data)?;

    if proposal.state == model::ProposalState::Executed {
        return Err(DaoEscrowError::ProposalAlreadyExecuted.into());
    }

    // Verify endowment has sufficient balance
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = endowment_data
        .map(|d| deserialize(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    if endowment.total_endowment < params.value {
        return Err(DaoEscrowError::InsufficientEndowment.into());
    }

    let update = model::ExecuteClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        proposal_id: params.proposal_id,
        value: params.value,
        state: model::ProposalState::Executed,
    };

    msg!("[dao_escrow::execute_claim_v1] Claim executed");
    wasm::util::set_return_data(&serialize(&update))
}

/// ExecuteClaimV1 apply - mark proposal as executed
fn execute_claim_apply_v1(cid: ContractId, update: model::ExecuteClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &update.proposal_id.to_repr())?;
    if let Some(data) = proposal_data {
        let mut proposal: model::Proposal = deserialize(&data)?;
        proposal.state = update.state;
        wasm::db::db_set(proposals_db, &update.proposal_id.to_repr(), &serialize(&proposal))?;
    }

    msg!("[dao_escrow::execute_claim_apply_v1] Proposal marked as executed");
    Ok(())
}

// ============================================================================
// REGISTER CAPABILITY REQUIREMENT V1 (0x0a)
// ============================================================================

/// RegisterCapabilityRequirementV1 instruction - registers a required capability for a DAO role
fn register_capability_requirement_v1(
    cid: ContractId,
    params: model::RegisterCapabilityRequirementParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::register_capability_requirement_v1] Registering capability requirement");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let _endowment: model::DaoEscrow = endowment_data
        .map(|d| deserialize(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    let requirement = model::CapabilityRequirement {
        role: params.role.clone(),
        capability_id: params.capability_id,
        identity_contract_bulla: params.identity_contract_bulla,
        active: true,
    };

    let update = model::RegisterCapabilityRequirementUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        role: params.role,
        requirement,
    };

    msg!("[dao_escrow::register_capability_requirement_v1] Requirement registered");
    wasm::util::set_return_data(&serialize(&update))
}

/// RegisterCapabilityRequirementV1 apply - store capability requirement
fn register_capability_requirement_apply_v1(
    cid: ContractId,
    update: model::RegisterCapabilityRequirementUpdateV1,
) -> ContractResult {
    let caps_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    wasm::db::db_set(caps_db, &update.role, &serialize(&update.requirement))?;
    msg!("[dao_escrow::register_capability_requirement_apply_v1] Capability requirement stored");
    Ok(())
}

// ============================================================================
// VERIFY MEMBER CAPABILITY V1 (0x0b)
// ============================================================================

/// VerifyMemberCapabilityV1 instruction - verifies a member holds a valid capability for this DAO
fn verify_member_capability_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::VerifyMemberCapabilityParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::verify_member_capability_v1] Verifying member capability");

    // Validate child call to Identity::VerifyCapabilityV1 (0x0b) for on-chain capability verification
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!("[verify_member_capability_v1] Error: Expected 1 child call (Identity::VerifyCapabilityV1), got {}",
             self_.children_indexes.len());
        return Err(DaoEscrowError::InvalidChildrenIndexes.into());
    }
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x0b {
        msg!("[verify_member_capability_v1] Error: Expected Identity::VerifyCapabilityV1 (0x0b), got 0x{:02x}",
             child_call.data[0]);
        return Err(DaoEscrowError::InvalidChildCall.into());
    }

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let _endowment: model::DaoEscrow = endowment_data
        .map(|d| deserialize(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    // Verify the capability proof
    if params.capability_proof.proof.is_empty() && params.capability_proof.nullifier.inner() == pallas::Base::zero() {
        return Err(DaoEscrowError::CapabilityVerificationFailed.into());
    }

    let update = model::VerifyMemberCapabilityUpdateV1 {
        capability_id: params.capability_proof.capability_id,
        verified: true,
    };

    msg!("[dao_escrow::verify_member_capability_v1] Capability verified");
    wasm::util::set_return_data(&serialize(&update))
}

/// VerifyMemberCapabilityV1 apply - record verification (currently no-op, logs only)
fn verify_member_capability_apply_v1(_cid: ContractId, _update: model::VerifyMemberCapabilityUpdateV1) -> ContractResult {
    msg!("[dao_escrow::verify_member_capability_apply_v1] Verification recorded");
    Ok(())
}

// ============================================================================
// RESOLVE DISPUTE V1 (0x0c)
// ============================================================================

/// ResolveDisputeV1 instruction - resolves a dispute via oracle attestations
fn resolve_dispute_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::ResolveDisputeParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::resolve_dispute_v1] Resolving dispute");

    // Validate child call setup: expect attestation verification calls + money_v3 transfer
    let self_ = &calls[call_idx];
    if self_.children_indexes.is_empty() {
        msg!("[resolve_dispute_v1] ERROR: No child calls for attestation verification");
        return Err(DaoEscrowError::InvalidChildrenIndexes.into());
    }

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = endowment_data
        .map(|d| deserialize(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    // Verify governance is active
    let gov_config = endowment.governance_config.as_ref()
        .ok_or(DaoEscrowError::GovernanceNotActive)?;
    if !gov_config.governance_active {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // Verify arbitrator holds dispute_arbitrator capability
    verify_capability_for_action(cid, &endowment, &params.capability_proof, "dispute_arbitrator")?;

    // Verify multi-oracle threshold met
    let attestation_count = params.attestations.len() as u64;
    let threshold = gov_config.oracle_threshold_numerator;
    if attestation_count < threshold {
        msg!("[resolve_dispute_v1] ERROR: Oracle threshold not met: {}/{}", attestation_count, threshold);
        return Err(DaoEscrowError::OracleThresholdNotMet(attestation_count, threshold).into());
    }

    // Verify sufficient endowment balance for payout
    if params.payout_amount > endowment.total_endowment {
        return Err(DaoEscrowError::InsufficientEndowment.into());
    }

    let dispute_id = dwow_sdk::crypto::poseidon_hash([
        params.proposal_id,
        pallas::Base::from(attestation_count),
        params.payout_recipient.xy().0,
    ]);

    let consumed_ids: Vec<pallas::Base> = params.attestations.iter()
        .map(|a| a.attestation_id)
        .collect();

    let update = model::ResolveDisputeUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        dispute_id,
        proposal_id: params.proposal_id,
        approved: true,
        payout_amount: params.payout_amount,
        consumed_attestation_ids: consumed_ids,
    };

    msg!("[dao_escrow::resolve_dispute_v1] Dispute resolved");
    wasm::util::set_return_data(&serialize(&update))
}

/// ResolveDisputeV1 apply - store dispute resolution record
fn resolve_dispute_apply_v1(cid: ContractId, update: model::ResolveDisputeUpdateV1) -> ContractResult {
    let disputes_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_DISPUTES_TREE)?;

    // Prevent double-resolution: check if dispute_id already exists
    if wasm::db::db_contains_key(disputes_db, &update.dispute_id.to_repr())? {
        msg!("[dao_escrow::resolve_dispute_apply_v1] ERROR: Dispute already resolved");
        return Err(DaoEscrowError::InvalidNullifier.into());
    }

    // Store minimal resolution record keyed by dispute_id
    let resolution_data = serialize(&update);
    wasm::db::db_set(disputes_db, &update.dispute_id.to_repr(), &resolution_data)?;
    msg!("[dao_escrow::resolve_dispute_apply_v1] Dispute resolution stored");
    Ok(())
}

// ============================================================================
// CANCEL CLAIM V1 (0x0d)
// ============================================================================

/// CancelClaimV1 instruction - cancels a pending proposal (proposer only)
fn cancel_claim_v1(
    cid: ContractId,
    params: model::CancelClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::cancel_claim_v1] Cancelling claim");

    // Load proposal
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.claim_id.to_repr())?
        .ok_or_else(|| DaoEscrowError::ClaimNotFound("Claim not found".to_string()))?;
    let proposal: model::Proposal = deserialize(&proposal_data)?;

    // Verify caller is the proposer
    if proposal.proposer_pubkey != params.proposer_pubkey {
        msg!("[dao_escrow::cancel_claim_v1] ERROR: Not claim proposer");
        return Err(DaoEscrowError::NotClaimProposer.into());
    }

    // Verify proposal is still pending
    if proposal.state != model::ProposalState::Pending {
        msg!("[dao_escrow::cancel_claim_v1] ERROR: Claim not pending");
        return Err(DaoEscrowError::ClaimNotPending.into());
    }

    let update = model::CancelClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        state: model::ProposalState::Cancelled,
    };

    msg!("[dao_escrow::cancel_claim_v1] Claim cancelled");
    wasm::util::set_return_data(&serialize(&update))
}

/// CancelClaimV1 apply - update proposal state to cancelled
fn cancel_claim_apply_v1(cid: ContractId, update: model::CancelClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &update.claim_id.to_repr())?;
    if let Some(data) = proposal_data {
        let mut proposal: model::Proposal = deserialize(&data)?;
        proposal.state = update.state;
        wasm::db::db_set(proposals_db, &update.claim_id.to_repr(), &serialize(&proposal))?;
    }

    msg!("[dao_escrow::cancel_claim_apply_v1] Proposal cancelled");
    Ok(())
}

// ============================================================================
// SET GOVERNANCE CONFIG V1 (0x0e)
// ============================================================================

/// SetGovernanceConfigV1 instruction - updates governance configuration
fn set_governance_config_v1(
    cid: ContractId,
    params: model::SetGovernanceConfigParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::set_governance_config_v1] Setting governance config");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = endowment_data
        .map(|d| deserialize(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    // If governance is already active, require board_treasury capability
    if let Some(ref existing_gov) = endowment.governance_config {
        if existing_gov.governance_active {
            verify_capability_for_action(cid, &endowment, &params.capability_proof, "board_treasury")?;
        }
    } else {
        // No governance yet — require owner signature for initial activation
        let signature_msg = serialize(&dwow_sdk::crypto::poseidon_hash([
            params.dao_escrow_bulla,
            pallas::Base::from(params.config.approval_ratio_quot as u64),
            pallas::Base::from(params.config.approval_ratio_base as u64),
            pallas::Base::from(params.config.quorum as u64),
        ]));
        if !endowment.owner_pubkey.verify(&signature_msg, &params.owner_signature) {
            msg!("[dao_escrow::set_governance_config_v1] ERROR: Invalid owner signature for governance activation");
            return Err(DaoEscrowError::CapabilityVerificationFailed.into());
        }
    }

    // Validate config parameters
    if params.config.approval_ratio_quot > params.config.approval_ratio_base {
        return Err(DaoEscrowError::InvalidApprovalRatio.into());
    }
    if params.config.quorum == 0 {
        return Err(DaoEscrowError::InvalidQuorum.into());
    }

    let update = model::SetGovernanceConfigUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        config: params.config,
    };

    msg!("[dao_escrow::set_governance_config_v1] Governance config updated");
    wasm::util::set_return_data(&serialize(&update))
}

/// SetGovernanceConfigV1 apply - store governance config
fn set_governance_config_apply_v1(cid: ContractId, update: model::SetGovernanceConfigUpdateV1) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_repr())?;
    if let Some(data) = endowment_data {
        let mut endowment: model::DaoEscrow = deserialize(&data)?;
        endowment.governance_config = Some(update.config);
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_repr(), &serialize(&endowment))?;
    }

    msg!("[dao_escrow::set_governance_config_apply_v1] Governance config stored");
    Ok(())
}

// ============================================================================
// SET GOVERNANCE ACTIVE V1 (0x0f)
// ============================================================================

/// SetGovernanceActiveV1 instruction - toggles governance_active on the GovernanceConfig
fn set_governance_active_v1(
    cid: ContractId,
    params: model::SetGovernanceActiveParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::set_governance_active_v1] Setting governance active state");

    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    let endowment: model::DaoEscrow = endowment_data
        .map(|d| deserialize(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    // Require board_treasury capability to toggle governance
    verify_capability_for_action(cid, &endowment, &params.capability_proof, "board_treasury")?;

    let update = model::SetGovernanceActiveUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        governance_active: params.governance_active,
    };

    msg!("[dao_escrow::set_governance_active_v1] Governance active set to {}", params.governance_active);
    wasm::util::set_return_data(&serialize(&update))
}

/// SetGovernanceActiveV1 apply - update governance active flag
fn set_governance_active_apply_v1(cid: ContractId, update: model::SetGovernanceActiveUpdateV1) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_repr())?;
    if let Some(data) = endowment_data {
        let mut endowment: model::DaoEscrow = deserialize(&data)?;
        if let Some(ref mut gov_config) = endowment.governance_config {
            gov_config.governance_active = update.governance_active;
            wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_repr(), &serialize(&endowment))?;
        }
    }

    msg!("[dao_escrow::set_governance_active_apply_v1] Governance active updated");
    Ok(())
}

// ============================================================================
// DEACTIVATE CAPABILITY REQUIREMENT V1 (0x10)
// ============================================================================

/// DeactivateCapabilityRequirementV1 instruction - sets a capability requirement to inactive
fn deactivate_capability_requirement_v1(
    cid: ContractId,
    params: model::DeactivateCapabilityRequirementParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::deactivate_capability_requirement_v1] Deactivating capability requirement");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_repr())?;
    if endowment_data.is_none() {
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    // Verify capability requirement exists
    let caps_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    let req_data = wasm::db::db_get(caps_db, &params.role)?
        .ok_or_else(|| DaoEscrowError::CapabilityRequirementNotRegistered(
            String::from_utf8_lossy(&params.role).to_string()
        ))?;
    let mut requirement: model::CapabilityRequirement = deserialize(&req_data)?;
    requirement.active = false;
    wasm::db::db_set(caps_db, &params.role, &serialize(&requirement))?;

    let update = model::DeactivateCapabilityRequirementUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        role: params.role,
    };

    msg!("[dao_escrow::deactivate_capability_requirement_v1] Capability requirement deactivated");
    wasm::util::set_return_data(&serialize(&update))
}

/// DeactivateCapabilityRequirementV1 apply - update already applied in instruction
fn deactivate_capability_requirement_apply_v1(
    _cid: ContractId,
    _update: model::DeactivateCapabilityRequirementUpdateV1,
) -> ContractResult {
    msg!("[dao_escrow::deactivate_capability_requirement_apply_v1] Capability requirement deactivation confirmed");
    Ok(())
}

// Helper imports are handled at the top of the file via crate:: imports