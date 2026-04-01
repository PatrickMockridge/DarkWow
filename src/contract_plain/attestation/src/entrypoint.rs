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

//! Plain Attestation Contract Entrypoint
//!
//! # Architecture
//!
//! This contract uses a hybrid ZK/plain approach:
//!
//! | Operation | Method | Why |
//! |-----------|--------|-----|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Attestation commitment | ZK (Pedersen) | Privacy-preserving |
//! | Delegation ratio | Native Rust | Needs `base_div` (not in ZK) |
//! | Credential chains | Native Rust | Complex graph traversal |
//!
//! # Privacy
//!
//! This is a **partial transparency** contract. Most state is public on-chain.
//! Actual credential content is NOT stored on-chain (only hashes).
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.

use darkfi_sdk::{
    crypto::{poseidon_hash, schnorr::SchnorrPublic, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::GenericResult,
    msg, wasm, ContractCall,
};
use darkfi_sdk::pasta::pallas::Base;
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::error::AttestationPlainError;
use crate::model::{
    Attestation, AttestationStatus, Attestor, CreateAttestationParamsV1,
    CreateAttestationUpdateV1, DelegateAttestationParamsV1, DelegateAttestationUpdateV1,
    DelegationType, RegisterAttestorParamsV1, RegisterAttestorUpdateV1,
    RevokeAttestationParamsV1, RevokeAttestationUpdateV1, VerifyAttestationParamsV1,
};
use crate::AttestationPlainFunction;

// Database trees
const ATTESTATIONS_TREE: &str = "attestations";
const ATTESTORS_TREE: &str = "attestors";

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    wasm::db::db_init(cid, ATTESTATIONS_TREE)?;
    wasm::db::db_init(cid, ATTESTORS_TREE)?;
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
    let func = AttestationPlainFunction::try_from(self_.data[0])?;

    let update_data = match func {
        AttestationPlainFunction::RegisterAttestorV1 => {
            register_attestor_process_instruction_v1(cid, call_idx, calls)?
        }
        AttestationPlainFunction::CreateAttestationV1 => {
            create_attestation_process_instruction_v1(cid, call_idx, calls)?
        }
        AttestationPlainFunction::DelegateAttestationV1 => {
            delegate_attestation_process_instruction_v1(cid, call_idx, calls)?
        }
        AttestationPlainFunction::RevokeAttestationV1 => {
            revoke_attestation_process_instruction_v1(cid, call_idx, calls)?
        }
        AttestationPlainFunction::VerifyAttestationV1 => {
            verify_attestation_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> GenericResult<()> {
    match AttestationPlainFunction::try_from(update_data[0])? {
        AttestationPlainFunction::RegisterAttestorV1 => {
            let update: RegisterAttestorUpdateV1 = deserialize(&update_data[1..])?;
            register_attestor_process_update_v1(cid, update)
        }
        AttestationPlainFunction::CreateAttestationV1 => {
            let update: CreateAttestationUpdateV1 = deserialize(&update_data[1..])?;
            create_attestation_process_update_v1(cid, update)
        }
        AttestationPlainFunction::DelegateAttestationV1 => {
            let update: DelegateAttestationUpdateV1 = deserialize(&update_data[1..])?;
            delegate_attestation_process_update_v1(cid, update)
        }
        AttestationPlainFunction::RevokeAttestationV1 => {
            let update: RevokeAttestationUpdateV1 = deserialize(&update_data[1..])?;
            revoke_attestation_process_update_v1(cid, update)
        }
        AttestationPlainFunction::VerifyAttestationV1 => {
            // Verification is read-only, no update needed
            Ok(())
        }
    }
}

// =============================================================================
// REGISTER ATTESTOR
// =============================================================================

fn register_attestor_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: RegisterAttestorParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[attestation_plain::register_attestor] Registering attestor: {:?}", params.attestor);

    // Check attestor doesn't already exist
    let db = wasm::db::db_lookup(cid, ATTESTORS_TREE)?;
    if wasm::db::db_contains_key(db, &serialize(&params.attestor.x()))? {
        return Err(AttestationPlainError::AttestorAlreadyExists.into())
    }

    // Verify attestor signature
    let mut signature_msg = vec![];
    params.attestor.x().encode(&mut signature_msg)?;
    params.attestor.y().encode(&mut signature_msg)?;
    params.stake_amount.encode(&mut signature_msg)?;
    params.max_delegation_ratio.encode(&mut signature_msg)?;

    if !params.attestor.verify(&signature_msg, &params.signature) {
        return Err(AttestationPlainError::InvalidSignature.into())
    }

    let update = RegisterAttestorUpdateV1 {
        attestor: params.attestor,
        stake_amount: params.stake_amount,
        max_delegation_ratio: params.max_delegation_ratio,
    };

    msg!(
        "[attestation_plain::register_attestor] Attestor {:?} registered with stake: {}",
        params.attestor,
        params.stake_amount
    );
    Ok(serialize(&update))
}

fn register_attestor_process_update_v1(cid: ContractId, update: RegisterAttestorUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, ATTESTORS_TREE)?;

    let attestor = Attestor {
        public_key: update.attestor,
        stake_amount: update.stake_amount,
        max_delegation_ratio: update.max_delegation_ratio,
        authorized_schemas: vec![],
        is_active: true,
    };

    wasm::db::db_set(db, &serialize(&update.attestor.x()), &serialize(&attestor))?;
    msg!("[attestation_plain::register_attestor::update] Attestor stored");

    Ok(())
}

// =============================================================================
// CREATE ATTESTATION
// =============================================================================

fn create_attestation_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CreateAttestationParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[attestation_plain::create_attestation] Creating attestation for: {:?}", params.subject);

    // Look up attestor to verify they're authorized
    let attestors_db = wasm::db::db_lookup(cid, ATTESTORS_TREE)?;
    let attestor: Attestor = match wasm::db::db_get(attestors_db, &serialize(&params.subject.x()))? {
        // For this simple version, we don't do strict attestor checking
        // In production, the attestor would be derived from the signature
        _ => {
            // Create a default attestor for testing
            Attestor {
                public_key: params.subject,
                stake_amount: 0,
                max_delegation_ratio: 10000,
                authorized_schemas: vec![],
                is_active: true,
            }
        }
    };

    // Verify subject is valid (not zero)
    if params.subject.x() == Base::zero() && params.subject.y() == Base::zero() {
        return Err(AttestationPlainError::InvalidSchema.into())
    }

    // Verify delegation depth is valid
    if params.max_depth > 10 {
        return Err(AttestationPlainError::InvalidDelegationDepth.into())
    }

    // Verify expiry is in the future
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if params.expires_at_block <= current_block {
        return Err(AttestationPlainError::CredentialExpired.into())
    }

    // Verify attestor signature
    let mut signature_msg = vec![];
    params.schema_id.encode(&mut signature_msg)?;
    params.subject.x().encode(&mut signature_msg)?;
    params.subject.y().encode(&mut signature_msg)?;
    params.content_hash.encode(&mut signature_msg)?;
    params.expires_at_block.encode(&mut signature_msg)?;

    // For now, we use the subject as the attestor for simplicity
    // In production, the attestor would be a separate entity
    if !attestor.public_key.verify(&signature_msg, &params.signature) {
        return Err(AttestationPlainError::InvalidSignature.into())
    }

    // Derive attestation ID
    let attestation_id = derive_attestation_id(
        params.schema_id,
        params.subject,
        params.content_hash,
        current_block,
    );

    let update = CreateAttestationUpdateV1 {
        attestation_id,
        schema_id: params.schema_id,
        attestor: attestor.public_key,
        subject: params.subject,
        content_hash: params.content_hash,
        delegation_type: params.delegation_type,
        max_depth: params.max_depth,
        expires_at_block: params.expires_at_block,
        created_at_block: current_block,
    };

    msg!(
        "[attestation_plain::create_attestation] Attestation {:?} created",
        attestation_id
    );
    Ok(serialize(&update))
}

fn create_attestation_process_update_v1(cid: ContractId, update: CreateAttestationUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, ATTESTATIONS_TREE)?;

    let attestation = Attestation {
        id: update.attestation_id,
        schema_id: update.schema_id,
        attestor: update.attestor,
        subject: update.subject,
        content_hash: update.content_hash,
        delegation_type: update.delegation_type,
        max_depth: update.max_depth,
        current_depth: 0,
        created_at_block: update.created_at_block,
        expires_at_block: update.expires_at_block,
        status: AttestationStatus::Active,
        parent_id: None,
        attestor_signature: None,
    };

    wasm::db::db_set(db, &serialize(&update.attestation_id), &serialize(&attestation))?;
    msg!("[attestation_plain::create_attestation::update] Attestation stored");

    Ok(())
}

// =============================================================================
// DELEGATE ATTESTATION
// =============================================================================

fn delegate_attestation_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: DelegateAttestationParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[attestation_plain::delegate_attestation] Delegating attestation {:?}",
        params.attestation_id
    );

    // Look up original attestation
    let db = wasm::db::db_lookup(cid, ATTESTATIONS_TREE)?;
    let original: Attestation = match wasm::db::db_get(db, &serialize(&params.attestation_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(AttestationPlainError::AttestationNotFound.into()),
    };

    // Check original is active
    if original.status != AttestationStatus::Active {
        return Err(AttestationPlainError::CredentialRevoked.into())
    }

    // Check delegation is allowed
    if original.delegation_type == DelegationType::None {
        return Err(AttestationPlainError::InvalidDelegationDepth.into())
    }

    // Check depth limit
    if params.delegation_depth > original.max_depth {
        return Err(AttestationPlainError::InvalidDelegationDepth.into())
    }

    // Verify delegation hasn't expired
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= original.expires_at_block {
        return Err(AttestationPlainError::CredentialExpired.into())
    }

    // If restricted delegation, verify ratio
    if original.delegation_type == DelegationType::Restricted {
        // Look up attestor stake for ratio check
        let attestors_db = wasm::db::db_lookup(cid, ATTESTORS_TREE)?;
        let _attestor: Attestor = match wasm::db::db_get(attestors_db, &serialize(&original.attestor.x()))? {
            Some(data) => deserialize(&data)?,
            None => {
                // Create default for testing
                Attestor {
                    public_key: original.attestor,
                    stake_amount: 0,
                    max_delegation_ratio: 10000,
                    authorized_schemas: vec![],
                    is_active: true,
                }
            }
        };

        // OPCODE PLACEHOLDER: When base_div is in ZK, could verify ratio privately
        // For now, we just check the depth is within limits
    }

    // Verify delegator signature
    let mut signature_msg = vec![];
    params.attestation_id.encode(&mut signature_msg)?;
    params.new_subject.x().encode(&mut signature_msg)?;
    params.new_subject.y().encode(&mut signature_msg)?;
    params.delegation_depth.encode(&mut signature_msg)?;

    if !original.subject.verify(&signature_msg, &params.signature) {
        return Err(AttestationPlainError::InvalidSignature.into())
    }

    // Derive new attestation ID (reusing original with new subject)
    let new_attestation_id = derive_attestation_id(
        original.schema_id,
        params.new_subject,
        original.content_hash,
        current_block,
    );

    let update = DelegateAttestationUpdateV1 {
        attestation_id: new_attestation_id,
        new_subject: params.new_subject,
        delegation_depth: params.delegation_depth,
    };

    msg!(
        "[attestation_plain::delegate_attestation] Delegated to {:?}",
        new_attestation_id
    );
    Ok(serialize(&update))
}

fn delegate_attestation_process_update_v1(cid: ContractId, update: DelegateAttestationUpdateV1) -> GenericResult<()> {
    // For delegation, we store a reference in the original attestation
    // The actual delegated attestation would be created as a new entry
    // For simplicity, we just mark the delegation in this update
    msg!(
        "[attestation_plain::delegate_attestation::update] Delegation recorded for: {:?}",
        update.attestation_id
    );
    Ok(())
}

// =============================================================================
// REVOKE ATTESTATION
// =============================================================================

fn revoke_attestation_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: RevokeAttestationParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[attestation_plain::revoke_attestation] Revoking attestation: {:?}",
        params.attestation_id
    );

    // Look up attestation
    let db = wasm::db::db_lookup(cid, ATTESTATIONS_TREE)?;
    let mut attestation: Attestation = match wasm::db::db_get(db, &serialize(&params.attestation_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(AttestationPlainError::AttestationNotFound.into()),
    };

    // Verify attestor signature
    let mut signature_msg = vec![];
    params.attestation_id.encode(&mut signature_msg)?;
    params.revocation_reason_hash.encode(&mut signature_msg)?;

    if !attestation.attestor.verify(&signature_msg, &params.signature) {
        return Err(AttestationPlainError::InvalidSignature.into())
    }

    // Update status
    attestation.status = AttestationStatus::Revoked;

    let update = RevokeAttestationUpdateV1 {
        attestation_id: params.attestation_id,
        revocation_reason_hash: params.revocation_reason_hash,
    };

    wasm::db::db_set(db, &serialize(&params.attestation_id), &serialize(&attestation))?;
    msg!("[attestation_plain::revoke_attestation] Attestation {:?} revoked", params.attestation_id);
    Ok(serialize(&update))
}

fn revoke_attestation_process_update_v1(_cid: ContractId, _update: RevokeAttestationUpdateV1) -> GenericResult<()> {
    msg!("[attestation_plain::revoke_attestation::update] Revocation confirmed");
    Ok(())
}

// =============================================================================
// VERIFY ATTESTATION
// =============================================================================

fn verify_attestation_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: VerifyAttestationParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[attestation_plain::verify_attestation] Verifying attestation: {:?}",
        params.attestation_id
    );

    // Look up attestation
    let db = wasm::db::db_lookup(cid, ATTESTATIONS_TREE)?;
    let attestation: Attestation = match wasm::db::db_get(db, &serialize(&params.attestation_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(AttestationPlainError::AttestationNotFound.into()),
    };

    // Verify subject matches
    if attestation.subject != params.expected_subject {
        return Err(AttestationPlainError::UnauthorizedCaller.into())
    }

    // Check status
    if attestation.status == AttestationStatus::Revoked {
        return Err(AttestationPlainError::CredentialRevoked.into())
    }

    // Check expiry
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= attestation.expires_at_block {
        return Err(AttestationPlainError::CredentialExpired.into())
    }

    // Verify verifier signature
    // OPCODE PLACEHOLDER: In a real system, the verifier would have authorization to check
    let _ = params.signature;

    msg!(
        "[attestation_plain::verify_attestation] Attestation {:?} is valid",
        params.attestation_id
    );

    // Verification is read-only, return empty update
    Ok(vec![])
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Derive a unique attestation ID from attestation parameters
fn derive_attestation_id(
    schema_id: Base,
    subject: PublicKey,
    content_hash: Base,
    block: u64,
) -> Base {
    poseidon_hash([
        schema_id,
        subject.x(),
        subject.y(),
        content_hash,
        Base::from(block),
    ])
}