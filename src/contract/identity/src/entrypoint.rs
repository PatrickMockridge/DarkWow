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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let func = IdentityFunction::try_from(*self_.data.first().ok_or_else(|| {
        ContractError::IoError("empty call data: no selector byte".to_string())
    })?)?;

    let mut zk_public_inputs: Vec<(String, Vec<Base>)> = vec![];

    // V2 tx_binding = poseidon_hash(DOMAIN_TX_BINDING=3, tx_commitment=0, tx_nonce=0).
    // Both tx_commitment and tx_nonce are zero in the client (no replay protection yet),
    // so tx_binding is a deterministic constant. MUST match the client computation
    // and the circuit's constrain_instance(tx_binding).
    let tx_binding = poseidon_hash([Base::from(3u64), Base::zero(), Base::zero()]);

    match func {
        IdentityFunction::IssueCredentialV1 => {
            let params = match IssueCredentialParams::decode(payload) {
                Ok(p) => p, Err(e) => { msg!("[identity::get_metadata] Error: Failed to deserialize IssueCredentialParams: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_ISSUE_NS_V2.to_string(),
                vec![params.commitment.inner(), tx_binding, Base::zero()],
            ));
        }
        IdentityFunction::VerifyCapabilityV1 => {
            let params = match VerifyCapabilityParams::decode(payload) {
                Ok(p) => p, Err(e) => { msg!("[identity::get_metadata] Error: Failed to deserialize VerifyCapabilityParams: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            // The circuit's instance order, and every value here is one the *proof* binds: exposing
            // only the nullifier (as this did) left the schema, the issuer, the threshold and the
            // predicate as instruction data the proof said nothing about, which is what
            // `process_verify_capability_instruction` now compares against the capability record.
            let proof = &params.capability_proof;
            let (issuer_x, issuer_y) = match proof.issuer_pub.xy() {
                Some(coords) => coords,
                None => {
                    msg!("[identity::get_metadata] Error: capability_proof.issuer_pub is the identity point");
                    let _ = wasm::util::set_return_data(&vec![]);
                    return Ok(())
                }
            };
            let schema_hash = match Option::<Base>::from(Base::from_repr(proof.schema_hash)) {
                Some(h) => h,
                None => {
                    msg!("[identity::get_metadata] Error: capability_proof.schema_hash is not a canonical field element");
                    let _ = wasm::util::set_return_data(&vec![]);
                    return Ok(())
                }
            };
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_VERIFY_CAP_NS_V2.to_string(),
                vec![
                    proof.nullifier.inner(),
                    schema_hash,
                    issuer_x,
                    issuer_y,
                    Base::from(proof.threshold),
                    Base::from(proof.predicate_result as u64),
                    proof.commitment.inner(),
                    tx_binding,
                    Base::zero(),
                ],
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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let func_byte = *self_.data.first().ok_or_else(|| {
        ContractError::IoError("empty call data: no selector byte".to_string())
    })?;
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
    let _ = wasm::util::set_return_data(&[&[func_byte][..], update_bytes.as_slice()].concat());
    Ok(())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let update_func = *update_data.first().ok_or_else(|| {
        ContractError::IoError("empty update data: no selector byte".to_string())
    })?;
    let update_payload = update_data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty update data: no payload after selector".to_string())
    })?;
    let func = IdentityFunction::try_from(update_func)?;

    match func {
        IdentityFunction::InitializeV1 => {
            let update = InitializeUpdateV1::decode(update_payload)?;
            apply_initialize_update(cid, update)
        }
        IdentityFunction::IssueCredentialV1 => {
            let update = IssueCredentialUpdateV1::decode(update_payload)?;
            apply_issue_credential_update(cid, update)
        }
        IdentityFunction::RevokeCredentialV1 => {
            let update = RevokeCredentialUpdateV1::decode(update_payload)?;
            apply_revoke_credential_update(cid, update)
        }
        IdentityFunction::RegisterCapabilityV1 => {
            let update = RegisterCapabilityUpdateV1::decode(update_payload)?;
            apply_register_capability_update(cid, update)
        }
        IdentityFunction::IssueCapabilityV1 => {
            let update = IssueCapabilityUpdateV1::decode(update_payload)?;
            apply_issue_capability_update(cid, update)
        }
        IdentityFunction::VerifyCapabilityV1 => {
            let update = VerifyCapabilityUpdateV1::decode(update_payload)?;
            apply_verify_capability_update(cid, update)
        }
        IdentityFunction::RevokeCapabilityV1 => {
            let update = RevokeCapabilityUpdateV1::decode(update_payload)?;
            apply_revoke_capability_update(cid, update)
        }
        IdentityFunction::RegisterIssuerV1 => {
            let update = RegisterIssuerUpdateV1::decode(update_payload)?;
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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params = InitializeParams::decode(payload)?;

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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params = IssueCredentialParams::decode(payload)?;

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
    let issuer_key = compute_issuer_key(&params.issuer_pub)?;
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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params = RevokeCredentialParams::decode(payload)?;

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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params  = RegisterCapabilityParams::decode(payload)?;

    msg!("[identity::register_capability] Registering capability");

    // Compute capability ID
    let capability_id = compute_capability_id(&params.name, &params.credential_requirement)?;

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
    Ok(update.encode()?)
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

    wasm::db::db_set(capabilities_db, &update.capability_id.to_bytes(), &capability.encode()?)?;

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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params = IssueCapabilityParams::decode(payload)?;

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
    Ok(update.encode()?)
}

fn apply_issue_capability_update(cid: ContractId, update: IssueCapabilityUpdateV1) -> ContractResult {
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;

    // Blind-write the capability with the incremented issued_count — no db_get.
    let cap_bytes = update.capability.capability_id.to_bytes();
    wasm::db::db_set(capabilities_db, &cap_bytes, &update.capability.encode()?)?;

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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params = VerifyCapabilityParams::decode(payload)?;

    msg!("[identity::verify_capability] Verifying capability");

    // Load the capability definition — and *use* it. This record says what a valid credential must
    // be (`CredentialRequirement`), and until 2026-09-21 the function loaded it and discarded it
    // (`let _cap_data = ...`), so none of the checks below existed: any schema, any issuer and a
    // threshold of zero were all acceptable, and the proof was over whatever credential the caller
    // held. OBL-Z17 in `doc/src/arch/verification-hazop.md`.
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let cap_bytes = params.capability_proof.capability_id.to_bytes();
    let cap_data = wasm::db::db_get(capabilities_db, &cap_bytes)?
        .ok_or(IdentityError::CapabilityNotFound)?;
    let capability = Capability::decode(&cap_data)?;
    let requirement = &capability.credential_requirement;

    // 1. The credential must exist, and be live. Credentials are stored keyed by their *nullifier*
    //    (`apply_issue_credential_update`), and the issuance path marks that same nullifier in the
    //    identity nullifiers tree — so membership there means "issued", not "consumed", and reading
    //    it as revocation would reject every legitimate credential while passing a fabricated one.
    //    Revocation is the record's own `revoked` flag, and expiry is its `expires_at`.
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let cred_bytes = params.capability_proof.nullifier.to_bytes();
    let cred_data = wasm::db::db_get(credentials_db, &cred_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;
    let credential = Credential::decode(&cred_data)?;

    if credential.revoked {
        msg!("[identity::verify_capability] Error: credential is revoked");
        return Err(IdentityError::CredentialRevoked.into())
    }
    let now = wasm::util::get_verifying_block_height()?.get();
    if credential.expires_at != 0 && now > credential.expires_at {
        msg!("[identity::verify_capability] Error: credential expired at {} (now {})",
             credential.expires_at, now);
        return Err(IdentityError::CredentialExpired.into())
    }

    // 2. The credential's own record must satisfy the capability — the *stored* schema and issuer,
    //    not the caller's copies of them. Both claims are public inputs of the proof as well, so the
    //    comparison is two-sided: the record against the requirement, and the proof against the
    //    record.
    if credential.schema_hash != requirement.schema_hash
        || params.capability_proof.schema_hash != requirement.schema_hash
    {
        msg!("[identity::verify_capability] Error: credential schema does not match the capability's requirement");
        return Err(IdentityError::SchemaNotRecognized.into())
    }

    if credential.issuer_pub != requirement.issuer_pub
        || params.capability_proof.issuer_pub != requirement.issuer_pub
    {
        msg!("[identity::verify_capability] Error: credential issuer is not the capability's trusted issuer");
        return Err(IdentityError::IssuerNotTrusted.into())
    }

    // 3. The proof must be *about this credential*. `commitment` is a public input the circuit
    //    reconstructs from the credential's preimage (`issuer`, `holder`, `schema`, both attributes,
    //    the attribute blind, the credential secret and the validity window), so requiring it to
    //    equal the stored record's commitment is what ties everything above to a credential the
    //    issuer actually signed. Without it the schema, issuer and threshold are compared against the
    //    caller's *claims* and a caller holding no credential at all can satisfy all of them.
    if params.capability_proof.commitment.inner() != credential.commitment.inner() {
        msg!("[identity::verify_capability] Error: the proof's commitment is not the stored credential's");
        return Err(IdentityError::AttributeMismatch.into())
    }

    // 4. The predicate must actually hold — a public input, so this is the proof's value.
    if params.capability_proof.predicate_result != 1 {
        msg!("[identity::verify_capability] Error: predicate not satisfied");
        return Err(IdentityError::PredicateFailed.into())
    }

    // 5. The threshold the predicate was evaluated at must be at least the capability's floor. The
    //    circuit proves `attribute_value >= threshold` and cannot see this record, so a caller free
    //    to choose `threshold` could pass `0` and satisfy any requirement.
    if params.capability_proof.threshold < requirement.min_threshold {
        msg!("[identity::verify_capability] Error: threshold {} is below the capability's minimum {}",
             params.capability_proof.threshold, requirement.min_threshold);
        return Err(IdentityError::PredicateFailed.into())
    }

    // Verification is a *read*: it consumes nothing and records no state, which is why the update
    // below carries no nullifier and the apply phase writes nothing. An earlier revision of this
    // function wrote the credential's nullifier "to make the check mean something" — but that
    // nullifier is already spent at issuance, so the write was a no-op and the check it was meant to
    // support was backwards.
    //
    // STILL OPEN, and it is the last piece: *possession*. The credential is a box, and
    // `IDENTITY_CONTRACT_BOX_CONTRACT_ID` is stored for precisely the `Box::Take` child call that
    // proves the caller holds it — nothing requires that call, so a caller who can produce a valid
    // credential's preimage (the issuer's and holder's keys, the schema, both attributes and the
    // blind) still passes without holding the box. Everything else — that the credential exists, is
    // live, is for this capability's schema and issuer, and that the predicate held over an attribute
    // it actually committed to — is checked above. Remaining work of OBL-Z17.

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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params = RevokeCapabilityParams::decode(payload)?;

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
fn compute_capability_id(
    name: &[u8],
    requirement: &CredentialRequirement,
) -> Result<CapabilityId, ContractError> {
    use dwow_sdk::crypto::poseidon_hash;
    let mut data = requirement.encode()?;
    data.extend_from_slice(name);
    // `8.min(data.len())` used to size this slice, and `copy_from_slice` panics when the lengths
    // differ — so a buffer shorter than 8 bytes was a panic rather than a rejection. `get(..8)`
    // makes the short case an error and the copy below is then exact by construction.
    let Some(head) = data.get(..8) else {
        return Err(ContractError::IoError(format!(
            "compute_capability_id: need 8 bytes, got {}", data.len()
        )))
    };
    let mut u64_bytes = [0u8; 8];
    u64_bytes.copy_from_slice(head);
    let value = u64::from_le_bytes(u64_bytes);
    let hash = poseidon_hash([dwow_sdk::pasta::pallas::Base::from(value)]);
    Ok(CapabilityId(hash))
}

/// Compute a hashed DB key from an issuer pubkey so the raw pubkey is not
/// exposed as a database key. Uses full 32-byte entropy via Poseidon.
fn compute_issuer_key(issuer_pub: &PublicKey) -> Result<Vec<u8>, ContractError> {
    // Typed rather than panicking: two of the three callers pass `params.issuer_pub`, which arrives
    // from `::decode`, and the derived `Decodable` for `PublicKey` builds the point directly — so a
    // decoded key can be the identity even though the constructor would have rejected it.
    let Some((x, y)) = issuer_pub.xy() else {
        return Err(ContractError::IoError(
            "compute_issuer_key: public key is the identity point".to_string(),
        ))
    };
    Ok(poseidon_hash([x, y, Base::zero(), Base::zero()]).to_repr().to_vec())
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
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let params = RegisterIssuerParams::decode(payload)?;

    msg!("[identity::register_issuer] Registering issuer");

    // Check if issuer already registered
    let issuers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;
    let issuer_key = compute_issuer_key(&params.issuer_pub)?;
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
    Ok(update.encode()?)
}

fn apply_register_issuer_update(cid: ContractId, update: RegisterIssuerUpdateV1) -> ContractResult {
    let issuers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;

    let issuer = Issuer {
        pub_key: update.issuer_id,
        name: update.name,
        authorized_schemas: update.authorized_schemas,
        trusted: true,
    };

    wasm::db::db_set(issuers_db, &compute_issuer_key(&update.issuer_id)?, &issuer.encode()?)?;

    msg!("[identity::register_issuer::update] Issuer stored");
    Ok(())
}
