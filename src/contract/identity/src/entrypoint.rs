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
    crypto::{ContractId, pasta_prelude::PrimeField, poseidon_hash},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg, ContractCall,
    wasm,
};
use dwow_serial::{deserialize, serialize, Encodable};
use dwow_sdk::pasta::pallas::Base;

use crate::error::IdentityError;
use crate::model::*;
use crate::IdentityFunction;
use crate::{
    IDENTITY_CONTRACT_CREDENTIALS_TREE, IDENTITY_CONTRACT_NULLIFIERS_TREE,
    IDENTITY_CONTRACT_ISSUERS_TREE, IDENTITY_CONTRACT_CONFIG_TREE,
    IDENTITY_CONTRACT_CAPABILITIES_TREE, IDENTITY_CONTRACT_CAPABILITY_ISSUANCES_TREE,
    IDENTITY_CONTRACT_REPUTATIONS_TREE, IDENTITY_CONTRACT_INFO_TREE,
    IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1,
    IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_DAG,
    IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_L1,
    IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_L1_V2,
    IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_MULTI,
    IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_RATIO,
    IDENTITY_CONTRACT_ZKAS_ISSUE_NS_V1,
    IDENTITY_CONTRACT_ZKAS_VERIFY_CAP_NS_V1,
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
    if wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITY_ISSUANCES_TREE).is_err() {
        wasm::db::db_init(cid, IDENTITY_CONTRACT_CAPABILITY_ISSUANCES_TREE)?;
    }
    if wasm::db::db_lookup(cid, IDENTITY_CONTRACT_REPUTATIONS_TREE).is_err() {
        wasm::db::db_init(cid, IDENTITY_CONTRACT_REPUTATIONS_TREE)?;
    }

    let create_claim_v1_dag_bincode = include_bytes!("../proof/create_claim_v1_dag.zk.bin");
    wasm::db::zkas_db_set(&create_claim_v1_dag_bincode[..])?;
    let create_claim_v1_l1_v2_bincode = include_bytes!("../proof/create_claim_v1_l1_v2.zk.bin");
    wasm::db::zkas_db_set(&create_claim_v1_l1_v2_bincode[..])?;
    let create_claim_v1_l1_bincode = include_bytes!("../proof/create_claim_v1_l1.zk.bin");
    wasm::db::zkas_db_set(&create_claim_v1_l1_bincode[..])?;
    let create_claim_v1_multi_bincode = include_bytes!("../proof/create_claim_v1_multi.zk.bin");
    wasm::db::zkas_db_set(&create_claim_v1_multi_bincode[..])?;
    let create_claim_v1_ratio_bincode = include_bytes!("../proof/create_claim_v1_ratio.zk.bin");
    wasm::db::zkas_db_set(&create_claim_v1_ratio_bincode[..])?;
    let create_claim_v1_bincode = include_bytes!("../proof/create_claim_v1.zk.bin");
    wasm::db::zkas_db_set(&create_claim_v1_bincode[..])?;
    let issue_credential_v1_bincode = include_bytes!("../proof/issue_credential_v1.zk.bin");
    wasm::db::zkas_db_set(&issue_credential_v1_bincode[..])?;
    let verify_capability_v1_bincode = include_bytes!("../proof/verify_capability_v1.zk.bin");
    wasm::db::zkas_db_set(&verify_capability_v1_bincode[..])?;

    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = IdentityFunction::try_from(self_.data[0])?;

    let mut zk_public_inputs: Vec<(String, Vec<Base>)> = vec![];

    match func {
        IdentityFunction::IssueCredentialV1 => {
            let params: IssueCredentialParams = deserialize(&self_.data[1..])?;
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_ISSUE_NS_V1.to_string(),
                vec![params.commitment.inner()],
            ));
        }
        IdentityFunction::CreateClaimV1 => {
            let params: CreateClaimParams = deserialize(&self_.data[1..])?;
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1.to_string(),
                vec![params.nullifier.inner()],
            ));
        }
        IdentityFunction::CreateClaimV1L1 => {
            let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_L1.to_string(),
                vec![params.nullifier.inner()],
            ));
        }
        IdentityFunction::CreateClaimV1L1V2 => {
            let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_L1_V2.to_string(),
                vec![params.nullifier.inner()],
            ));
        }
        IdentityFunction::CreateClaimV1Multi => {
            let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_MULTI.to_string(),
                vec![params.nullifier.inner()],
            ));
        }
        IdentityFunction::CreateClaimV1Ratio => {
            let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_RATIO.to_string(),
                vec![params.nullifier.inner()],
            ));
        }
        IdentityFunction::CreateClaimDAGV1 => {
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1_DAG.to_string(),
                vec![],
            ));
        }
        IdentityFunction::VerifyCapabilityV1 => {
            let params: VerifyCapabilityParams = deserialize(&self_.data[1..])?;
            zk_public_inputs.push((
                IDENTITY_CONTRACT_ZKAS_VERIFY_CAP_NS_V1.to_string(),
                vec![params.capability_proof.nullifier.inner()],
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
    let func = IdentityFunction::try_from(self_.data[0])?;

    match func {
        IdentityFunction::InitializeV1 => process_initialize_instruction(cid, call_idx, calls),
        IdentityFunction::IssueCredentialV1 => process_issue_credential_instruction(cid, call_idx, calls),
        IdentityFunction::RevokeCredentialV1 => process_revoke_credential_instruction(cid, call_idx, calls),
        IdentityFunction::CreateClaimV1 => process_create_claim_instruction(cid, call_idx, calls),
        IdentityFunction::CreateClaimV1L1 => process_create_claim_l1_instruction(cid, call_idx, calls),
        IdentityFunction::VerifyClaimV1 => process_verify_claim_instruction(cid, call_idx, calls),
        IdentityFunction::CreateClaimV1L1V2 => process_create_claim_l1_v2_instruction(cid, call_idx, calls),
        IdentityFunction::CreateClaimV1Multi => process_create_claim_multi_instruction(cid, call_idx, calls),
        IdentityFunction::CreateClaimV1Ratio => process_create_claim_ratio_instruction(cid, call_idx, calls),
        IdentityFunction::RegisterCapabilityV1 => process_register_capability_instruction(cid, call_idx, calls),
        IdentityFunction::IssueCapabilityV1 => process_issue_capability_instruction(cid, call_idx, calls),
        IdentityFunction::VerifyCapabilityV1 => process_verify_capability_instruction(cid, call_idx, calls),
        IdentityFunction::RevokeCapabilityV1 => process_revoke_capability_instruction(cid, call_idx, calls),
        IdentityFunction::CreateClaimDAGV1 => process_create_claim_dag_instruction(cid, call_idx, calls),
        IdentityFunction::RegisterIssuerV1 => process_register_issuer_instruction(cid, call_idx, calls),
        IdentityFunction::UpdateReputationV1 => process_update_reputation_instruction(cid, call_idx, calls),
    }
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = IdentityFunction::try_from(update_data[0])?;

    match func {
        IdentityFunction::InitializeV1 => {
            let update: InitializeUpdateV1 = deserialize(&update_data[1..])?;
            apply_initialize_update(cid, update)
        }
        IdentityFunction::IssueCredentialV1 => {
            let update: IssueCredentialUpdateV1 = deserialize(&update_data[1..])?;
            apply_issue_credential_update(cid, update)
        }
        IdentityFunction::RevokeCredentialV1 => {
            let update: RevokeCredentialUpdateV1 = deserialize(&update_data[1..])?;
            apply_revoke_credential_update(cid, update)
        }
        IdentityFunction::CreateClaimV1 => {
            let update: CreateClaimUpdateV1 = deserialize(&update_data[1..])?;
            apply_create_claim_update(cid, update)
        }
        IdentityFunction::CreateClaimV1L1 => {
            let update: CreateClaimUpdateV1 = deserialize(&update_data[1..])?;
            apply_create_claim_update(cid, update)
        }
        IdentityFunction::VerifyClaimV1 => {
            let update: VerifyClaimUpdateV1 = deserialize(&update_data[1..])?;
            apply_verify_claim_update(cid, update)
        }
        IdentityFunction::CreateClaimV1L1V2 => {
            let update: CreateClaimUpdateV1 = deserialize(&update_data[1..])?;
            apply_create_claim_update(cid, update)
        }
        IdentityFunction::CreateClaimV1Multi => {
            let update: CreateClaimUpdateV1 = deserialize(&update_data[1..])?;
            apply_create_claim_update(cid, update)
        }
        IdentityFunction::CreateClaimV1Ratio => {
            let update: CreateClaimUpdateV1 = deserialize(&update_data[1..])?;
            apply_create_claim_update(cid, update)
        }
        IdentityFunction::RegisterCapabilityV1 => {
            let update: RegisterCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            apply_register_capability_update(cid, update)
        }
        IdentityFunction::IssueCapabilityV1 => {
            let update: IssueCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            apply_issue_capability_update(cid, update)
        }
        IdentityFunction::VerifyCapabilityV1 => {
            let update: VerifyCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            apply_verify_capability_update(cid, update)
        }
        IdentityFunction::RevokeCapabilityV1 => {
            let update: RevokeCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            apply_revoke_capability_update(cid, update)
        }
        IdentityFunction::CreateClaimDAGV1 => {
            let update: CreateClaimDAGUpdateV1 = deserialize(&update_data[1..])?;
            apply_create_claim_dag_update(cid, update)
        }
        IdentityFunction::RegisterIssuerV1 => {
            let update: RegisterIssuerUpdateV1 = deserialize(&update_data[1..])?;
            apply_register_issuer_update(cid, update)
        }
        IdentityFunction::UpdateReputationV1 => {
            let update: UpdateReputationUpdateV1 = deserialize(&update_data[1..])?;
            apply_update_reputation_update(cid, update)
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: InitializeParams = deserialize(&self_.data[1..])?;

    msg!("[identity::initialize] Initializing Identity contract v{}", params.version);

    let update = InitializeUpdateV1 {
        version: params.version,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[identity::initialize] Identity contract initialized successfully");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_initialize_update(cid: ContractId, update: InitializeUpdateV1) -> ContractResult {
    let config_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CONFIG_TREE)?;

    wasm::db::db_set(
        config_db,
        b"version",
        &serialize(&update.version),
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: IssueCredentialParams = deserialize(&self_.data[1..])?;

    msg!("[identity::issue_credential] Issuing credential to holder");

    // Verify credential doesn't already exist
    let nullifier_bytes = serialize(&params.nullifier);
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let existing = wasm::db::db_get(credentials_db, &nullifier_bytes)?;
    if existing.is_some() {
        msg!("[identity::issue_credential] ERROR: Credential already exists");
        return Err(IdentityError::CredentialAlreadyExists.into());
    }

    // Check nullifier hasn't been used
    let nullifiers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;
    let nullifier_used = wasm::db::db_get(nullifiers_db, &nullifier_bytes)?;
    if nullifier_used.is_some() {
        msg!("[identity::issue_credential] ERROR: Nullifier already used");
        return Err(IdentityError::NullifierAlreadySpent.into());
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
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_issue_credential_update(cid: ContractId, update: IssueCredentialUpdateV1) -> ContractResult {
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;

    let nullifier_bytes = serialize(&update.nullifier);

    // Store credential
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

    wasm::db::db_set(credentials_db, &nullifier_bytes, &serialize(&credential))?;

    // Store nullifier (prevents double-issuance)
    wasm::db::db_set(nullifiers_db, &nullifier_bytes, &[])?;

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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: RevokeCredentialParams = deserialize(&self_.data[1..])?;

    msg!("[identity::revoke_credential] Revoking credential");

    // Load credential (just to verify it exists)
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = serialize(&params.nullifier);
    let _cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let update = RevokeCredentialUpdateV1 {
        nullifier: params.nullifier,
        reason: params.reason,
        revoked: true,
    };

    msg!("[identity::revoke_credential] Revocation prepared");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_revoke_credential_update(cid: ContractId, update: RevokeCredentialUpdateV1) -> ContractResult {
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_NULLIFIERS_TREE)?;

    let nullifier_bytes = serialize(&update.nullifier);

    // Load and update credential
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let mut credential: Credential = deserialize(&cred_data)?;
    credential.revoked = true;

    wasm::db::db_set(credentials_db, &nullifier_bytes, &serialize(&credential))?;

    // Add to nullifiers list
    wasm::db::db_set(nullifiers_db, &nullifier_bytes, &update.reason)?;

    msg!("[identity::revoke_credential::update] Credential revoked");
    Ok(())
}

// ============================================================================
// CREATE CLAIM
// ============================================================================

fn process_create_claim_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CreateClaimParams = deserialize(&self_.data[1..])?;

    msg!("[identity::create_claim] Creating claim");

    // Load and verify credential
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = serialize(&params.nullifier);
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let credential: Credential = deserialize(&cred_data)?;

    if credential.revoked {
        msg!("[identity::create_claim] ERROR: Credential is revoked");
        return Err(IdentityError::CredentialRevoked.into());
    }

    let update = CreateClaimUpdateV1 {
        nullifier: params.nullifier,
        claim_type: params.claim_type,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[identity::create_claim] Claim created");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_create_claim_update(_cid: ContractId, _update: CreateClaimUpdateV1) -> ContractResult {
    // Claims are typically verified off-chain, so no on-chain state update needed
    msg!("[identity::create_claim::update] Claim recorded");
    Ok(())
}

// ============================================================================
// CREATE CLAIM L1 (Selective Disclosure)
// ============================================================================

fn process_create_claim_l1_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;

    msg!("[identity::create_claim_l1] Creating Level 1 claim");

    // Load and verify credential
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = serialize(&params.nullifier);
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let credential: Credential = deserialize(&cred_data)?;

    if credential.revoked {
        msg!("[identity::create_claim_l1] ERROR: Credential is revoked");
        return Err(IdentityError::CredentialRevoked.into());
    }

    let update = CreateClaimUpdateV1 {
        nullifier: params.nullifier,
        claim_type: params.claim_type,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!(
        "[identity::create_claim_l1] Claim created with predicate_result={}",
        params.predicate_result
    );
    wasm::util::set_return_data(&serialize(&update))
}

// ============================================================================
// VERIFY CLAIM
// ============================================================================

fn process_verify_claim_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: VerifyClaimParams = deserialize(&self_.data[1..])?;

    msg!("[identity::verify_claim] Verifying claim");

    // Load and verify credential
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = serialize(&params.claim.nullifier);
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let credential: Credential = deserialize(&cred_data)?;

    if credential.revoked {
        msg!("[identity::verify_claim] ERROR: Credential is revoked");
        return Err(IdentityError::CredentialRevoked.into());
    }

    let update = VerifyClaimUpdateV1 {
        nullifier: params.claim.nullifier,
        verified: true,
    };

    msg!("[identity::verify_claim] Claim verified");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_verify_claim_update(_cid: ContractId, _update: VerifyClaimUpdateV1) -> ContractResult {
    msg!("[identity::verify_claim::update] Verification recorded");
    Ok(())
}

// ============================================================================
// CREATE CLAIM L1 V2
// ============================================================================

fn process_create_claim_l1_v2_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;

    msg!("[identity::create_claim_l1_v2] Creating Level 1 v2 claim");

    // Load and verify credential
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = serialize(&params.nullifier);
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let credential: Credential = deserialize(&cred_data)?;

    if credential.revoked {
        msg!("[identity::create_claim_l1_v2] ERROR: Credential is revoked");
        return Err(IdentityError::CredentialRevoked.into());
    }

    let update = CreateClaimUpdateV1 {
        nullifier: params.nullifier,
        claim_type: params.claim_type,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!(
        "[identity::create_claim_l1_v2] Claim created with predicate_result={}",
        params.predicate_result
    );
    wasm::util::set_return_data(&serialize(&update))
}

// ============================================================================
// CREATE CLAIM MULTI
// ============================================================================

fn process_create_claim_multi_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;

    msg!("[identity::create_claim_multi] Creating multi-credential claim");

    // Load and verify credential
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = serialize(&params.nullifier);
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let _credential: Credential = deserialize(&cred_data)?;

    let update = CreateClaimUpdateV1 {
        nullifier: params.nullifier,
        claim_type: params.claim_type,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[identity::create_claim_multi] Multi-credential claim created");
    wasm::util::set_return_data(&serialize(&update))
}

// ============================================================================
// CREATE CLAIM RATIO
// ============================================================================

fn process_create_claim_ratio_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CreateClaimParamsL1 = deserialize(&self_.data[1..])?;

    msg!("[identity::create_claim_ratio] Creating ratio-based claim");

    // Load and verify credential
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let nullifier_bytes = serialize(&params.nullifier);
    let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    let _credential: Credential = deserialize(&cred_data)?;

    let update = CreateClaimUpdateV1 {
        nullifier: params.nullifier,
        claim_type: params.claim_type,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[identity::create_claim_ratio] Ratio claim created");
    wasm::util::set_return_data(&serialize(&update))
}

// ============================================================================
// REGISTER CAPABILITY
// ============================================================================

fn process_register_capability_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: RegisterCapabilityParams = deserialize(&self_.data[1..])?;

    msg!("[identity::register_capability] Registering capability");

    // Compute capability ID
    let capability_id = compute_capability_id(&params.name, &params.credential_requirement);

    // Check if capability already exists
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let cap_bytes = serialize(&capability_id);
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
    wasm::util::set_return_data(&serialize(&update))
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

    wasm::db::db_set(capabilities_db, &serialize(&update.capability_id), &serialize(&capability))?;

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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: IssueCapabilityParams = deserialize(&self_.data[1..])?;

    msg!("[identity::issue_capability] Issuing capability");

    // Load capability definition
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let cap_bytes = serialize(&params.capability_id);
    let cap_data = wasm::db::db_get(capabilities_db, &cap_bytes)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    let capability: Capability = deserialize(&cap_data)?;

    // Check max holders limit
    if let Some(max) = capability.max_holders {
        if capability.issued_count >= max {
            msg!("[identity::issue_capability] ERROR: Max holders reached");
            return Err(IdentityError::CapabilityMaxHoldersReached.into());
        }
    }

    // Verify credential exists
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    let cred_nullifier_bytes = serialize(&params.credential_nullifier);
    let _cred_data = wasm::db::db_get(credentials_db, &cred_nullifier_bytes)?
        .ok_or(IdentityError::CredentialNotFound)?;

    // Generate capability secret
    let capability_secret = derive_capability_secret(params.holder_pub, params.capability_id);

    // Compute issuance key
    let issuance_key = compute_issuance_key(params.capability_id, params.holder_pub);

    let update = IssueCapabilityUpdateV1 {
        capability_id: params.capability_id,
        holder_pub: params.holder_pub,
        capability_secret,
        expires_at: 0,
        issuance_key,
    };

    msg!("[identity::issue_capability] Capability issuance prepared");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_issue_capability_update(cid: ContractId, update: IssueCapabilityUpdateV1) -> ContractResult {
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let issuances_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITY_ISSUANCES_TREE)?;

    // Store capability issuance record
    let issuance = StoredCapability {
        capability_id: update.capability_id,
        holder_pub: update.holder_pub,
        secret: update.capability_secret,
        revoked: false,
        issued_at: wasm::util::get_verifying_block_height()? as u64,
        expires_at: update.expires_at,
    };

    wasm::db::db_set(issuances_db, &update.issuance_key, &serialize(&issuance))?;

    // Update issued count
    let cap_bytes = serialize(&update.capability_id);
    let cap_data = wasm::db::db_get(capabilities_db, &cap_bytes)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    let mut capability: Capability = deserialize(&cap_data)?;
    capability.issued_count += 1;

    wasm::db::db_set(capabilities_db, &cap_bytes, &serialize(&capability))?;

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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: VerifyCapabilityParams = deserialize(&self_.data[1..])?;

    msg!("[identity::verify_capability] Verifying capability");

    // Load capability definition
    let capabilities_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITIES_TREE)?;
    let cap_bytes = serialize(&params.capability_proof.capability_id);
    let _cap_data = wasm::db::db_get(capabilities_db, &cap_bytes)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    // Load issuance record
    let issuances_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITY_ISSUANCES_TREE)?;
    let issuance_key = compute_issuance_key(
        params.capability_proof.capability_id,
        params.capability_proof.capability_secret,
    );
    let issuance_data = wasm::db::db_get(issuances_db, &issuance_key)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    let issuance: StoredCapability = deserialize(&issuance_data)?;

    if issuance.revoked {
        msg!("[identity::verify_capability] ERROR: Capability is revoked");
        return Err(IdentityError::CapabilityRevoked.into());
    }

    let update = VerifyCapabilityUpdateV1 {
        capability_id: params.capability_proof.capability_id,
        holder_pub: params.capability_proof.issuer_pub,
        verified: true,
    };

    msg!("[identity::verify_capability] Capability verified");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_verify_capability_update(_cid: ContractId, _update: VerifyCapabilityUpdateV1) -> ContractResult {
    msg!("[identity::verify_capability::update] Verification recorded");
    Ok(())
}

// ============================================================================
// REVOKE CAPABILITY
// ============================================================================

fn process_revoke_capability_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: RevokeCapabilityParams = deserialize(&self_.data[1..])?;

    msg!("[identity::revoke_capability] Revoking capability");

    // Load issuance record
    let issuances_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITY_ISSUANCES_TREE)?;
    let issuance_key = compute_issuance_key(params.capability_id, params.holder_pub);
    let issuance_data = wasm::db::db_get(issuances_db, &issuance_key)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    let _issuance: StoredCapability = deserialize(&issuance_data)?;

    let update = RevokeCapabilityUpdateV1 {
        capability_id: params.capability_id,
        holder_pub: params.holder_pub,
        issuance_key,
    };

    msg!("[identity::revoke_capability] Capability revocation prepared");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_revoke_capability_update(cid: ContractId, update: RevokeCapabilityUpdateV1) -> ContractResult {
    let issuances_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CAPABILITY_ISSUANCES_TREE)?;

    // Load and update issuance
    let issuance_data = wasm::db::db_get(issuances_db, &update.issuance_key)?
        .ok_or(IdentityError::CapabilityNotFound)?;

    let mut issuance: StoredCapability = deserialize(&issuance_data)?;
    issuance.revoked = true;

    wasm::db::db_set(issuances_db, &update.issuance_key, &serialize(&issuance))?;

    msg!("[identity::revoke_capability::update] Capability revoked");
    Ok(())
}

// ============================================================================
// CREATE CLAIM DAG
// ============================================================================

fn process_create_claim_dag_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CreateClaimDAGParams = deserialize(&self_.data[1..])?;

    msg!("[identity::create_claim_dag] Creating DAG claim for DAG {:?}", &params.dag_id);

    // Verify all credentials in the path are valid
    let credentials_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_CREDENTIALS_TREE)?;

    assert!(
        params.credentials.len() <= crate::IDENTITY_CONTRACT_MAX_DAG_CREDENTIALS,
        "Too many DAG credentials"
    );
    for dag_cred in &params.credentials {
        let nullifier_bytes = serialize(&dag_cred.nullifier);
        let cred_data = wasm::db::db_get(credentials_db, &nullifier_bytes)?
            .ok_or(IdentityError::CredentialNotFound)?;

        let credential: Credential = deserialize(&cred_data)?;

        if credential.revoked {
            msg!("[identity::create_claim_dag] ERROR: Credential is revoked");
            return Err(IdentityError::CredentialRevoked.into());
        }
    }

    let update = CreateClaimDAGUpdateV1 {
        dag_id: params.dag_id,
        path_index: params.path_index,
        predicate_result: params.predicate_result,
    };

    msg!("[identity::create_claim_dag] DAG claim created");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_create_claim_dag_update(_cid: ContractId, _update: CreateClaimDAGUpdateV1) -> ContractResult {
    msg!("[identity::create_claim_dag::update] DAG claim recorded");
    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Compute capability ID from name and requirements
fn compute_capability_id(name: &[u8], requirement: &CredentialRequirement) -> [u8; 32] {
    use dwow_sdk::crypto::poseidon_hash;
    use dwow_serial::serialize;
    use dwow_sdk::pasta::group::ff::PrimeFieldBits;
    // Serialize the inputs to get deterministic bytes
    let mut data = serialize(requirement);
    data.extend_from_slice(name);
    // Use first 8 bytes as u64 for hashing (simplified for MVP)
    let mut u64_bytes = [0u8; 8];
    u64_bytes.copy_from_slice(&data[..8.min(data.len())]);
    let value = u64::from_le_bytes(u64_bytes);
    let hash = poseidon_hash([dwow_sdk::pasta::pallas::Base::from(value)]);
    // Convert hash to bytes using to_le_bits
    let bits = hash.to_le_bits();
    let mut result = [0u8; 32];
    for (i, bit) in bits.iter().by_vals().take(256).enumerate() {
        if bit {
            result[i / 8] |= 1 << (i % 8);
        }
    }
    result
}

/// Derive capability secret from holder key and capability ID
fn derive_capability_secret(holder_pub: [u8; 32], capability_id: [u8; 32]) -> [u8; 32] {
    use dwow_sdk::crypto::poseidon_hash;
    use dwow_sdk::pasta::group::ff::PrimeFieldBits;
    // Use first 8 bytes of holder_pub as u64
    let mut u64_bytes = [0u8; 8];
    u64_bytes.copy_from_slice(&holder_pub[..8]);
    let holder_value = u64::from_le_bytes(u64_bytes);
    // Use first 8 bytes of capability_id as u64
    let mut cap_bytes = [0u8; 8];
    cap_bytes.copy_from_slice(&capability_id[..8]);
    let cap_value = u64::from_le_bytes(cap_bytes);
    let hash = poseidon_hash([
        dwow_sdk::pasta::pallas::Base::from(holder_value),
        dwow_sdk::pasta::pallas::Base::from(cap_value),
    ]);
    // Convert hash to bytes using to_le_bits
    let bits = hash.to_le_bits();
    let mut result = [0u8; 32];
    for (i, bit) in bits.iter().by_vals().take(256).enumerate() {
        if bit {
            result[i / 8] |= 1 << (i % 8);
        }
    }
    result
}

/// Compute a hashed DB key from an issuer pubkey so the raw pubkey is not
/// exposed as a database key. Uses full 32-byte entropy via Poseidon.
fn compute_issuer_key(issuer_pub: &[u8; 32]) -> Vec<u8> {
    let mut chunks = [0u64; 4];
    for i in 0..4 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&issuer_pub[i * 8..(i + 1) * 8]);
        chunks[i] = u64::from_le_bytes(bytes);
    }
    let hash = poseidon_hash([
        Base::from(chunks[0]),
        Base::from(chunks[1]),
        Base::from(chunks[2]),
        Base::from(chunks[3]),
    ]);
    hash.to_repr().to_vec()
}

/// Compute issuance key from capability ID and holder pub.
/// Hashed so the raw holder pub is not exposed as a DB key.
fn compute_issuance_key(capability_id: [u8; 32], holder_pub: [u8; 32]) -> Vec<u8> {
    let mut h_chunks = [0u64; 4];
    for i in 0..4 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&holder_pub[i * 8..(i + 1) * 8]);
        h_chunks[i] = u64::from_le_bytes(bytes);
    }
    let mut c_bytes = [0u8; 8];
    c_bytes.copy_from_slice(&capability_id[..8]);
    let cid = u64::from_le_bytes(c_bytes);
    poseidon_hash([
        Base::from(cid),
        Base::from(h_chunks[0]),
        Base::from(h_chunks[1]),
        Base::from(h_chunks[2]),
        Base::from(h_chunks[3]),
    ]).to_repr().to_vec()
}

// ============================================================================
// REGISTER ISSUER (Phase 2d hardening)
// ============================================================================

fn process_register_issuer_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: RegisterIssuerParams = deserialize(&self_.data[1..])?;

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
        registered_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[identity::register_issuer] Issuer registration prepared");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_register_issuer_update(cid: ContractId, update: RegisterIssuerUpdateV1) -> ContractResult {
    let issuers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;

    let issuer = Issuer {
        pub_key: update.issuer_id,
        name: update.name,
        authorized_schemas: update.authorized_schemas,
        trusted: true,
    };

    wasm::db::db_set(issuers_db, &compute_issuer_key(&update.issuer_id), &serialize(&issuer))?;

    msg!("[identity::register_issuer::update] Issuer stored");
    Ok(())
}

// ============================================================================
// UPDATE REPUTATION (Phase 2d hardening)
// ============================================================================

/// Compute reputation ID from issuer and relayer pubkeys.
/// Uses full 32-byte pubkeys (4 u64 chunks each) to prevent entropy loss.
fn compute_reputation_id(issuer_pub: &[u8; 32], relayer_pub: &[u8; 32]) -> [u8; 32] {
    let mut ikeys = [0u64; 4];
    let mut rkeys = [0u64; 4];
    for i in 0..4 {
        let mut ib = [0u8; 8];
        let mut rb = [0u8; 8];
        ib.copy_from_slice(&issuer_pub[i * 8..(i + 1) * 8]);
        rb.copy_from_slice(&relayer_pub[i * 8..(i + 1) * 8]);
        ikeys[i] = u64::from_le_bytes(ib);
        rkeys[i] = u64::from_le_bytes(rb);
    }
    let hash = poseidon_hash([
        Base::from(ikeys[0]), Base::from(ikeys[1]),
        Base::from(ikeys[2]), Base::from(ikeys[3]),
        Base::from(rkeys[0]), Base::from(rkeys[1]),
        Base::from(rkeys[2]), Base::from(rkeys[3]),
    ]);
    hash.to_repr()
}

fn process_update_reputation_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: UpdateReputationParams = deserialize(&self_.data[1..])?;

    msg!("[identity::update_reputation] Updating relayer reputation");

    // Verify issuer is registered
    let issuers_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_ISSUERS_TREE)?;
    let issuer_key = compute_issuer_key(&params.issuer_pub);
    wasm::db::db_get(issuers_db, &issuer_key)?
        .ok_or(IdentityError::IssuerNotTrusted)?;

    let reputation_id = compute_reputation_id(&params.issuer_pub, &params.relayer_pub);
    let current_height = wasm::util::get_verifying_block_height()? as u64;

    let update = UpdateReputationUpdateV1 {
        reputation_id,
        relayer_pub: params.relayer_pub,
        issuer_pub: params.issuer_pub,
        slash_count: params.slash_count,
        success_count: params.success_count,
        total_volume: params.total_volume,
        settlement_frequency: params.settlement_frequency,
        last_updated: current_height,
    };

    msg!("[identity::update_reputation] Reputation update prepared");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_update_reputation_update(cid: ContractId, update: UpdateReputationUpdateV1) -> ContractResult {
    let reputations_db = wasm::db::db_lookup(cid, IDENTITY_CONTRACT_REPUTATIONS_TREE)?;

    let record = ReputationRecord {
        relayer_pub: update.relayer_pub,
        issuer_pub: update.issuer_pub,
        slash_count: update.slash_count,
        success_count: update.success_count,
        total_volume: update.total_volume,
        settlement_frequency: update.settlement_frequency,
        last_updated: update.last_updated,
    };

    wasm::db::db_set(reputations_db, &serialize(&update.reputation_id), &serialize(&record))?;

    msg!("[identity::update_reputation::update] Reputation stored");
    Ok(())
}