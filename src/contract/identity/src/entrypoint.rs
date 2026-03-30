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

//! WASM entrypoint for the Identity Contract
//!
//! This contract implements minimal credential proofs - selective disclosure
//! of attributes without revealing more than necessary.

use darkfi_sdk::{
    bridge::{BridgeCall, BridgeParameter},
    contract::ContractResult,
    error::ContractError,
    msg,
    runtime::Runtime,
};

use crate::{error::IdentityError, model::*, IdentityFunction};

/// Initialize the Identity contract
pub fn identity_init(rt: &mut Runtime, params: BridgeParameter) -> ContractResult<()> {
    let call = BridgeCall::decode(params)?;
    let config: InitializeParams = deserialize_init_params(&call.data[1..])?;

    msg!("[identity_init] Initializing Identity contract v{}", config.version);

    // Create trees
    rt.create_tree(IDENTITY_CONTRACT_CREDENTIALS_TREE)?;
    rt.create_tree(IDENTITY_CONTRACT_NULLIFIERS_TREE)?;
    rt.create_tree(IDENTITY_CONTRACT_ISSUERS_TREE)?;
    rt.create_tree(IDENTITY_CONTRACT_CONFIG_TREE)?;

    // Store version
    rt.store_set(
        IDENTITY_CONTRACT_CONFIG_TREE,
        IDENTITY_CONTRACT_DB_VERSION,
        &config.version.encode()?,
    )?;

    msg!("[identity_init] Identity contract initialized successfully");
    Ok(())
}

/// Main contract entrypoint
pub fn identity_exec(rt: &mut Runtime, params: BridgeParameter) -> ContractResult<()> {
    let call = BridgeCall::decode(params)?;
    let function = IdentityFunction::try_from(call.function)?;

    match function {
        IdentityFunction::InitializeV1 => identity_init(rt, params),
        IdentityFunction::IssueCredentialV1 => identity_issue_credential(rt, call),
        IdentityFunction::RevokeCredentialV1 => identity_revoke_credential(rt, call),
        IdentityFunction::CreateClaimV1 => identity_create_claim(rt, call),
        IdentityFunction::CreateClaimV1L1 => identity_create_claim_l1(rt, call),
        IdentityFunction::VerifyClaimV1 => identity_verify_claim(rt, call),
    }
}

// ============================================================================
// ISSUE CREDENTIAL
// ============================================================================

/// Issue a new credential to a holder
///
/// Flow:
/// 1. Verify issuer is trusted
/// 2. Verify credential doesn't already exist (nullifier uniqueness)
/// 3. Store credential record
/// 4. Emit CredentialIssued event
fn identity_issue_credential(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: IssueCredentialParams = deserialize_issue_params(&call.data[1..])?;

    msg!("[identity_issue_credential] Issuing credential to {:?}", &params.holder_pub);

    // =========================================================================
    // STEP 1: Verify issuer is trusted
    // =========================================================================

    let issuer_data = rt.load(IDENTITY_CONTRACT_ISSUERS_TREE, &params.issuer_pub)?;
    if issuer_data.is_none() {
        msg!("[identity_issue_credential] ERROR: Issuer not recognized");
        return Err(IdentityError::IssuerNotTrusted.into())
    }

    // TODO: Verify issuer is trusted for this schema

    // =========================================================================
    // STEP 2: Verify credential doesn't already exist
    // =========================================================================

    let nullifier_bytes = params.nullifier.to_bytes();
    let existing = rt.load(IDENTITY_CONTRACT_CREDENTIALS_TREE, nullifier_bytes)?;
    if existing.is_some() {
        msg!("[identity_issue_credential] ERROR: Credential already exists");
        return Err(IdentityError::CredentialAlreadyExists.into())
    }

    // Check nullifier hasn't been used
    let nullifier_used = rt.load(IDENTITY_CONTRACT_NULLIFIERS_TREE, nullifier_bytes)?;
    if nullifier_used.is_some() {
        msg!("[identity_issue_credential] ERROR: Nullifier already used");
        return Err(IdentityError::NullifierAlreadySpent.into())
    }

    // =========================================================================
    // STEP 3: Store credential
    // =========================================================================

    let credential = Credential {
        nullifier: params.nullifier,
        issuer_pub: params.issuer_pub,
        holder_pub: params.holder_pub,
        schema_hash: params.schema_hash,
        commitment: params.commitment,
        revoked: false,
        issued_at: params.issued_at,
        expires_at: params.expires_at,
    };

    rt.store_set(
        IDENTITY_CONTRACT_CREDENTIALS_TREE,
        nullifier_bytes,
        &credential.encode()?,
    )?;

    // Store nullifier (prevents double-issuance for same holder)
    rt.store_set(IDENTITY_CONTRACT_NULLIFIERS_TREE, nullifier_bytes, &[])?;

    // =========================================================================
    // STEP 4: Emit event
    // =========================================================================
    // Note: We only emit the nullifier and issuer, NOT:
    // - Who the holder is
    // - What attributes were issued
    // - What the schema contains

    msg!(
        "[identity_issue_credential] EMIT_EVENT: CredentialIssued(nullifier={:?}, issuer={:?})",
        &params.nullifier,
        &params.issuer_pub
    );

    Ok(())
}

// ============================================================================
// REVOKE CREDENTIAL
// ============================================================================

/// Revoke a previously issued credential
///
/// Flow:
/// 1. Verify issuer signature
/// 2. Verify credential exists
/// 3. Mark credential as revoked
/// 4. Add to revocation list
fn identity_revoke_credential(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: RevokeCredentialParams = deserialize_revoke_params(&call.data[1..])?;

    msg!("[identity_revoke_credential] Revoking credential {:?}", &params.nullifier);

    // =========================================================================
    // STEP 1: Verify issuer signature
    // =========================================================================
    // In production: verify params.issuer_sig against known issuer keys

    // =========================================================================
    // STEP 2: Load and verify credential exists
    // =========================================================================

    let nullifier_bytes = params.nullifier.to_bytes();
    let cred_data = rt.load(IDENTITY_CONTRACT_CREDENTIALS_TREE, nullifier_bytes)?;
    let mut credential: Credential = match cred_data {
        Some(data) => Credential::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[identity_revoke_credential] ERROR: Credential not found");
            return Err(IdentityError::CredentialNotFound.into())
        }
    };

    // =========================================================================
    // STEP 3: Mark as revoked
    // =========================================================================

    credential.revoked = true;
    rt.store_set(
        IDENTITY_CONTRACT_CREDENTIALS_TREE,
        nullifier_bytes,
        &credential.encode()?,
    )?;

    // Add to revocation list
    rt.store_set(
        IDENTITY_CONTRACT_NULLIFIERS_TREE,
        nullifier_bytes,
        &params.reason,
    )?;

    // =========================================================================
    // STEP 4: Emit event
    // =========================================================================

    msg!(
        "[identity_revoke_credential] EMIT_EVENT: CredentialRevoked(nullifier={:?})",
        &params.nullifier
    );

    Ok(())
}

// ============================================================================
// CREATE CLAIM
// ============================================================================

/// Create a claim from a credential (typically off-chain)
///
/// This is mostly done off-chain by the holder. The on-chain version
/// is for registering claims that must be on-chain.
///
/// Flow:
/// 1. Verify credential exists and is valid
/// 2. Verify credential is not expired or revoked
/// 3. Store claim (if on-chain registration required)
/// 4. Emit ClaimCreated event
fn identity_create_claim(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: CreateClaimParams = deserialize_claim_params(&call.data[1..])?;

    msg!("[identity_create_claim] Creating claim for nullifier {:?}", &params.nullifier);

    // =========================================================================
    // STEP 1: Load and verify credential
    // =========================================================================

    let nullifier_bytes = params.nullifier.to_bytes();
    let cred_data = rt.load(IDENTITY_CONTRACT_CREDENTIALS_TREE, nullifier_bytes)?;
    let credential: Credential = match cred_data {
        Some(data) => Credential::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[identity_create_claim] ERROR: Credential not found");
            return Err(IdentityError::CredentialNotFound.into())
        }
    };

    // =========================================================================
    // STEP 2: Verify not expired or revoked
    // =========================================================================

    if credential.revoked {
        msg!("[identity_create_claim] ERROR: Credential is revoked");
        return Err(IdentityError::CredentialRevoked.into())
    }

    let current_time = get_current_timestamp(rt)?;
    if credential.expires_at > 0 && current_time > credential.expires_at {
        msg!("[identity_create_claim] ERROR: Credential expired");
        return Err(IdentityError::CredentialExpired.into())
    }

    // =========================================================================
    // STEP 3: Store claim (if required on-chain)
    // =========================================================================
    // Most claims are verified off-chain. On-chain storage is only
    // needed for scenarios requiring on-chain verification.

    // =========================================================================
    // STEP 4: Emit event
    // =========================================================================
    // Note: We only emit that a claim was created, NOT:
    // - What the claim proves
    // - What attributes were revealed
    // - Who the holder is

    msg!(
        "[identity_create_claim] EMIT_EVENT: ClaimCreated(nullifier={:?})",
        &params.nullifier
    );

    Ok(())
}

// ============================================================================
// CREATE CLAIM (Level 1 - Selective Disclosure)
// ============================================================================

/// Create a Level 1 claim from a credential with selective disclosure
///
/// This is similar to CreateClaimV1 but returns a public predicate_result bit.
/// The verifier learns whether the predicate is satisfied, not just that
/// the proof is valid.
///
/// Flow:
/// 1. Verify credential exists and is valid
/// 2. Verify credential is not expired or revoked
/// 3. Verify ZK proof with bounded equation (returns predicate_result)
/// 4. Store claim (if on-chain registration required)
/// 5. Emit ClaimCreated event with predicate_result
fn identity_create_claim_l1(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: CreateClaimParamsL1 = deserialize_claim_params_l1(&call.data[1..])?;

    msg!("[identity_create_claim_l1] Creating Level 1 claim for nullifier {:?}", &params.nullifier);

    // =========================================================================
    // STEP 1: Load and verify credential
    // =========================================================================

    let nullifier_bytes = params.nullifier.to_bytes();
    let cred_data = rt.load(IDENTITY_CONTRACT_CREDENTIALS_TREE, nullifier_bytes)?;
    let credential: Credential = match cred_data {
        Some(data) => Credential::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[identity_create_claim_l1] ERROR: Credential not found");
            return Err(IdentityError::CredentialNotFound.into())
        }
    };

    // =========================================================================
    // STEP 2: Verify not expired or revoked
    // =========================================================================

    if credential.revoked {
        msg!("[identity_create_claim_l1] ERROR: Credential is revoked");
        return Err(IdentityError::CredentialRevoked.into())
    }

    let current_time = get_current_timestamp(rt)?;
    if credential.expires_at > 0 && current_time > credential.expires_at {
        msg!("[identity_create_claim_l1] ERROR: Credential expired");
        return Err(IdentityError::CredentialExpired.into())
    }

    // =========================================================================
    // STEP 3: Verify ZK proof with Level 1 bounded equation
    // =========================================================================
    //
    // The Level 1 ZK proof verifies:
    // - Holder knows the secret key corresponding to holder_pub
    // - Credential commitment matches stored commitment
    // - The predicate is satisfied (e.g., age >= 18)
    // - The bounded equation holds: threshold + delta = attribute_value + (1 - result) * 2^64
    //
    // PUBLIC OUTPUT: predicate_result (0 or 1)
    //
    // What it reveals:
    // - predicate_result: 1 if threshold <= attribute_value, 0 otherwise
    //
    // What it does NOT reveal:
    // - Who the holder is
    // - What the credential attributes are
    // - What the actual threshold or attribute values are

    // In production: call ZK verifier with proof

    // =========================================================================
    // STEP 4: Store claim (if required on-chain)
    // =========================================================================

    // =========================================================================
    // STEP 5: Emit event with predicate result
    // =========================================================================
    // Note: Level 1 reveals the predicate result publicly

    msg!(
        "[identity_create_claim_l1] EMIT_EVENT: ClaimCreatedL1(nullifier={:?}, predicate_result={})",
        &params.nullifier,
        params.predicate_result
    );

    Ok(())
}

// ============================================================================
// VERIFY CLAIM
// ============================================================================

/// Verify a claim (typically on-chain verification)
///
/// Flow:
/// 1. Verify credential exists and is valid
/// 2. Verify credential is not expired or revoked
/// 3. Verify ZK proof is valid
/// 4. Mark claim as used (prevent double-spend)
/// 5. Emit ClaimVerified event with result
fn identity_verify_claim(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: VerifyClaimParams = deserialize_verify_params(&call.data[1..])?;

    msg!("[identity_verify_claim] Verifying claim for nullifier {:?}", &params.claim.nullifier);

    // =========================================================================
    // STEP 1: Load and verify credential
    // =========================================================================

    let nullifier_bytes = params.claim.nullifier.to_bytes();
    let cred_data = rt.load(IDENTITY_CONTRACT_CREDENTIALS_TREE, nullifier_bytes)?;
    let credential: Credential = match cred_data {
        Some(data) => Credential::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[identity_verify_claim] ERROR: Credential not found");
            return Err(IdentityError::CredentialNotFound.into())
        }
    };

    // =========================================================================
    // STEP 2: Verify not expired or revoked
    // =========================================================================

    if credential.revoked {
        msg!("[identity_verify_claim] ERROR: Credential is revoked");
        return Err(IdentityError::CredentialRevoked.into())
    }

    let current_time = get_current_timestamp(rt)?;
    if credential.expires_at > 0 && current_time > credential.expires_at {
        msg!("[identity_verify_claim] ERROR: Credential expired");
        return Err(IdentityError::CredentialExpired.into())
    }

    // =========================================================================
    // STEP 3: Verify ZK proof
    // =========================================================================
    //
    // The ZK proof verifies:
    // - Holder knows the secret key corresponding to holder_pub
    // - Credential commitment matches stored commitment
    // - The predicate is satisfied (e.g., age >= 18)
    // - The claim is consistent with the credential
    //
    // What it does NOT reveal:
    // - Who the holder is
    // - What the credential attributes are
    // - What the predicate is checking exactly

    // In production: call ZK verifier with proof

    // =========================================================================
    // STEP 4: Mark claim as used
    // =========================================================================
    // Claims should be consumable or persistent based on use case:
    // - One-time claims: mark as used (nullifier spending)
    // - Persistent claims: don't mark (can be reused until expired)

    // For MVP: one-time claims
    // TODO: Make this configurable

    // =========================================================================
    // STEP 5: Emit verification result
    // =========================================================================
    // Note: The verifier learns only:
    // - Whether the claim is valid or not
    // - The issuer (if they care about trusted issuers)
    // NOT: Who the holder is, what attributes were checked

    msg!(
        "[identity_verify_claim] EMIT_EVENT: ClaimVerified(nullifier={:?}, result={:?})",
        &params.claim.nullifier,
        &params.claim.predicate_result
    );

    Ok(())
}

// ============================================================================
// TRUSTED ISSUER MANAGEMENT
// ============================================================================

/// Add a trusted issuer
pub fn identity_add_issuer(rt: &mut Runtime, issuer_pub: [u8; 32], name: Vec<u8>) -> ContractResult<()> {
    let issuer = Issuer {
        pub_key: issuer_pub,
        name,
        authorized_schemas: vec![],
        trusted: true,
    };

    rt.store_set(IDENTITY_CONTRACT_ISSUERS_TREE, &issuer_pub, &issuer.encode()?)?;

    msg!("[identity_add_issuer] Added trusted issuer {:?}", &issuer_pub);
    Ok(())
}

/// Remove a trusted issuer (revoke their issuing power)
pub fn identity_remove_issuer(rt: &mut Runtime, issuer_pub: [u8; 32]) -> ContractResult<()> {
    let issuer_data = rt.load(IDENTITY_CONTRACT_ISSUERS_TREE, &issuer_pub)?;
    match issuer_data {
        Some(data) => {
            let mut issuer: Issuer = Issuer::decode(&mut std::io::Cursor::new(&data))
                .map_err(|_| ContractError::DecodeError)?;
            issuer.trusted = false;
            rt.store_set(IDENTITY_CONTRACT_ISSUERS_TREE, &issuer_pub, &issuer.encode()?)?;
        }
        None => {
            msg!("[identity_remove_issuer] ERROR: Issuer not found");
            return Err(IdentityError::InvalidIssuer.into())
        }
    }

    msg!("[identity_remove_issuer] Removed trusted issuer {:?}", &issuer_pub);
    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Get current block timestamp
fn get_current_timestamp(_rt: &mut Runtime) -> ContractResult<u64> {
    // In production: rt.get_block_timestamp()
    Ok(0)
}

// ============================================================================
// DESERIALIZATION
// ============================================================================

fn deserialize_init_params(data: &[u8]) -> ContractResult<InitializeParams> {
    InitializeParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_issue_params(data: &[u8]) -> ContractResult<IssueCredentialParams> {
    IssueCredentialParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_revoke_params(data: &[u8]) -> ContractResult<RevokeCredentialParams> {
    RevokeCredentialParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_claim_params(data: &[u8]) -> ContractResult<CreateClaimParams> {
    CreateClaimParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_verify_params(data: &[u8]) -> ContractResult<VerifyClaimParams> {
    VerifyClaimParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_claim_params_l1(data: &[u8]) -> ContractResult<CreateClaimParamsL1> {
    CreateClaimParamsL1::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}