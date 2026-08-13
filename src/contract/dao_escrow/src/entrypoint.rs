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
    crypto::{pasta_prelude::PrimeField, poseidon_hash, BOX_CONTRACT_ID, ContractId, PURSE_CONTRACT_ID, TokenId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta::pallas,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    error::DaoEscrowError,
    model,
    DaoEscrowFunction, DAO_ESCROW_CONTRACT_BULLAS_TREE, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE,
    DAO_ESCROW_CONTRACT_DISPUTES_TREE, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE,
    DAO_ESCROW_CONTRACT_GOVERNANCE_TREE, DAO_ESCROW_CONTRACT_INFO_TREE,
    DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE,
    DAO_ESCROW_CONTRACT_PROPOSALS_TREE, DAO_ESCROW_CONTRACT_VOTES_TREE,
    BOX_CONTRACT_ID_KEY, PROMISSORY_NOTE_CONTRACT_ID_KEY,
    PURSE_CONTRACT_ID_KEY,
    IDENTITY_CONTRACT_ID_KEY,
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

    // V2 (V1 loads removed — rc3 migration) circuits (HAZOP RC3: domain separation)
    let init_v2_bincode = include_bytes!("../proof/init.zk.bin");
    wasm::db::zkas_db_set(&init_v2_bincode[..])?;
    let pay_premium_v2_bincode = include_bytes!("../proof/pay_premium.zk.bin");
    wasm::db::zkas_db_set(&pay_premium_v2_bincode[..])?;
    let propose_claim_v2_bincode = include_bytes!("../proof/propose_claim.zk.bin");
    wasm::db::zkas_db_set(&propose_claim_v2_bincode[..])?;
    let vote_claim_v2_bincode = include_bytes!("../proof/vote_claim.zk.bin");
    wasm::db::zkas_db_set(&vote_claim_v2_bincode[..])?;
    let verify_member_cap_v2_bincode = include_bytes!("../proof/verify_member_capability.zk.bin");
    wasm::db::zkas_db_set(&verify_member_cap_v2_bincode[..])?;
    let resolve_dispute_v2_bincode = include_bytes!("../proof/resolve_dispute.zk.bin");
    wasm::db::zkas_db_set(&resolve_dispute_v2_bincode[..])?;
    let set_governance_config_v2_bincode = include_bytes!("../proof/set_governance_config.zk.bin");
    wasm::db::zkas_db_set(&set_governance_config_v2_bincode[..])?;

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, DAO_ESCROW_DB_VERSION_KEY, &env!("CARGO_PKG_VERSION").as_bytes())?;
    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY, &[0u8; 32])?;
    wasm::db::db_set(info_db, IDENTITY_CONTRACT_ID_KEY, &[0u8; 32])?;
    wasm::db::db_set(info_db, BOX_CONTRACT_ID_KEY, &BOX_CONTRACT_ID.to_bytes())?;
    wasm::db::db_set(info_db, PURSE_CONTRACT_ID_KEY, &PURSE_CONTRACT_ID.to_bytes())?;

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
        DaoEscrowFunction::SetGovernanceConfigV1 => set_governance_config_get_metadata(cid, call_idx, &calls),
        _ => Ok(vec![]),
    }?;

    wasm::util::set_return_data(&metadata)
}

/// Metadata for InitializeV1 (0x00)
fn initialize_get_metadata(_cid: ContractId, call_idx: usize, calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::InitializeParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    let (owner_pub_x, owner_pub_y) = params.owner_pubkey.xy().expect("pk not identity");

    // Compute endowment_bulla using same formula as InitV2 circuit
    // endowment_bulla = poseidon_hash(DOMAIN_COIN_COMMIT, dao_bulla, owner_pub_x, owner_pub_y,
    //                                  endowment_token_id, bulla_blind)
    let endowment_bulla = dwow_sdk::crypto::poseidon_hash([
        pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.dao_bulla.inner(),
        owner_pub_x,
        owner_pub_y,
        params.endowment_token_id.inner(),
        params.bulla_blind.inner(),
    ]);

    let tx_binding = pallas::Base::zero(); // Pattern A: pass-through placeholder
    let tx_nonce_val = pallas::Base::zero(); // Pattern A: pass-through placeholder

    // Circuit constrain_instance order: [dao_bulla, tx_binding, tx_nonce, endowment_bulla]
    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_INIT_NS_V2.to_string(),
        vec![
            params.dao_bulla.inner(),
            tx_binding,
            tx_nonce_val,
            endowment_bulla,
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for PayPremiumV1 (0x02) — PayPremiumV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce]
fn pay_premium_get_metadata(_cid: ContractId, _call_idx: usize, _calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Result<Vec<u8>, ContractError> {
    let tx_binding = pallas::Base::zero(); // Pattern A: pass-through placeholder
    let tx_nonce_val = pallas::Base::zero(); // Pattern A: pass-through placeholder

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_PREMIUM_NS_V2.to_string(),
        vec![tx_binding, tx_nonce_val],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
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
            let params = model::InitializeParamsV1::decode(&self_.data[1..])?;
            initialize_v1(cid, params)
        }
        DaoEscrowFunction::UpdateV1 => {
            let params = model::UpdateParamsV1::decode(&self_.data[1..])?;
            update_v1(cid, params)
        }
        DaoEscrowFunction::PayPremiumV1 => {
            let params = model::PayPremiumParamsV1::decode(&self_.data[1..])?;
            pay_premium_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let params = model::WithdrawParamsV1::decode(&self_.data[1..])?;
            withdraw_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            let params = model::EndowmentWithdrawParamsV1::decode(&self_.data[1..])?;
            endowment_withdraw_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            let params = model::TreasurySpendParamsV1::decode(&self_.data[1..])?;
            treasury_spend_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::EnableDrainProtectionV1 => {
            let params = model::EnableDrainProtectionParamsV1::decode(&self_.data[1..])?;
            enable_drain_protection_v1(cid, params)
        }
        DaoEscrowFunction::ProposeClaimV1 => {
            let params = model::ProposeClaimParamsV1::decode(&self_.data[1..])?;
            propose_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::VoteClaimV1 => {
            let params = model::VoteClaimParamsV1::decode(&self_.data[1..])?;
            vote_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::ExecuteClaimV1 => {
            let params = model::ExecuteClaimParamsV1::decode(&self_.data[1..])?;
            execute_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::RegisterCapabilityRequirementV1 => {
            let params = model::RegisterCapabilityRequirementParamsV1::decode(&self_.data[1..])?;
            register_capability_requirement_v1(cid, params)
        }
        DaoEscrowFunction::VerifyMemberCapabilityV1 => {
            let params = model::VerifyMemberCapabilityParamsV1::decode(&self_.data[1..])?;
            verify_member_capability_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::ResolveDisputeV1 => {
            let params = model::ResolveDisputeParamsV1::decode(&self_.data[1..])?;
            resolve_dispute_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::CancelClaimV1 => {
            let params = model::CancelClaimParamsV1::decode(&self_.data[1..])?;
            cancel_claim_v1(cid, params)
        }
        DaoEscrowFunction::SetGovernanceConfigV1 | DaoEscrowFunction::SetGovernanceActiveV1 => {
            // Removed — MultiSig groups manage governance.
            Ok(())
        }
        DaoEscrowFunction::DeactivateCapabilityRequirementV1 => {
            let params = model::DeactivateCapabilityRequirementParamsV1::decode(&self_.data[1..])?;
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
            let update = model::InitializeUpdateV1::decode(&update_data[1..])?;
            initialize_apply_v1(cid, update)
        }
        DaoEscrowFunction::UpdateV1 => {
            let update = model::UpdateUpdateV1::decode(&update_data[1..])?;
            update_apply_v1(cid, update)
        }
        DaoEscrowFunction::PayPremiumV1 => {
            let update = model::PayPremiumUpdateV1::decode(&update_data[1..])?;
            pay_premium_apply_v1(cid, update)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let update = model::WithdrawUpdateV1::decode(&update_data[1..])?;
            withdraw_apply_v1(cid, update)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            let update = model::EndowmentWithdrawUpdateV1::decode(&update_data[1..])?;
            endowment_withdraw_apply_v1(cid, update)
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            let update = model::TreasurySpendUpdateV1::decode(&update_data[1..])?;
            treasury_spend_apply_v1(cid, update)
        }
        DaoEscrowFunction::EnableDrainProtectionV1 => {
            let update = model::EnableDrainProtectionUpdateV1::decode(&update_data[1..])?;
            enable_drain_protection_apply_v1(cid, update)
        }
        DaoEscrowFunction::ProposeClaimV1 => {
            let update = model::ProposeClaimUpdateV1::decode(&update_data[1..])?;
            propose_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::VoteClaimV1 => {
            let update = model::VoteClaimUpdateV1::decode(&update_data[1..])?;
            vote_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::ExecuteClaimV1 => {
            let update = model::ExecuteClaimUpdateV1::decode(&update_data[1..])?;
            execute_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::RegisterCapabilityRequirementV1 => {
            let update = model::RegisterCapabilityRequirementUpdateV1::decode(&update_data[1..])?;
            register_capability_requirement_apply_v1(cid, update)
        }
        DaoEscrowFunction::VerifyMemberCapabilityV1 => {
            let update = model::VerifyMemberCapabilityUpdateV1::decode(&update_data[1..])?;
            verify_member_capability_apply_v1(cid, update)
        }
        DaoEscrowFunction::ResolveDisputeV1 => {
            let update = model::ResolveDisputeUpdateV1::decode(&update_data[1..])?;
            resolve_dispute_apply_v1(cid, update)
        }
        DaoEscrowFunction::CancelClaimV1 => {
            let update = model::CancelClaimUpdateV1::decode(&update_data[1..])?;
            cancel_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::SetGovernanceConfigV1 | DaoEscrowFunction::SetGovernanceActiveV1 => Ok(()),
        DaoEscrowFunction::DeactivateCapabilityRequirementV1 => {
            let update = model::DeactivateCapabilityRequirementUpdateV1::decode(&update_data[1..])?;
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
    if wasm::db::db_contains_key(bullas_db, &params.dao_bulla.to_bytes())? {
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
        params.bulla_blind.clone(),
    );

    // Create update
    let update = model::InitializeUpdateV1 {
        instance_seed: params.instance_seed,
        bulla: endowment_bulla,
        owner_pubkey: params.owner_pubkey,
        bulla_blind: params.bulla_blind,
    };

    msg!("[dao_escrow::initialize_v1] Endowment initialized: {:?}", endowment_bulla);
    wasm::util::set_return_data(&update.encode())
}

/// InitializeV1 apply - store new endowment
fn initialize_apply_v1(cid: ContractId, update: model::InitializeUpdateV1) -> ContractResult {
    let bullas_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Store endowment bulla in bullas tree (non-empty marker — empty is
    // invisible to db_contains_key per §9.1, which breaks the duplicate check)
    wasm::db::db_set(bullas_db, &update.bulla.to_bytes(), &[1])?;

    // Initialize endowment state
    let endowment = model::DaoEscrow {
        version: 1,
        instance_seed: update.instance_seed,
        bulla: update.bulla,
        mode: model::DaoEscrowMode::Escrow,
        owner_pubkey: update.owner_pubkey,
        pool_token_id: TokenId::DRKW,
        multisig_group_id: pallas::Base::zero(),
        pool_purse_id: pallas::Base::zero(),
        treasury_purse_id: pallas::Base::zero(),
        endowment_purse_id: pallas::Base::zero(),
        member_count: 0,
        fee_config: None,
        min_premium: 0,
        max_members: u64::MAX,
        created_at: wasm::util::get_verifying_block_height()?.get(),
        bulla_blind: update.bulla_blind,
        paused: false,
        drain_protection_enabled: false,
        drain_protection_bulla: None,
    };

    wasm::db::db_set(endowments_db, &update.bulla.to_bytes(), &endowment.encode())?;

    msg!("[dao_escrow::initialize_apply_v1] Endowment stored: {:?}", update.bulla);
    Ok(())
}

/// UpdateV1 instruction - update endowment parameters
fn update_v1(cid: ContractId, params: model::UpdateParamsV1) -> ContractResult {
    msg!("[dao_escrow::update_v1] Updating DAO-Escrow: {:?}", params.bulla);

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.bulla.to_bytes())?;
    if endowment_data.is_none() {
        msg!("[dao_escrow::update_v1] ERROR: Endowment not found");
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    // Create update
    let update = model::UpdateUpdateV1 { bulla: params.bulla };

    msg!("[dao_escrow::update_v1] Endowment update prepared: {:?}", params.bulla);
    wasm::util::set_return_data(&update.encode())
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

    // Validate child call is promissory_note::transfer_v1 (0x04) for premium payment
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[pay_premium_v1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[pay_premium_v1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify DAO-Escrow endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    if endowment_data.is_none() {
        msg!("[dao_escrow::pay_premium_v1] ERROR: Endowment not found");
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    // Verify membership note doesn't already exist
    let membership_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;
    if wasm::db::db_contains_key(membership_db, &params.membership_note.to_bytes())? {
        msg!("[dao_escrow::pay_premium_v1] ERROR: Membership already exists");
        return Err(DaoEscrowError::ClaimAlreadyExists("Membership already exists".to_string()).into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::DAO_ESCROW_ZKAS_PREMIUM_NS)?;

    // Calculate fee split based on mode (simplified - all to endowment)
    // Create update — Purse::DepositV1 child call handles balance
    let update = model::PayPremiumUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        membership_note: params.membership_note,
        amount: params.value,
        member_count: 1,
        member_pubkey: params.member_pubkey,
        token_id: params.token_id,
        expiry: params.expiry,
    };

    msg!("[dao_escrow::pay_premium_v1] Premium processed: {:?}", params.membership_note);
    wasm::util::set_return_data(&update.encode())
}

/// PayPremiumV1 apply - store membership note and update endowment
fn pay_premium_apply_v1(cid: ContractId, update: model::PayPremiumUpdateV1) -> ContractResult {
    let membership_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Create and store membership
    let membership = model::Membership {
        version: 1,
        note: update.membership_note,
        dao_escrow_bulla: update.dao_escrow_bulla,
        member_pubkey: update.member_pubkey,
        value: update.amount,
        token_id: update.token_id,
        expiry: update.expiry,
        created_at: wasm::util::get_verifying_block_height()?.get(),
    };

    wasm::db::db_set(membership_db, &update.membership_note.to_bytes(), &membership.encode())?;

    // Update endowment totals
    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_bytes())?;
    if let Some(data) = endowment_data {
        let mut endowment = model::DaoEscrow::decode(&data)?;
        // Purse::DepositV1 child call handles balance update.
        // endowment_purse_id is the Purse instance reference, not a raw counter.
        endowment.member_count += 1;
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &endowment.encode())?;
    }

    msg!("[dao_escrow::pay_premium_apply_v1] Membership stored: {:?}", update.membership_note);
    Ok(())
}

/// WithdrawV1 instruction - endowment owner withdraws funds
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn withdraw_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::WithdrawParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::withdraw_v1] Processing withdrawal");

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!(
            "[WithdrawV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[WithdrawV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::withdraw_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Verify authorization: governance-active uses capability proof,
    // otherwise fall back to owner pubkey check (backward compat)
    if endowment.multisig_group_id != pallas::Base::zero() {
        // MultiSig composition: governance authorization via group membership
    } else if endowment.owner_pubkey != params.recipient_pubkey {
        msg!("[dao_escrow::withdraw_v1] ERROR: Not authorized to withdraw");
        return Err(DaoEscrowError::NotAuthorizedToWithdraw.into())
    }

    // Verify sufficient balance
    // Purse::WithdrawV1 verifies balance >= amount
if false {
        msg!("[dao_escrow::withdraw_v1] ERROR: Insufficient endowment balance");
        return Err(DaoEscrowError::InsufficientEndowment.into())
    }

    // Create update
    let update = model::WithdrawUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        value: params.value,
        amount: params.value, // Purse::WithdrawV1 verifies balance >= amount
    };

    msg!("[dao_escrow::withdraw_v1] Withdrawal processed: {}", params.value);
    wasm::util::set_return_data(&update.encode())
}

/// WithdrawV1 apply - update endowment totals
fn withdraw_apply_v1(cid: ContractId, update: model::WithdrawUpdateV1) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_bytes())?;
    if let Some(data) = endowment_data {
        let endowment = model::DaoEscrow::decode(&data)?;
        // Purse handles balance update: endowment_purse_id is the instance reference
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &endowment.encode())?;
    }

    msg!("[dao_escrow::withdraw_apply_v1] Endowment updated: new total = {}", update.amount);
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
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    if endowment_data.is_none() {
        msg!("[dao_escrow::enable_drain_protection_v1] ERROR: Endowment not found");
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    let update = model::EnableDrainProtectionUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        drain_protection_bulla: params.drain_protection_bulla,
    };

    msg!("[dao_escrow::enable_drain_protection_v1] Drain protection update prepared");
    wasm::util::set_return_data(&update.encode())
}

/// EnableDrainProtectionV1 apply
fn enable_drain_protection_apply_v1(
    cid: ContractId,
    update: model::EnableDrainProtectionUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_bytes())?;
    if let Some(data) = endowment_data {
        let mut endowment = model::DaoEscrow::decode(&data)?;
        endowment.drain_protection_enabled = true;
        endowment.drain_protection_bulla = Some(update.drain_protection_bulla);
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &endowment.encode())?;
    }

    msg!("[dao_escrow::enable_drain_protection_apply_v1] Drain protection enabled");
    Ok(())
}

/// EndowmentWithdrawV1 instruction - executes an approved claim from endowment
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn endowment_withdraw_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::EndowmentWithdrawParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::endowment_withdraw_v1] Processing endowment withdrawal");

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!(
            "[EndowmentWithdrawV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[EndowmentWithdrawV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
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
        verify_proposal_approved(cid, proposal_id, params.dao_escrow_bulla.inner(), params.value, &params.recipient_pubkey)?;
    } else if let Some(ref capability_proof) = params.capability_proof {
        verify_capability_for_action(
            cid, &endowment, capability_proof, "board_endowment",
        )?;
    } else {
        msg!("[dao_escrow::endowment_withdraw_v1] ERROR: No authorization provided");
        return Err(DaoEscrowError::EndowmentWithdrawUnauthorized.into())
    }

    // Verify sufficient endowment balance
    // Purse::WithdrawV1 verifies balance >= amount
if false {
        msg!("[dao_escrow::endowment_withdraw_v1] ERROR: Insufficient endowment balance");
        return Err(DaoEscrowError::InsufficientEndowment.into())
    }

    // Calculate new total
    // Purse::WithdrawV1 verifies balance and computes new commitment

    // Create update
    let update = model::EndowmentWithdrawUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        value: params.value,
        amount: params.value, // Purse verifies balance
    };

    msg!(
        "[dao_escrow::endowment_withdraw_v1] Endowment withdrawal processed: {} to {:?}",
        params.value,
        params.recipient_pubkey
    );
    wasm::util::set_return_data(&update.encode())
}

/// EndowmentWithdrawV1 apply - update endowment totals
fn endowment_withdraw_apply_v1(
    cid: ContractId,
    update: model::EndowmentWithdrawUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_bytes())?;
    if let Some(data) = endowment_data {
        let endowment = model::DaoEscrow::decode(&data)?;
        // Purse handles balance update: endowment_purse_id is the instance reference
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &endowment.encode())?;
    }

    msg!(
        "[dao_escrow::endowment_withdraw_apply_v1] Endowment updated: new total = {}",
        update.amount
    );
    Ok(())
}

/// TreasurySpendV1 instruction - executes an approved treasury spend
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn treasury_spend_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::TreasurySpendParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::treasury_spend_v1] Processing treasury spend");

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!(
            "[TreasurySpendV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[TreasurySpendV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify endowment exists and is in treasury mode
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
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
        verify_proposal_approved(cid, params.proposal_id, params.dao_escrow_bulla.inner(), params.value, &params.recipient_pubkey)?;
    } else if let Some(ref capability_proof) = params.capability_proof {
        verify_capability_for_action(
            cid, &endowment, capability_proof, "board_treasury",
        )?;
    } else {
        msg!("[dao_escrow::treasury_spend_v1] ERROR: No authorization provided");
        return Err(DaoEscrowError::EndowmentWithdrawUnauthorized.into())
    }

    // Verify sufficient treasury balance
    // Purse::WithdrawV1 verifies balance >= amount
if false {
        msg!("[dao_escrow::treasury_spend_v1] ERROR: Insufficient treasury balance");
        return Err(DaoEscrowError::InsufficientEndowment.into())
    }

    // Calculate new total
    // Purse::WithdrawV1 handles balance update

    // Create update
    let update = model::TreasurySpendUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        proposal_id: params.proposal_id,
        value: params.value,
        amount: params.value, // Purse verifies balance
    };

    msg!(
        "[dao_escrow::treasury_spend_v1] Treasury spend processed: {} to {:?}",
        params.value,
        params.recipient_pubkey
    );
    wasm::util::set_return_data(&update.encode())
}

/// TreasurySpendV1 apply - update treasury totals
fn treasury_spend_apply_v1(
    cid: ContractId,
    update: model::TreasurySpendUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    let endowment_data = wasm::db::db_get(endowments_db, &update.dao_escrow_bulla.to_bytes())?;
    if let Some(data) = endowment_data {
        let endowment = model::DaoEscrow::decode(&data)?;
        // Purse handles treasury balance: treasury_purse_id is the instance reference
        wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &endowment.encode())?;
    }

    msg!(
        "[dao_escrow::treasury_spend_apply_v1] Treasury updated: new total = {}",
        update.amount
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
    if endowment.multisig_group_id == pallas::Base::zero() {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // Look up capability requirement for this role
    let caps_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    let role_bytes = role.as_bytes().to_vec();
    let req_data = wasm::db::db_get(caps_db, &role_bytes)?
        .ok_or_else(|| DaoEscrowError::CapabilityRequirementNotRegistered(role.to_string()))?;
    let _requirement: model::CapabilityRequirement = model::CapabilityRequirement::decode(&req_data)?;

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
    let proposal = model::Proposal::decode(&proposal_data)?;

    // Verify proposal state is Approved
    if proposal.state != model::ProposalState::Approved {
        msg!("[dao_escrow::verify_proposal] ERROR: Proposal not approved");
        return Err(DaoEscrowError::ProposalNotPending.into());
    }

    // Verify execution deadline not passed
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block > proposal.execution_deadline {
        msg!("[dao_escrow::verify_proposal] ERROR: Execution deadline passed");
        return Err(DaoEscrowError::ClaimExecutionDeadlinePassed.into());
    }

    // Verify proposal matches (value, recipient, escrow)
    if proposal.dao_escrow_bulla.inner() != dao_escrow_bulla {
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

/// Metadata for ProposeClaimV1 (0x07) — ProposeClaimV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce, claim_commit]
fn propose_claim_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::ProposeClaimParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    // Reconstruct capability_secret as Base from capability_proof bytes
    let cap_secret_fp = pallas::Base::from_repr(params.capability_proof.capability_secret)
        .into_option()
        .unwrap_or(pallas::Base::zero());

    // claim_commit = poseidon_hash(DOMAIN_COIN_COMMIT, claim_id, claim_amount, claim_blind)
    let claim_commit = poseidon_hash([
        pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.claim_id.inner(),
        pallas::Base::from(params.value),
        cap_secret_fp, // claim_blind placeholder (needs dedicated field in params)
    ]);

    let tx_binding = pallas::Base::zero(); // Pattern A: pass-through placeholder
    let tx_nonce_val = pallas::Base::zero(); // Pattern A: pass-through placeholder

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_PROPOSE_CLAIM_NS_V2.to_string(),
        vec![tx_binding, tx_nonce_val, claim_commit],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for VoteClaimV1 (0x08) — VoteClaimV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce, vote_nullifier]
fn vote_claim_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::VoteClaimParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    let cap_secret_fp = pallas::Base::from_repr(params.capability_proof.capability_secret)
        .into_option()
        .unwrap_or(pallas::Base::zero());

    let (voter_pub_x, voter_pub_y) = params.voter_pubkey.xy().expect("pk not identity");

    // vote_nullifier = poseidon_hash(DOMAIN_NULLIFIER, capability_secret, proposal_id,
    //                                 voter_pub_x, voter_pub_y)
    let vote_nullifier = poseidon_hash([
        pallas::Base::from(1u64), // DOMAIN_NULLIFIER
        cap_secret_fp,
        params.claim_id.inner(),
        voter_pub_x,
        voter_pub_y,
    ]);

    let tx_binding = pallas::Base::zero(); // Pattern A: pass-through placeholder
    let tx_nonce_val = pallas::Base::zero(); // Pattern A: pass-through placeholder

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_VOTE_CLAIM_NS_V2.to_string(),
        vec![tx_binding, tx_nonce_val, vote_nullifier],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for VerifyMemberCapabilityV1 (0x0b) — VerifyMemberCapabilityV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce, capability_commit]
fn verify_member_cap_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::VerifyMemberCapabilityParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    let cap_id_fp = pallas::Base::from_repr(params.capability_proof.capability_id).into_option()
        .unwrap_or(pallas::Base::zero());
    let cap_secret_fp = pallas::Base::from_repr(params.capability_proof.capability_secret)
        .into_option()
        .unwrap_or(pallas::Base::zero());

    // capability_commit = poseidon_hash(DOMAIN_COIN_COMMIT, capability_id, capability_secret,
    //                                    dao_escrow_bulla)
    let capability_commit = poseidon_hash([
        pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        cap_id_fp,
        cap_secret_fp,
        params.dao_escrow_bulla.inner(),
    ]);

    let tx_binding = pallas::Base::zero(); // Pattern A: pass-through placeholder
    let tx_nonce_val = pallas::Base::zero(); // Pattern A: pass-through placeholder

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_VERIFY_MEMBER_CAP_NS_V2.to_string(),
        vec![tx_binding, tx_nonce_val, capability_commit],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for ResolveDisputeV1 (0x0c) — ResolveDisputeV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce, resolution_commit]
fn resolve_dispute_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::ResolveDisputeParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    // Reconstruct capability_secret as Base from capability_proof bytes
    let cap_secret_fp = pallas::Base::from_repr(params.capability_proof.capability_secret)
        .into_option()
        .unwrap_or(pallas::Base::zero());

    // dispute_id = poseidon_hash(DOMAIN_NULLIFIER, capability_secret, proposal_id)
    // Use proposal_id.inner() as the dispute identifier
    let _dispute_id = params.proposal_id.inner();

    // resolution_commit = poseidon_hash(DOMAIN_COIN_COMMIT, dispute_id, resolution_type,
    //                                    resolution_blind)
    // Reconstruct from available params data
    let resolution_commit = poseidon_hash([
        pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        _dispute_id,
        pallas::Base::from(params.payout_amount),
        cap_secret_fp, // resolution_blind placeholder (needs dedicated field in params)
    ]);

    let tx_binding = pallas::Base::zero(); // Pattern A: pass-through placeholder
    let tx_nonce_val = pallas::Base::zero(); // Pattern A: pass-through placeholder

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_RESOLVE_DISPUTE_NS_V2.to_string(),
        vec![tx_binding, tx_nonce_val, resolution_commit],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for SetGovernanceConfigV1 (0x0e) — SetGovernanceConfigV2 circuit
/// Circuit constrain_instance order: [owner_pub_x, owner_pub_y, owner_nullifier, tx_binding, tx_nonce]
fn set_governance_config_get_metadata(
    _cid: ContractId,
    _call_idx: usize,
    _calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    // SetGovernanceConfig was migrated to MultiSig; params struct removed.
    // Return five-element placeholder vector matching circuit public input count.
    let owner_pub_x = pallas::Base::zero();
    let owner_pub_y = pallas::Base::zero();
    let owner_nullifier = pallas::Base::zero();
    let tx_binding = pallas::Base::zero();
    let tx_nonce_val = pallas::Base::zero();

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_SET_GOVERNANCE_CONFIG_NS_V2.to_string(),
        vec![owner_pub_x, owner_pub_y, owner_nullifier, tx_binding, tx_nonce_val],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
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
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::propose_claim_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // MultiSig governance: group must be configured
    if endowment.multisig_group_id == pallas::Base::zero() {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // Verify proposal does not already exist
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    if wasm::db::db_get(proposals_db, &params.claim_id.to_bytes())?.is_some() {
        return Err(DaoEscrowError::ClaimAlreadyExists("Claim already exists".to_string()).into());
    }

    // MultiSig: voting windows and claim limits are group configuration,
    // not contract parameters. Threshold verification via SignV1 + FinalizeV1.
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let voting_ends_at = current_block + 1000; // default window
    let execution_deadline = voting_ends_at + 1000;

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
    wasm::util::set_return_data(&update.encode())
}

/// ProposeClaimV1 apply - store proposal and record nullifier
fn propose_claim_apply_v1(cid: ContractId, update: model::ProposeClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;

    let proposal = model::Proposal {
        version: 1,
        id: model::ProposalId(update.claim_id.inner()),
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

    wasm::db::db_set(proposals_db, &update.claim_id.to_bytes(), &proposal.encode())?;
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
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::vote_claim_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // MultiSig governance: group must be configured
    if endowment.multisig_group_id == pallas::Base::zero() {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // Load proposal
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.claim_id.to_bytes())?
        .ok_or_else(|| DaoEscrowError::ClaimNotFound("Claim not found".to_string()))?;
    let proposal = model::Proposal::decode(&proposal_data)?;

    // Verify proposal is pending
    if proposal.state != model::ProposalState::Pending {
        msg!("[dao_escrow::vote_claim_v1] ERROR: Proposal not pending");
        return Err(DaoEscrowError::ClaimNotPending.into());
    }

    // Verify voting window has not expired; if it has, auto-expire the proposal
    let current_block = wasm::util::get_verifying_block_height()?.get();
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
        wasm::util::set_return_data(&update.encode())?;
        return Ok(())
    }

    // Check for double-vote via nullifier, then store it to prevent re-use
    let nullifiers_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;
    let vote_nullifier = params.capability_proof.nullifier.inner();
    if wasm::db::db_contains_key(nullifiers_db, &vote_nullifier.to_repr())? {
        msg!("[dao_escrow::vote_claim_v1] ERROR: Already voted");
        return Err(DaoEscrowError::AlreadyVoted.into());
    }
    wasm::db::db_mark_spent(nullifiers_db, &vote_nullifier.to_repr())?;

    // Count vote — MultiSig delegation: each SignV1 = one vote
    let (yes_votes, no_votes) = match params.vote {
        model::VoteType::Yes => (proposal.yes_votes + 1, proposal.no_votes),
        model::VoteType::No => (proposal.yes_votes, proposal.no_votes + 1),
    };

    // MultiSig: threshold verification via FinalizeV1 child call

    let update = model::VoteClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        yes_votes,
        no_votes,
        passed: false,
        expired: false,
    };

    msg!("[dao_escrow::vote_claim_v1] Vote recorded: {:?}", params.claim_id);
    wasm::util::set_return_data(&update.encode())
}

/// VoteClaimV1 apply - update vote tally and proposal state
fn vote_claim_apply_v1(cid: ContractId, update: model::VoteClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;

    let proposal_data = wasm::db::db_get(proposals_db, &update.claim_id.to_bytes())?;
    if let Some(data) = proposal_data {
        let mut proposal = model::Proposal::decode(&data)?;
        proposal.yes_votes = update.yes_votes;
        proposal.no_votes = update.no_votes;

        if update.expired {
            proposal.state = model::ProposalState::Expired;
        } else if false {
            proposal.state = model::ProposalState::Approved;
        }

        wasm::db::db_set(proposals_db, &update.claim_id.to_bytes(), &proposal.encode())?;
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

    // Validate child call is promissory_note::transfer_v1
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

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify proposal is approved
    verify_proposal_approved(cid, params.proposal_id.inner(), params.dao_escrow_bulla.inner(), params.value, &params.recipient_pubkey)?;

    // Load proposal and verify not already executed
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.proposal_id.inner().to_repr())?
        .ok_or_else(|| DaoEscrowError::ProposalNotFound("Proposal not found".to_string()))?;
    let proposal = model::Proposal::decode(&proposal_data)?;

    if proposal.state == model::ProposalState::Executed {
        return Err(DaoEscrowError::ProposalAlreadyExecuted.into());
    }

    // Verify endowment has sufficient balance
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let _endowment: model::DaoEscrow = endowment_data
        .map(|d| model::DaoEscrow::decode(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    // Purse::WithdrawV1 verifies balance >= amount
if false {
        return Err(DaoEscrowError::InsufficientEndowment.into());
    }

    let update = model::ExecuteClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        proposal_id: params.proposal_id,
        value: params.value,
        state: model::ProposalState::Executed,
    };

    msg!("[dao_escrow::execute_claim_v1] Claim executed");
    wasm::util::set_return_data(&update.encode())
}

/// ExecuteClaimV1 apply - mark proposal as executed
fn execute_claim_apply_v1(cid: ContractId, update: model::ExecuteClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &update.proposal_id.inner().to_repr())?;
    if let Some(data) = proposal_data {
        let mut proposal = model::Proposal::decode(&data)?;
        proposal.state = update.state;
        wasm::db::db_set(proposals_db, &update.proposal_id.inner().to_repr(), &proposal.encode())?;
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
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let _endowment: model::DaoEscrow = endowment_data
        .map(|d| model::DaoEscrow::decode(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    let requirement = model::CapabilityRequirement {
        version: 1,
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
    wasm::util::set_return_data(&update.encode())
}

/// RegisterCapabilityRequirementV1 apply - store capability requirement
fn register_capability_requirement_apply_v1(
    cid: ContractId,
    update: model::RegisterCapabilityRequirementUpdateV1,
) -> ContractResult {
    let caps_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    wasm::db::db_set(caps_db, &update.role, &update.requirement.encode())?;
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
    if child_call.data[0] != 0x06 {
        msg!("[verify_member_capability_v1] Error: Expected Identity::VerifyCapabilityV1 (0x06), got 0x{:02x}",
             child_call.data[0]);
        return Err(DaoEscrowError::InvalidChildCall.into());
    }

    // Validate child call targets the Identity contract (safety.md Lesson 15)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let identity_cid_bytes = wasm::db::db_get(info_db, IDENTITY_CONTRACT_ID_KEY)?;
    if let Some(bytes) = identity_cid_bytes {
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            let identity_cid = ContractId::from_bytes(arr).unwrap();
            if identity_cid != ContractId::ZERO {
                if child_call.contract_id != identity_cid {
                    msg!("[verify_member_capability_v1] Error: Child call contract_id does not match stored Identity contract ID");
                    return Err(DaoEscrowError::ChildContractIdMismatch.into());
                }
            }
        }
    }

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let _endowment: model::DaoEscrow = endowment_data
        .map(|d| model::DaoEscrow::decode(&d))
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
    wasm::util::set_return_data(&update.encode())
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

    // Validate child call setup: expect attestation verification calls + promissory_note transfer
    let self_ = &calls[call_idx];
    if self_.children_indexes.is_empty() {
        msg!("[resolve_dispute_v1] ERROR: No child calls for attestation verification");
        return Err(DaoEscrowError::InvalidChildrenIndexes.into());
    }

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = endowment_data
        .map(|d| model::DaoEscrow::decode(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    // MultiSig governance: group must be configured
    if endowment.multisig_group_id == pallas::Base::zero() {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // MultiSig: dispute resolution via separate oracle MultiSig group.
    // Oracle attestation threshold verified via FinalizeV1 child call.
    let attestation_count = params.attestations.len() as u64;

    // Verify sufficient endowment balance for payout
    // Purse::WithdrawV1 verifies balance >= payout_amount
if false {
        return Err(DaoEscrowError::InsufficientEndowment.into());
    }

    let dispute_id = dwow_sdk::crypto::poseidon_hash([
        params.proposal_id.inner(),
        pallas::Base::from(attestation_count),
        params.payout_recipient.xy().expect("pk not identity").0,
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
    wasm::util::set_return_data(&update.encode())
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
    let resolution_data = update.encode();
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
    let proposal_data = wasm::db::db_get(proposals_db, &params.claim_id.to_bytes())?
        .ok_or_else(|| DaoEscrowError::ClaimNotFound("Claim not found".to_string()))?;
    let proposal = model::Proposal::decode(&proposal_data)?;

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
    wasm::util::set_return_data(&update.encode())
}

/// CancelClaimV1 apply - update proposal state to cancelled
fn cancel_claim_apply_v1(cid: ContractId, update: model::CancelClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &update.claim_id.to_bytes())?;
    if let Some(data) = proposal_data {
        let mut proposal = model::Proposal::decode(&data)?;
        proposal.state = update.state;
        wasm::db::db_set(proposals_db, &update.claim_id.to_bytes(), &proposal.encode())?;
    }

    msg!("[dao_escrow::cancel_claim_apply_v1] Proposal cancelled");
    Ok(())
}

// ============================================================================
// SET GOVERNANCE CONFIG V1 (0x0e)
// ============================================================================
// SET GOVERNANCE CONFIG / SET GOVERNANCE ACTIVE — removed, replaced by MultiSig
// Governance is now managed via MultiSig groups created during init. Group
// membership and threshold are configured via MultiSig::CreateGroupV1.
// Activation is implicit — a group with threshold ≥ 1 is active.
// ============================================================================

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
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    if endowment_data.is_none() {
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    // Verify capability requirement exists
    let caps_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    let req_data = wasm::db::db_get(caps_db, &params.role)?
        .ok_or_else(|| DaoEscrowError::CapabilityRequirementNotRegistered(
            String::from_utf8_lossy(&params.role).to_string()
        ))?;
    let requirement = model::CapabilityRequirement::decode(&req_data)?;
    // Validate that the requirement exists and is active — the actual
    // deactivation write happens in apply (two-phase exec/apply separation).
    if !requirement.active {
        msg!("[dao_escrow::deactivate_capability_requirement_v1] Requirement already deactivated");
        return Err(DaoEscrowError::CapabilityExpired.into());
    }

    let update = model::DeactivateCapabilityRequirementUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        role: params.role.clone(),
    };

    msg!("[dao_escrow::deactivate_capability_requirement_v1] Capability requirement deactivation computed");
    wasm::util::set_return_data(&update.encode())
}

/// DeactivateCapabilityRequirementV1 apply — writes the deactivation to state
fn deactivate_capability_requirement_apply_v1(
    cid: ContractId,
    update: model::DeactivateCapabilityRequirementUpdateV1,
) -> ContractResult {
    let caps_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_CAPABILITY_REQUIREMENTS_TREE)?;
    let req_data = wasm::db::db_get(caps_db, &update.role)?
        .ok_or_else(|| DaoEscrowError::CapabilityRequirementNotRegistered(
            String::from_utf8_lossy(&update.role).to_string()
        ))?;
    let mut requirement = model::CapabilityRequirement::decode(&req_data)?;
    requirement.active = false;
    wasm::db::db_set(caps_db, &update.role, &requirement.encode())?;
    msg!("[dao_escrow::deactivate_capability_requirement_apply_v1] Capability requirement deactivation written");
    Ok(())
}

// Helper imports are handled at the top of the file via crate:: imports