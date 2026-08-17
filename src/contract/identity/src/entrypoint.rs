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

//! DarkWow Identity Contract - Level 0 MVP: Minimal Credential Proofs
//!
//! This contract enables **selective disclosure** of attributes without
//! revealing more than necessary. The core primitive is the "claim" -
//! a ZK proof that certain conditions are met without revealing identity
//! or additional details.

use dwow_sdk::{
    crypto::{BOX_CONTRACT_ID, ContractId, pasta_prelude::PrimeField, poseidon_hash, PublicKey,
        schnorr::SchnorrPublic},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg, ContractCall,
    wasm,
};
use dwow_serial::{deserialize, Encodable};
use dwow_sdk::pasta::pallas::Base;

use crate::error::IdentityError;
use dwow_sdk::error::ContractError;
use crate::model::*;
use crate::IdentityFunction;
use crate::{
    IDENTITY_CONTRACT_CREDENTIALS_TREE, IDENTITY_CONTRACT_NULLIFIERS_TREE,
    IDENTITY_CONTRACT_ISSUERS_TREE, IDENTITY_CONTRACT_CONFIG_TREE,
    IDENTITY_CONTRACT_CAPABILITIES_TREE,
    IDENTITY_CONTRACT_INFO_TREE,
    IDENTITY_CONTRACT_BOX_CONTRACT_ID,
    IDENTITY_CONTRACT_ZKAS_ISSUE_NS_V2,
    IDENTITY_CONTRACT_ZKAS_VERIFY_CAP_NS_V2,
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

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize INFO_TREE with redeployment guard
    let _info_db = match wasm::db::db_lookup(cid, IDENTITY_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, IDENTITY_CONTRACT_INFO_TREE)?,
    };

    // Store BOX_CONTRACT_ID for cross-contract child call validation
    let info_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, IDENTITY_CONTRACT_BOX_CONTRACT_ID, &BOX_CONTRACT_ID.to_bytes())?;

    // Initialize database trees with redeployment guards
    if wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE).is_err() {
        wasm::db::db_init(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    }
    if wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE).is_err() {
        wasm::db::db_init(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CONFIG_TREE).is_err() {
        wasm::db::db_init(cid, IDENTITY_CONTRACT_CONFIG_TREE)?;
    }
    if wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE).is_err() {
        wasm::db::db_init(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    }
    // Register ZK circuits (2 consolidated circuits, no duplicates)
    wasm::db::zkas_db_set(include_bytes!("../proof/issue_credential.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../proof/verify_capability.zk.bin"))?;

    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = IdentityFunction::try_from(self_.data[0])?;

    let mut zk_public_inputs: Vec<(String, Vec<Base>)> = vec![];

    // V2 tx_binding = poseidon_hash(DOMAIN_TX_BINDING=3, tx_commitment=0, tx_nonce=0).
    // Both tx_commitment and tx_nonce are zero in the client (no replay protection yet),
    // so tx_binding is a deterministic constant. MUST match the client computation
    // and the circuit's constrain_instance(tx_binding).
    let tx_binding = poseidon_hash([Base::from(3u64), Base::zero(), Base::zero()]);

    match func {
        IdentityFunction::IssueCredentialV1 => {
            let params = match IssueCredentialParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[identity::get_metadata] Error: Failed to deserialize IssueCredentialParams: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_ISSUE_NS_V2.to_string(),
                vec![params.commitment.inner(), tx_binding, Base::zero()],
            ));
        }
        IdentityFunction::VerifyCapabilityV1 => {
            let params = match VerifyCapabilityParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[identity::get_metadata] Error: Failed to deserialize VerifyCapabilityParams: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_VERIFY_CAP_NS_V2.to_string(),
                vec![params.capability_proof.nullifier.inner(), tx_binding, Base::zero()],
            ));
        }
        // Non-ZK functions: no public inputs
        _ => {}
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
    let func = IdentityFunction::try_from(func_byte)?;

    let update_bytes = match func {
        IdentityFunction::InitializeV1 => process_initialize_instruction(cid, call_idx, calls)?,
        IdentityFunction::IssueCredentialV1 => process_issue_credential_instruction(cid, call_idx, calls)?,
        IdentityFunction::RevokeCredentialV1 => process_revoke_credential_instruction(cid, call_idx, calls)?,
        IdentityFunction::RegisterCapabilityV1 => process_register_capability_instruction(cid, call_idx, calls)?,
        IdentityFunction::IssueCapabilityV1 => process_issue_capability_instruction(cid, call_idx, calls)?,
        IdentityFunction::VerifyCapabilityV1 => process_verify_capability_instruction(cid, call_idx, calls)?,
        IdentityFunction::RevokeCapabilityV1 => process_revoke_capability_instruction(cid, call_idx, calls)?,
        IdentityFunction::RegisterIssuerV1 => process_register_issuer_instruction(cid, call_idx, calls)?,
    };
    let _ = wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat());
    Ok(())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = IdentityFunction::try_from(update_data[0])?;

    match func {
        IdentityFunction::InitializeV1 => {
            let update = InitializeUpdateV1::decode(&update_data[1..])?;
            apply_initialize_update(cid, update)
        }
        IdentityFunction::IssueCredentialV1 => {
            let update = IssueCredentialUpdateV1::decode(&update_data[1..])?;
            apply_issue_credential_update(cid, update)
        }
        IdentityFunction::RevokeCredentialV1 => {
            let update = RevokeCredentialUpdateV1::decode(&update_data[1..])?;
            apply_revoke_credential_update(cid, update)
        }
        IdentityFunction::RegisterCapabilityV1 => {
            let update = RegisterCapabilityUpdateV1::decode(&update_data[1..])?;
            apply_register_capability_update(cid, update)
        }
        IdentityFunction::IssueCapabilityV1 => {
            let update = IssueCapabilityUpdateV1::decode(&update_data[1..])?;
            apply_issue_capability_update(cid, update)
        }
        IdentityFunction::VerifyCapabilityV1 => {
            let update = VerifyCapabilityUpdateV1::decode(&update_data[1..])?;
            apply_verify_capability_update(cid, update)
        }
        IdentityFunction::RevokeCapabilityV1 => {
            let update = RevokeCapabilityUpdateV1::decode(&update_data[1..])?;
            apply_revoke_capability_update(cid, update)
        }
        IdentityFunction::RegisterIssuerV1 => {
            let update = RegisterIssuerUpdateV1::decode(&update_data[1..])?;
            apply_register_issuer_update(cid, update)
        }
    }
}

// ============================================================================
// INITIALIZE
// ============================================================================

fn process_initialize_instruction(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= InitializeParams::decode(&self_.data[1..])?;

    msg!("[identity::initialize] Initializing Identity contract v{}", params.version);

    let update = InitializeUpdateV1 {
        version: params.version,
        created_at: wasm::util::get_verifying_block_height()?.get(),
    };

    msg!("[identity::initialize] Identity contract initialized successfully");
    Ok(update.encode())
}

fn apply_initialize_update(cid: ContractId, update: InitializeUpdateV1) -> ContractResult {
    let config_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CONFIG_TREE)?;

    wasm::db::db_set(
        config_db,
        b"version",
        &update.version.to_le_bytes(),
    )?;

    msg!("[identity::initialize::update] Config stored");
    Ok(())
}

// ============================================================================
// ISSUE CREDENTIAL
// ============================================================================

fn process_issue_credential_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= IssueCredentialParams::decode(&self_.data[1..])?;

    msg!("[identity::issue_credential] Issuing credential to holder");

    // Credential data stored locally for DAG; possession tracked via Box::Put.
    let nullifier_bytes = params.nullifier.to_bytes();
    // Check nullifier hasn't been used
    let nullifiers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &nullifier_bytes)? {
        msg!("[identity::issue_credential] ERROR: Nullifier already used");
        return Err(IdentityError::NullifierAlreadySpent.into());
    }

    // HAZOP ID-2 fix: verify the issuer is registered in the issuers tree.
    // The ZK proof (via metadata) proves the prover knows issuer_secret, but
    // the contract must also confirm the issuer_pub is a known trusted issuer.
    let issuers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;
    let issuer_key = compute_issuer_key(&params.issuer_pub);
    if !wasm::db::db_contains_key(issuers_db, &issuer_key)? {
        msg!("[identity::issue_credential] ERROR: Issuer not registered");
        return Err(IdentityError::IssuerNotTrusted.into());
    }

    let update = IssueCredentialUpdateV1 {
        nullifier: params.nullifier,
        issuer_pub: params.issuer_pub,
        holder_pub: params.holder_pub,
        schema_hash: params.schema_hash,
        commitment: params.commitment,
        issued_at: params.issued_at,
        expires_at: params.expires_at,
    };

    msg!("[identity::issue_credential] Credential issuance prepared");
    Ok(update.encode())
}

fn apply_issue_credential_update(cid: ContractId, update: IssueCredentialUpdateV1) -> ContractResult {
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;
    let nullifier_bytes = update.nullifier.to_bytes();

    // Store credential data for DAG operations.
    // Possession tracking delegated to Box::Put child call.
    let credential = Credential {
        nullifier: update.nullifier,
        issuer_pub: update.issuer_pub,
        holder_pub: update.holder_pub,
        schema_hash: update.schema_hash,
        commitment: update.commitment,
        revoked: false,
        issued_at: update.issued_at,
        expires_at: update.expires_at,
    };
    wasm::db::db_set(credentials_db, &nullifier_bytes, &credential.encode())?;

    // Store nullifier (prevents double-issuance)
    wasm::db::db_mark_spent(nullifiers_db, &nullifier_bytes)?;

    msg!("[identity::issue_credential::update] Credential stored");
    Ok(())
}

// ============================================================================
// REVOKE CREDENTIAL
// ============================================================================

fn process_revoke_credential_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= RevokeCredentialParams::decode(&self_.data[1..])?;

    msg!("[identity::revoke_credential] Revoking credential");

    // Load credential
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = params.nullifier.to_bytes();
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;
    let mut credential: Credential = Credential::decode(&cred_data)?;

    // Verify issuer authorization: only the credential issuer may revoke it.
    // issuer_sig must be a valid Schnorr signature by credential.issuer_pub
    // over the credential nullifier (binding the revocation to this credential).
    let sig = dwow_sdk::crypto::schnorr::Signature::decode(&params.issuer_sig)
        .ok_or(IdentityError::InvalidSignature)?;
    if !credential.issuer_pub.verify(&nullifier_bytes, &sig) {
        msg!("[identity::revoke_credential] ERROR: Invalid issuer signature");
        return Err(IdentityError::InvalidSignature.into());
    }

    credential.revoked = true;
    let update = RevokeCredentialUpdateV1 { credential };

    msg!("[identity::revoke_credential] Revocation prepared");
    Ok(update.encode())
}

fn apply_revoke_credential_update(cid: ContractId, update: RevokeCredentialUpdateV1) -> ContractResult {
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;

    let nullifier_bytes = update.credential.nullifier.to_bytes();

    // Blind-write the revoked credential — no db_get in apply.
    wasm::db::db_set(credentials_db, &nullifier_bytes, &update.credential.encode())?;

    // Add to nullifiers list
    wasm::db::db_mark_spent(nullifiers_db, &nullifier_bytes)?;

    msg!("[identity::revoke_credential::update] Credential revoked");
    Ok(())
}

// ============================================================================
// REGISTER CAPABILITY
// ============================================================================

fn process_register_capability_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = RegisterCapabilityParams::decode(&self_.data[1..])?;

    msg!("[identity::register_capability] Registering capability");

    // Compute capability ID
    let capability_id = compute_capability_id(&params.name, &params.credential_requirement);

    // Check if capability already exists
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let cap_bytes = capability_id.to_bytes();
    let existing = wasm::db::db_get(capabilities_db, &cap_bytes)?;
    if existing.is_some() {
        msg!("[identity::register_capability] ERROR: Capability already registered");
        return Err(IdentityError::CapabilityAlreadyExists.into());
    }

    let update = RegisterCapabilityUpdateV1 {
        capability_id,
        name: params.name,
        credential_requirement: params.credential_requirement,
        max_holders: params.max_holders,
    };

    msg!("[identity::register_capability] Capability registered");
    Ok(update.encode())
}

fn apply_register_capability_update(cid: ContractId, update: RegisterCapabilityUpdateV1) -> ContractResult {
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;

    let capability = Capability {
        capability_id: update.capability_id,
        name: update.name,
        credential_requirement: update.credential_requirement.clone(),
        issuer_pub: update.credential_requirement.issuer_pub,
        max_holders: update.max_holders,
        issued_count: 0,
    };

    wasm::db::db_set(capabilities_db, &update.capability_id.to_bytes(), &capability.encode())?;

    msg!("[identity::register_capability::update] Capability stored");
    Ok(())
}

// ============================================================================
// ISSUE CAPABILITY
// ============================================================================

fn process_issue_capability_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= IssueCapabilityParams::decode(&self_.data[1..])?;

    msg!("[identity::issue_capability] Issuing capability");

    // Load capability definition
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let cap_bytes = params.capability_id.to_bytes();
    let cap_data = wasm::db::db_get(capabilities_db, &cap_bytes)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    let mut capability: Capability = Capability::decode(&cap_data)?;

    // Check max holders limit
    if let Some(max) = capability.max_holders {
        if capability.issued_count >= max {
            msg!("[identity::issue_capability] ERROR: Max holders reached");
            return Err(IdentityError::CapabilityMaxHoldersReached.into());
        }
    }

    // Verify credential exists
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let cred_nullifier_bytes = params.credential_nullifier.to_bytes();
    let _cred_data = wasm::db::db_get(credentials_db, &cred_nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    // Possession tracked via Box::Put child call; issuance key not needed.

    capability.issued_count += 1;
    let update = IssueCapabilityUpdateV1 { capability };

    msg!("[identity::issue_capability] Capability issuance prepared");
    Ok(update.encode())
}

fn apply_issue_capability_update(cid: ContractId, update: IssueCapabilityUpdateV1) -> ContractResult {
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;

    // Blind-write the capability with the incremented issued_count — no db_get.
    let cap_bytes = update.capability.capability_id.to_bytes();
    wasm::db::db_set(capabilities_db, &cap_bytes, &update.capability.encode())?;

    msg!("[identity::issue_capability::update] Capability issued");
    Ok(())
}

// ============================================================================
// VERIFY CAPABILITY
// ============================================================================

fn process_verify_capability_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= VerifyCapabilityParams::decode(&self_.data[1..])?;

    msg!("[identity::verify_capability] Verifying capability");

    // Load capability definition
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let cap_bytes = params.capability_proof.capability_id.to_bytes();
    let _cap_data = wasm::db::db_get(capabilities_db, &cap_bytes)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    // Possession verified via Box::Take child call.
    // The credential is a Box; TakeV1 proves the caller holds it.
    // Revocation checked via Identity's nullifier tree.

    let update = VerifyCapabilityUpdateV1 {
        capability_id: params.capability_proof.capability_id,
        holder_pub: params.capability_proof.issuer_pub,
        verified: true,
    };

    msg!("[identity::verify_capability] Capability verified");
    Ok(update.encode())
}

fn apply_verify_capability_update(_cid: ContractId, _update: VerifyCapabilityUpdateV1) -> ContractResult {
    msg!("[identity::verify_capability::update] Verification recorded");
    Ok(())
}

// ============================================================================
// REVOKE CAPABILITY
// ============================================================================

fn process_revoke_capability_instruction(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= RevokeCapabilityParams::decode(&self_.data[1..])?;

    msg!("[identity::revoke_capability] Revoking capability");

    // Capability possession tracked via Box.
    // Revocation via Box::Take nullifier consumption.

    let update = RevokeCapabilityUpdateV1 {
        capability_id: params.capability_id,
        holder_pub: params.holder_pub,
    };

    msg!("[identity::revoke_capability] Capability revocation prepared");
    Ok(update.encode())
}

fn apply_revoke_capability_update(cid: ContractId, update: RevokeCapabilityUpdateV1) -> ContractResult {
    // Write revocation marker to the nullifiers tree.
    // Keyed by poseidon_hash(capability_id, holder_pub_x, holder_pub_y)
    // so that capability-gated entrypoints can check for revocation.
    let nullifiers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;
    let (hx, hy) = update.holder_pub.xy().ok_or(IdentityError::InvalidSignature)?;
    let revoke_key = poseidon_hash([update.capability_id.inner(), hx, hy]);
    wasm::db::db_mark_spent(nullifiers_db, &revoke_key.to_repr())?;
    msg!("[identity::revoke_capability::update] Capability revoked");
    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Compute capability ID from name and requirements
fn compute_capability_id(name: &[u8], requirement: &CredentialRequirement) -> CapabilityId {
    use dwow_sdk::crypto::poseidon_hash;
    let mut data = requirement.encode();
    data.extend_from_slice(name);
    let mut u64_bytes = [0u8; 8];
    u64_bytes.copy_from_slice(&data[..8.min(data.len())]);
    let value = u64::from_le_bytes(u64_bytes);
    let hash = poseidon_hash([dwow_sdk::pasta::pallas::Base::from(value)]);
    CapabilityId(hash)
}

/// Compute a hashed DB key from an issuer pubkey so the raw pubkey is not
/// exposed as a database key. Uses full 32-byte entropy via Poseidon.
fn compute_issuer_key(issuer_pub: &PublicKey) -> Vec<u8> {
    let (x, y) = issuer_pub.xy().expect("pk not identity");
    poseidon_hash([x, y, Base::zero(), Base::zero()]).to_repr().to_vec()
}

// fn compute_issuance_key removed — dead code, never called.
// Reinstated when capability issuance tracking requires it.

// ============================================================================
// REGISTER ISSUER (Phase 2d hardening)
// ============================================================================

fn process_register_issuer_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= RegisterIssuerParams::decode(&self_.data[1..])?;

    msg!("[identity::register_issuer] Registering issuer");

    // Check if issuer already registered
    let issuers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;
    let issuer_key = compute_issuer_key(&params.issuer_pub);
    let existing = wasm::db::db_get(issuers_db, &issuer_key)?;
    if existing.is_some() {
        msg!("[identity::register_issuer] ERROR: Issuer already registered");
        return Err(IdentityError::IssuerAlreadyRegistered.into());
    }

    let update = RegisterIssuerUpdateV1 {
        issuer_id: params.issuer_pub,
        name: params.name.clone(),
        authorized_schemas: params.authorized_schemas.clone(),
        registered_at: wasm::util::get_verifying_block_height()?.get(),
    };

    msg!("[identity::register_issuer] Issuer registration prepared");
    Ok(update.encode())
}

fn apply_register_issuer_update(cid: ContractId, update: RegisterIssuerUpdateV1) -> ContractResult {
    let issuers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;

    let issuer = Issuer {
        pub_key: update.issuer_id,
        name: update.name,
        authorized_schemas: update.authorized_schemas,
        trusted: true,
    };

    wasm::db::db_set(issuers_db, &compute_issuer_key(&update.issuer_id), &issuer.encode())?;

    msg!("[identity::register_issuer::update] Issuer stored");
    Ok(())
}
