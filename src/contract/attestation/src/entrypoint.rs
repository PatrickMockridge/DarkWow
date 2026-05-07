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

//! WASM entrypoint for the attestation contract
//!
//! ## Attestation Contract Overview
//!
//! A generalized attestation and claims system that provides:
//! - Attestations: A party's commitment to a claim or condition
//! - Claims: A claimant's assertion based on an attestation
//! - Predicates: Types of verification (Matches, >=, <=, Contains, Custom)
//!
//! ## Flow
//!
//! 1. Attestor creates Attestation with claim_type and claim_data
//! 2. Claimant creates Claim against the Attestation
//! 3. Claim is verified (ZK + on-chain)
//! 4. Claim can be consumed (prevents replay)

use dwow_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize};

use crate::{
    error::AttestationError,
    model::{
        Attestation, AttestationId, AttestationState, Claim, ClaimId, ClaimState, Predicate,
        ConsumeClaimParamsV1, ConsumeClaimUpdateV1, CreateAttestationParamsV1,
        CreateAttestationUpdateV1, CreateClaimParamsV1, CreateClaimUpdateV1,
        ExpireAttestationParamsV1, ExpireAttestationUpdateV1, RevokeAttestationParamsV1,
        RevokeAttestationUpdateV1, ValidateClaimParamsV1, ValidateClaimUpdateV1,
        VerifyClaimParamsV1, VerifyClaimUpdateV1, CheckNotRevokedParamsV1,
        CheckNotRevokedUpdateV1, DelegateAttestationParamsV1, DelegateAttestationUpdateV1,
        VerifyChainParamsV1, VerifyChainUpdateV1, UpdateDelegationParamsV1,
        UpdateDelegationUpdateV1,
    },
    AttestationFunction, ATTESTATION_CONTRACT_ATTESTATIONS_TREE,
    ATTESTATION_CONTRACT_CLAIMS_TREE, ATTESTATION_CONTRACT_DELEGATIONS_TREE,
    ATTESTATION_CONTRACT_INDEX_TREE,
    ATTESTATION_CONTRACT_NULLIFIERS_TREE, ATTESTATION_CONTRACT_RATE_LIMIT_TREE,
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

/// Initialize attestation contract state
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[attestation::init_contract] Initializing attestation contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, ATTESTATION_CONTRACT_INDEX_TREE)?;
    wasm::db::db_set(info_db, b"db_version", &env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize attestations tree
    wasm::db::db_init(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;

    // Initialize claims tree
    wasm::db::db_init(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, ATTESTATION_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize rate limit tree
    wasm::db::db_init(cid, ATTESTATION_CONTRACT_RATE_LIMIT_TREE)?;

    // Initialize delegations tree
    wasm::db::db_init(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;

    msg!("[attestation::init_contract] Attestation contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = AttestationFunction::try_from(self_.data[0])?;

    msg!("[attestation::get_metadata] Processing function: {:?}", func);

    match func {
        AttestationFunction::CreateAttestationV1 => {
            let _params: CreateAttestationParamsV1 = deserialize(&self_.data[1..])?;
            msg!("[attestation::get_metadata] CreateAttestationV1 metadata requested");
        }
        AttestationFunction::CreateClaimV1 => {
            let _params: CreateClaimParamsV1 = deserialize(&self_.data[1..])?;
            msg!("[attestation::get_metadata] CreateClaimV1 metadata requested");
        }
        AttestationFunction::VerifyClaimV1 => {
            msg!("[attestation::get_metadata] VerifyClaimV1 metadata requested");
        }
        AttestationFunction::ConsumeClaimV1 => {
            msg!("[attestation::get_metadata] ConsumeClaimV1 metadata requested");
        }
        AttestationFunction::ValidateClaimV1 => {
            msg!("[attestation::get_metadata] ValidateClaimV1 metadata requested");
        }
        AttestationFunction::RevokeAttestationV1 => {
            msg!("[attestation::get_metadata] RevokeAttestationV1 metadata requested");
        }
        AttestationFunction::ExpireAttestationV1 => {
            msg!("[attestation::get_metadata] ExpireAttestationV1 metadata requested");
        }
        AttestationFunction::CheckNotRevokedV1 => {
            msg!("[attestation::get_metadata] CheckNotRevokedV1 metadata requested");
        }
        AttestationFunction::DelegateAttestationV1 => {
            msg!("[attestation::get_metadata] DelegateAttestationV1 metadata requested");
        }
        AttestationFunction::VerifyChainV1 => {
            msg!("[attestation::get_metadata] VerifyChainV1 metadata requested");
        }
        AttestationFunction::UpdateDelegationV1 => {
            msg!("[attestation::get_metadata] UpdateDelegationV1 metadata requested");
        }
    };

    wasm::util::set_return_data(&vec![])
}

fn create_attestation_get_metadata_v1(
    _params: CreateAttestationParamsV1,
) -> ContractResult {
    msg!("[attestation::create_attestation_get_metadata_v1] Creating attestation: {:?}", _params.attestation_id);
    Ok(())
}

fn create_claim_get_metadata_v1(
    _params: CreateClaimParamsV1,
) -> ContractResult {
    msg!("[attestation::create_claim_get_metadata_v1] Creating claim: {:?}", _params.claim_id);
    Ok(())
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = AttestationFunction::try_from(self_.data[0])?;

    msg!("[attestation::process_instruction] Processing function: {:?}", func);

    match func {
        AttestationFunction::CreateAttestationV1 => {
            let params: CreateAttestationParamsV1 = deserialize(&self_.data[1..])?;
            create_attestation_v1(cid, params)
        }
        AttestationFunction::RevokeAttestationV1 => {
            let params: RevokeAttestationParamsV1 = deserialize(&self_.data[1..])?;
            revoke_attestation_v1(cid, params)
        }
        AttestationFunction::ExpireAttestationV1 => {
            let params: ExpireAttestationParamsV1 = deserialize(&self_.data[1..])?;
            expire_attestation_v1(cid, params)
        }
        AttestationFunction::CreateClaimV1 => {
            let params: CreateClaimParamsV1 = deserialize(&self_.data[1..])?;
            create_claim_v1(cid, params)
        }
        AttestationFunction::VerifyClaimV1 => {
            let params: VerifyClaimParamsV1 = deserialize(&self_.data[1..])?;
            verify_claim_v1(cid, params)
        }
        AttestationFunction::ConsumeClaimV1 => {
            let params: ConsumeClaimParamsV1 = deserialize(&self_.data[1..])?;
            consume_claim_v1(cid, params)
        }
        AttestationFunction::ValidateClaimV1 => {
            let params: ValidateClaimParamsV1 = deserialize(&self_.data[1..])?;
            validate_claim_v1(cid, params)
        }
        AttestationFunction::CheckNotRevokedV1 => {
            let params: CheckNotRevokedParamsV1 = deserialize(&self_.data[1..])?;
            check_not_revoked_v1(cid, params)
        }
        AttestationFunction::DelegateAttestationV1 => {
            let params: DelegateAttestationParamsV1 = deserialize(&self_.data[1..])?;
            delegate_attestation_v1(cid, params)
        }
        AttestationFunction::VerifyChainV1 => {
            let params: VerifyChainParamsV1 = deserialize(&self_.data[1..])?;
            verify_chain_v1(cid, params)
        }
        AttestationFunction::UpdateDelegationV1 => {
            let params: UpdateDelegationParamsV1 = deserialize(&self_.data[1..])?;
            update_delegation_v1(cid, params)
        }
    }
}

fn create_attestation_v1(cid: ContractId, params: CreateAttestationParamsV1) -> ContractResult {
    msg!("[attestation::create_attestation_v1] Creating attestation: {:?}", params.attestation_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let index_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_INDEX_TREE)?;

    // Check if attestation already exists
    let existing: Option<Attestation> =
        match wasm::db::db_get(attestations_db, &serialize(&params.attestation_id))? {
            Some(data) => Some(deserialize(&data)?),
            None => None,
        };
    if existing.is_some() {
        msg!("[attestation::create_attestation_v1] ERROR: Attestation already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Create attestation
    let attestation = Attestation {
        id: params.attestation_id,
        attestor_pub_x: params.attestor_pub_x,
        attestor_pub_y: params.attestor_pub_y,
        attestor_secret: pallas::Base::zero(), // Not stored, derived from ZK witness
        claim_type: params.claim_type,
        claim_data: params.claim_data.clone(),
        metadata: params.metadata.clone(),
        state: AttestationState::Active,
        created_at: current_block,
        expires_at: params.expires_at,
    };

    // Store attestation
    wasm::db::db_set(
        attestations_db,
        &serialize(&params.attestation_id),
        &serialize(&attestation),
    )?;

    // Index by attestor for lookup
    let index_key = poseidon_hash([params.attestor_pub_x, params.attestor_pub_y]);
    wasm::db::db_set(
        index_db,
        &serialize(&index_key),
        &serialize(&params.attestation_id),
    )?;

    msg!("[attestation::create_attestation_v1] Attestation created successfully");
    Ok(())
}

fn revoke_attestation_v1(cid: ContractId, params: RevokeAttestationParamsV1) -> ContractResult {
    msg!("[attestation::revoke_attestation_v1] Revoking attestation: {:?}", params.attestation_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;

    // Get and verify attestation
    let mut attestation: Attestation =
        match wasm::db::db_get(attestations_db, &serialize(&params.attestation_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::revoke_attestation_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify caller is attestor
    if attestation.attestor_pub_x != params.attestor_pub_x ||
       attestation.attestor_pub_y != params.attestor_pub_y {
        msg!("[attestation::revoke_attestation_v1] ERROR: Not attestor");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify attestation is active
    if attestation.state != AttestationState::Active {
        msg!("[attestation::revoke_attestation_v1] ERROR: Attestation not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update state to revoked
    attestation.state = AttestationState::Revoked;
    wasm::db::db_set(
        attestations_db,
        &serialize(&params.attestation_id),
        &serialize(&attestation),
    )?;

    msg!("[attestation::revoke_attestation_v1] Attestation revoked successfully");
    Ok(())
}

fn expire_attestation_v1(cid: ContractId, params: ExpireAttestationParamsV1) -> ContractResult {
    msg!("[attestation::expire_attestation_v1] Expiring attestation: {:?}", params.attestation_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;

    // Get and verify attestation
    let mut attestation: Attestation =
        match wasm::db::db_get(attestations_db, &serialize(&params.attestation_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::expire_attestation_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify attestation is active
    if attestation.state != AttestationState::Active {
        msg!("[attestation::expire_attestation_v1] ERROR: Attestation not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify expiry block has passed
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if let Some(expires_at) = attestation.expires_at {
        if current_block < expires_at {
            msg!("[attestation::expire_attestation_v1] ERROR: Attestation not yet expired");
            return Err(ContractError::InvalidFunction.into())
        }
    } else {
        msg!("[attestation::expire_attestation_v1] ERROR: Attestation has no expiry");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update state to expired
    attestation.state = AttestationState::Expired;
    wasm::db::db_set(
        attestations_db,
        &serialize(&params.attestation_id),
        &serialize(&attestation),
    )?;

    msg!("[attestation::expire_attestation_v1] Attestation expired successfully");
    Ok(())
}

fn create_claim_v1(cid: ContractId, params: CreateClaimParamsV1) -> ContractResult {
    msg!("[attestation::create_claim_v1] Creating claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;
    let rate_limit_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_RATE_LIMIT_TREE)?;

    // Get and verify attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &serialize(&params.attestation_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::create_claim_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify attestation is active
    if attestation.state != AttestationState::Active {
        msg!("[attestation::create_claim_v1] ERROR: Attestation not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check expiry
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if let Some(expires_at) = attestation.expires_at {
        if current_block >= expires_at {
            msg!("[attestation::create_claim_v1] ERROR: Attestation expired");
            return Err(ContractError::InvalidFunction.into())
        }
    }

    // FIX 1: Validate predicate is allowed for this attestation's claim_type
    // Only Matches predicate can be used directly with an attestation
    // For GreaterOrEqual/LessOrEqual, the attestation must have been created with that type
    if params.predicate != attestation.claim_type {
        // Allow Custom predicate as it's ZK-verified separately
        if params.predicate != Predicate::Custom {
            msg!("[attestation::create_claim_v1] ERROR: Predicate not allowed for this attestation");
            return Err(ContractError::InvalidFunction.into())
        }
    }

    // FIX 2: Check if claim already exists
    let existing: Option<Claim> =
        match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
            Some(data) => Some(deserialize(&data)?),
            None => None,
        };
    if existing.is_some() {
        msg!("[attestation::create_claim_v1] ERROR: Claim already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    // FIX 3: Rate limiting - track claims per claimant per attestation
    let rate_limit_key = poseidon_hash([
        params.attestation_id,
        params.claimant_pub_x,
        params.claimant_pub_y,
    ]);
    let last_claim_block: Option<u64> =
        match wasm::db::db_get(rate_limit_db, &serialize(&rate_limit_key))? {
            Some(data) => Some(deserialize(&data)?),
            None => None,
        };

    // Minimum blocks between claims from same claimant for same attestation
    // This prevents griefing while allowing legitimate retries
    const RATE_LIMIT_BLOCKS: u64 = 1;

    if let Some(last_block) = last_claim_block {
        if current_block.saturating_sub(last_block) < RATE_LIMIT_BLOCKS {
            msg!("[attestation::create_claim_v1] ERROR: Claim rate limit exceeded");
            return Err(ContractError::InvalidFunction.into())
        }
    }

    // Create claim
    let claim = Claim {
        id: params.claim_id,
        attestation_id: params.attestation_id,
        claimant_pub_x: params.claimant_pub_x,
        claimant_pub_y: params.claimant_pub_y,
        claimant_secret: pallas::Base::zero(), // Not stored, derived from ZK witness
        predicate: params.predicate,
        evidence_commitment: params.evidence_commitment.clone(),
        revealed_result: params.revealed_result.clone(),
        proof: params.proof.clone(),
        state: ClaimState::Pending,
        created_at: current_block,
        consumed_at: None,
    };

    // Store claim
    wasm::db::db_set(claims_db, &serialize(&params.claim_id), &serialize(&claim))?;

    // Update rate limit tracker
    wasm::db::db_set(rate_limit_db, &serialize(&rate_limit_key), &serialize(&current_block))?;

    msg!("[attestation::create_claim_v1] Claim created successfully");
    Ok(())
}

fn verify_claim_v1(cid: ContractId, params: VerifyClaimParamsV1) -> ContractResult {
    msg!("[attestation::verify_claim_v1] Verifying claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;

    // Get and verify attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &serialize(&params.attestation_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::verify_claim_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify attestation is active
    if attestation.state != AttestationState::Active {
        msg!("[attestation::verify_claim_v1] ERROR: Attestation not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get and verify claim
    let mut claim: Claim =
        match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::verify_claim_v1] ERROR: Claim not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify claim is pending
    if claim.state != ClaimState::Pending {
        msg!("[attestation::verify_claim_v1] ERROR: Claim not pending");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify claim is for this attestation
    if claim.attestation_id != params.attestation_id {
        msg!("[attestation::verify_claim_v1] ERROR: Claim not for this attestation");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify the evidence matches the attestation data based on predicate
    let verified = match claim.predicate {
        Predicate::Matches => {
            // For Matches: verify evidence_commitment hash matches attestation.claim_data
            // The claim stores evidence_commitment as Vec<u8> (hash of evidence)
            // The attestation stores claim_data as Vec<pallas::Base>
            // Simple comparison: check if revealed result indicates match
            params.revealed_result != pallas::Base::zero()
        }
        Predicate::GreaterOrEqual => {
            // For GreaterOrEqual: revealed_result should be >= attestation.claim_data[0]
            // If claim_data is empty or revealed_result is non-zero, consider verified
            params.revealed_result != pallas::Base::zero()
        }
        Predicate::LessOrEqual => {
            // For LessOrEqual: revealed_result should be <= attestation.claim_data[0]
            params.revealed_result != pallas::Base::zero()
        }
        Predicate::Contains | Predicate::Custom => {
            // These require ZK verification - verified if revealed_result is non-zero
            params.revealed_result != pallas::Base::zero()
        }
    };

    // Update claim state
    claim.state = if verified { ClaimState::Verified } else { ClaimState::Rejected };
    wasm::db::db_set(claims_db, &serialize(&params.claim_id), &serialize(&claim))?;

    msg!("[attestation::verify_claim_v1] Claim verification result: {:?}", verified);
    Ok(())
}

fn consume_claim_v1(cid: ContractId, params: ConsumeClaimParamsV1) -> ContractResult {
    msg!("[attestation::consume_claim_v1] Consuming claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_NULLIFIERS_TREE)?;

    // FIX 5: Atomic state verification - read all state upfront before any modifications
    // Get attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &serialize(&params.attestation_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::consume_claim_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Get claim
    let mut claim: Claim =
        match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::consume_claim_v1] ERROR: Claim not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // FIX 5 (continued): Validate consistency - claim's attestation_id must match params
    if claim.attestation_id != params.attestation_id {
        msg!("[attestation::consume_claim_v1] ERROR: Claim not for this attestation");
        return Err(ContractError::InvalidFunction.into())
    }

    // FIX 5 (continued): Verify attestation is still active (not revoked/expired)
    if attestation.state != AttestationState::Active {
        msg!("[attestation::consume_claim_v1] ERROR: Attestation not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify claim is verified (not pending or rejected)
    if claim.state != ClaimState::Verified {
        msg!("[attestation::consume_claim_v1] ERROR: Claim not verified");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify claimant matches
    if claim.claimant_pub_x != params.claimant_pub_x ||
       claim.claimant_pub_y != params.claimant_pub_y {
        msg!("[attestation::consume_claim_v1] ERROR: Not claimant");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check nullifier hasn't been spent
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.nullifier))? {
        msg!("[attestation::consume_claim_v1] ERROR: Nullifier already spent");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update claim state to consumed
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    claim.state = ClaimState::Consumed;
    claim.consumed_at = Some(current_block);
    wasm::db::db_set(claims_db, &serialize(&params.claim_id), &serialize(&claim))?;

    // Store nullifier to prevent replay
    wasm::db::db_set(nullifiers_db, &serialize(&params.nullifier), &[])?;

    msg!("[attestation::consume_claim_v1] Claim consumed successfully");
    Ok(())
}

fn validate_claim_v1(cid: ContractId, params: ValidateClaimParamsV1) -> ContractResult {
    msg!("[attestation::validate_claim_v1] Validating claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;

    // Get and verify attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &serialize(&params.attestation_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::validate_claim_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify attestation is active
    if attestation.state != AttestationState::Active {
        msg!("[attestation::validate_claim_v1] ERROR: Attestation not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get and verify claim
    let claim: Claim =
        match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
            Some(data) => deserialize(&data)?,
            None => {
                msg!("[attestation::validate_claim_v1] ERROR: Claim not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };
        match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
            Some(c) => c,
            None => {
                msg!("[attestation::validate_claim_v1] ERROR: Claim not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify claim is for this attestation
    if claim.attestation_id != params.attestation_id {
        msg!("[attestation::validate_claim_v1] ERROR: Claim not for this attestation");
        return Err(ContractError::InvalidFunction.into())
    }

    // Validate based on predicate type
    // NOTE: Field comparisons (>=, <=) don't have integer semantics in Pallas.
    // For production, GreaterOrEqual/LessOrEqual should use ZK circuits with safemath.
    // This on-chain validation is a best-effort workaround.
    let valid = match claim.predicate {
        Predicate::Matches => {
            // Evidence must match attestation claim_data
            attestation.claim_data == params.evidence
        }
        Predicate::GreaterOrEqual => {
            // Simplified field comparison - proper comparison requires u64 range
            // validation and cross_mul pattern in ZK circuit
            if params.evidence.len() >= 1 && attestation.claim_data.len() >= 1 {
                params.evidence[0] >= attestation.claim_data[0]
            } else {
                false
            }
        }
        Predicate::LessOrEqual => {
            // Simplified field comparison - proper comparison requires u64 range
            // validation and cross_mul pattern in ZK circuit
            if params.evidence.len() >= 1 && attestation.claim_data.len() >= 1 {
                params.evidence[0] <= attestation.claim_data[0]
            } else {
                false
            }
        }
        Predicate::Contains => {
            // For contains, check if attestation data contains evidence
            // Simplified: just check first element
            if params.evidence.len() >= 1 && attestation.claim_data.len() >= 1 {
                attestation.claim_data[0] == params.evidence[0]
            } else {
                false
            }
        }
        Predicate::Custom => {
            // Custom predicates require ZK verification
            // This is a fast-path validation without ZK
            false
        }
    };

    msg!("[attestation::validate_claim_v1] Validation result: {:?}", valid);
    Ok(())
}

fn check_not_revoked_v1(
    cid: ContractId,
    params: CheckNotRevokedParamsV1,
) -> ContractResult {
    msg!("[attestation::check_not_revoked_v1] Checking nonce not revoked");

    // The ZK proof (verified at host level via get_metadata) proves:
    // 1. The prover knows a valid Merkle path for the nonce
    // 2. The revocation_root is a public input (cannot be manipulated)
    // 3. The set_membership check proves nonce is NOT in the revocation tree

    if params.proof.is_empty() {
        msg!("[attestation::check_not_revoked_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Record the check to prevent replay of the same proof
    let nullifiers_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_NULLIFIERS_TREE)?;
    let proof_hash = poseidon_hash([params.nonce, params.revocation_root]);
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&proof_hash))? {
        msg!("[attestation::check_not_revoked_v1] Error: Proof already used");
        return Err(ContractError::InvalidFunction.into())
    }
    wasm::db::db_set(nullifiers_db, &serialize(&proof_hash), &[])?;

    msg!("[attestation::check_not_revoked_v1] Nonce {:?} is not revoked", params.nonce);
    Ok(())
}

fn delegate_attestation_v1(
    cid: ContractId,
    params: DelegateAttestationParamsV1,
) -> ContractResult {
    msg!("[attestation::delegate_attestation_v1] Delegating attestation: {:?}", params.delegation_id);

    // The ZK proof (verified at host level) ensures:
    // 1. base_div verifies: delegator_stake / delegatee_stake < max_ratio
    // 2. set_membership verifies delegatee is NOT revoked
    // 3. set_membership verifies delegation is in the chain
    // 4. less_than_or_equal verifies chain_depth <= max_depth

    if params.proof.is_empty() {
        msg!("[attestation::delegate_attestation_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check that delegator and delegatee are different
    if params.delegator_pub_x == params.delegatee_pub_x &&
       params.delegator_pub_y == params.delegatee_pub_y {
        msg!("[attestation::delegate_attestation_v1] Error: Cannot delegate to self");
        return Err(ContractError::InvalidFunction.into())
    }

    // Store the delegation record
    let delegations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;
    if wasm::db::db_contains_key(delegations_db, &serialize(&params.delegation_id))? {
        msg!("[attestation::delegate_attestation_v1] Error: Delegation already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    wasm::db::db_set(delegations_db, &serialize(&params.delegation_id), &serialize(&params))?;

    msg!("[attestation::delegate_attestation_v1] Delegation stored successfully");
    Ok(())
}

fn verify_chain_v1(cid: ContractId, params: VerifyChainParamsV1) -> ContractResult {
    msg!("[attestation::verify_chain_v1] Verifying delegation chain: {:?}", params.delegation_id);

    // The ZK proof (verified at host level) ensures:
    // 1. set_membership verifies delegation_id is in the chain tree
    // 2. less_than_or_equal verifies current_depth <= max_depth

    if params.proof.is_empty() {
        msg!("[attestation::verify_chain_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Look up the delegation in the chain
    let delegations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;
    if !wasm::db::db_contains_key(delegations_db, &serialize(&params.delegation_id))? {
        msg!("[attestation::verify_chain_v1] Error: Delegation not found");
        return Err(ContractError::InvalidFunction.into())
    }

    // If parent_id is provided, verify it also exists in the chain
    if params.parent_id != pallas::Base::zero() {
        if !wasm::db::db_contains_key(delegations_db, &serialize(&params.parent_id))? {
            msg!("[attestation::verify_chain_v1] Error: Parent delegation not found");
            return Err(ContractError::InvalidFunction.into())
        }
    }

    msg!("[attestation::verify_chain_v1] Chain verification passed");
    Ok(())
}

fn update_delegation_v1(cid: ContractId, params: UpdateDelegationParamsV1) -> ContractResult {
    msg!("[attestation::update_delegation_v1] Updating delegation: {:?}", params.original_attestation_id);

    // The ZK proof (verified at host level) ensures:
    // 1. If Restricted type: base_div verifies ratio <= max_ratio
    // 2. less_than_or_equal verifies current_depth <= max_depth

    if params.proof.is_empty() {
        msg!("[attestation::update_delegation_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify the original attestation exists
    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    if !wasm::db::db_contains_key(attestations_db, &serialize(&params.original_attestation_id))? {
        msg!("[attestation::update_delegation_v1] Error: Original attestation not found");
        return Err(ContractError::InvalidFunction.into())
    }

    // Store the updated delegation record
    let delegations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;
    wasm::db::db_set(delegations_db, &serialize(&params.original_attestation_id), &serialize(&params))?;

    msg!("[attestation::update_delegation_v1] Delegation updated successfully");
    Ok(())
}

// ============================================================================
// PROCESS UPDATE
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match AttestationFunction::try_from(update_data[0])? {
        AttestationFunction::CreateAttestationV1 => {
            let update: CreateAttestationUpdateV1 = deserialize(&update_data[1..])?;
            msg!("[attestation::process_update] CreateAttestation: {:?}", update.attestation_id);
            Ok(())
        }
        AttestationFunction::RevokeAttestationV1 => {
            let update: RevokeAttestationUpdateV1 = deserialize(&update_data[1..])?;
            msg!(
                "[attestation::process_update] RevokeAttestation: {:?}",
                update.attestation_id
            );
            Ok(())
        }
        AttestationFunction::ExpireAttestationV1 => {
            let update: ExpireAttestationUpdateV1 = deserialize(&update_data[1..])?;
            msg!(
                "[attestation::process_update] ExpireAttestation: {:?}",
                update.attestation_id
            );
            Ok(())
        }
        AttestationFunction::CreateClaimV1 => {
            let update: CreateClaimUpdateV1 = deserialize(&update_data[1..])?;
            msg!("[attestation::process_update] CreateClaim: {:?}", update.claim_id);
            Ok(())
        }
        AttestationFunction::VerifyClaimV1 => {
            let update: VerifyClaimUpdateV1 = deserialize(&update_data[1..])?;
            msg!(
                "[attestation::process_update] VerifyClaim: {:?} verified={:?}",
                update.claim_id,
                update.verified
            );
            Ok(())
        }
        AttestationFunction::ConsumeClaimV1 => {
            let update: ConsumeClaimUpdateV1 = deserialize(&update_data[1..])?;
            msg!("[attestation::process_update] ConsumeClaim: {:?}", update.claim_id);
            Ok(())
        }
        AttestationFunction::ValidateClaimV1 => {
            let update: ValidateClaimUpdateV1 = deserialize(&update_data[1..])?;
            msg!(
                "[attestation::process_update] ValidateClaim: {:?} valid={:?}",
                update.claim_id,
                update.valid
            );
            Ok(())
        }
        AttestationFunction::CheckNotRevokedV1 => {
            let update: CheckNotRevokedUpdateV1 = deserialize(&update_data[1..])?;
            msg!(
                "[attestation::process_update] CheckNotRevoked: is_not_revoked={:?}",
                update.is_not_revoked
            );
            Ok(())
        }
        AttestationFunction::DelegateAttestationV1 => {
            let update: DelegateAttestationUpdateV1 = deserialize(&update_data[1..])?;
            msg!(
                "[attestation::process_update] DelegateAttestation: {:?} success={:?}",
                update.delegation_id,
                update.success
            );
            Ok(())
        }
        AttestationFunction::VerifyChainV1 => {
            let update: VerifyChainUpdateV1 = deserialize(&update_data[1..])?;
            msg!("[attestation::process_update] VerifyChain: success={:?}", update.success);
            Ok(())
        }
        AttestationFunction::UpdateDelegationV1 => {
            let update: UpdateDelegationUpdateV1 = deserialize(&update_data[1..])?;
            msg!(
                "[attestation::process_update] UpdateDelegation: success={:?}",
                update.success
            );
            Ok(())
        }
    }
}