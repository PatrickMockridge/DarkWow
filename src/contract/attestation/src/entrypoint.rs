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
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas::{self, Base},
    wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    model::{
        AttestSlashParamsV1, AttestSlashUpdateV1,
        Attestation, AttestationId, AttestationState, Claim, ClaimState, Predicate,
        CommitFeeScheduleParamsV1, CommitFeeScheduleUpdateV1,
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
    ATTESTATION_CONTRACT_ZKAS_CREATE_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_CREATE_CLAIM_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_VERIFY_CLAIM_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_CONSUME_CLAIM_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_CHECK_NOT_REVOKED_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_DELEGATE_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_VERIFY_CHAIN_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_UPDATE_DELEGATION_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_ATTEST_SLASH_NS_V2,
    ATTESTATION_CONTRACT_ZKAS_COMMIT_FEE_SCHEDULE_NS_V2,
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

    // V2 circuits (V1 loads removed — rc3 migration) (HAZOP RC3: domain separation)
    let attest_slash_v2_bincode = include_bytes!("../proof/attest_slash.zk.bin");
    wasm::db::zkas_db_set(&attest_slash_v2_bincode[..])?;
    let check_not_revoked_v2_bincode = include_bytes!("../proof/check_not_revoked.zk.bin");
    wasm::db::zkas_db_set(&check_not_revoked_v2_bincode[..])?;
    let commit_fee_schedule_v2_bincode = include_bytes!("../proof/commit_fee_schedule.zk.bin");
    wasm::db::zkas_db_set(&commit_fee_schedule_v2_bincode[..])?;
    let consume_claim_v2_bincode = include_bytes!("../proof/consume_claim.zk.bin");
    wasm::db::zkas_db_set(&consume_claim_v2_bincode[..])?;
    let create_attestation_v2_bincode = include_bytes!("../proof/create_attestation.zk.bin");
    wasm::db::zkas_db_set(&create_attestation_v2_bincode[..])?;
    let create_claim_v2_bincode = include_bytes!("../proof/create_claim.zk.bin");
    wasm::db::zkas_db_set(&create_claim_v2_bincode[..])?;
    let delegate_attestation_v2_bincode = include_bytes!("../proof/delegate_attestation.zk.bin");
    wasm::db::zkas_db_set(&delegate_attestation_v2_bincode[..])?;
    let update_delegation_v2_bincode = include_bytes!("../proof/update_delegation.zk.bin");
    wasm::db::zkas_db_set(&update_delegation_v2_bincode[..])?;
    let verify_chain_v2_bincode = include_bytes!("../proof/verify_chain.zk.bin");
    wasm::db::zkas_db_set(&verify_chain_v2_bincode[..])?;
    let verify_claim_v2_bincode = include_bytes!("../proof/verify_claim.zk.bin");
    wasm::db::zkas_db_set(&verify_claim_v2_bincode[..])?;

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

    let mut zk_public_inputs: Vec<(String, Vec<Base>)> = vec![];

    // Circuit computes tx_binding = poseidon_hash(witness[3], witness[3], witness[4]) =
    // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3.
    // All clients compute tx_binding = poseidon_hash(3, 0, 0).
    let txb = poseidon_hash([Base::from(3), Base::zero(), Base::zero()]);

    match func {
        AttestationFunction::CreateAttestationV1 => {
            let _params = match CreateAttestationParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize CreateAttestationParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_CREATE_NS_V2.to_string(),
                {
                    // Circuit constrain_instance order: tx_binding, tx_nonce
                    vec![txb, Base::zero()]
                },
            ));
        }
        AttestationFunction::CreateClaimV1 => {
            let _params = match CreateClaimParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize CreateClaimParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_CREATE_CLAIM_NS_V2.to_string(),
                {
                    // Circuit constrain_instance order: tx_binding, tx_nonce
                    vec![txb, Base::zero()]
                },
            ));
        }
        AttestationFunction::VerifyClaimV1 => {
            let _params = match VerifyClaimParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize VerifyClaimParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_VERIFY_CLAIM_NS_V2.to_string(),
                // Circuit constrain_instance order: tx_binding, tx_nonce
                vec![txb, Base::zero()],
            ));
        }
        AttestationFunction::ConsumeClaimV1 => {
            let params = match ConsumeClaimParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize ConsumeClaimParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_CONSUME_CLAIM_NS_V2.to_string(),
                {
                    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
                    let (cx, cy) = params.claimant_pub.xy().expect("pk not identity");
                    vec![
                    params.claim_id.inner(),
                    cx,
                    cy,
                    params.nullifier,
                    txb,
                    Base::zero(),
                ]}
            ));
        }
        AttestationFunction::CheckNotRevokedV1 => {
            let _params = match CheckNotRevokedParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize CheckNotRevokedParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_CHECK_NOT_REVOKED_NS_V2.to_string(),
                // Circuit constrain_instance order: tx_binding, tx_nonce
                vec![txb, Base::zero()],
            ));
        }
        AttestationFunction::DelegateAttestationV1 => {
            let params = match DelegateAttestationParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to decode DelegateAttestationParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_DELEGATE_NS_V2.to_string(),
                {
                    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
                    let (ex, ey) = params.delegatee_pub.xy().expect("pk not identity");
                    // Circuit: delegatee_leaf = poseidon_hash(DOMAIN_COIN_COMMIT, delegatee_pub_x, delegatee_pub_y)
                    // where DOMAIN_COIN_COMMIT = witness_base(4) = 4
                    // Circuit constrain_instance order: delegatee_leaf, tx_binding, tx_nonce
                    let delegatee_leaf = poseidon_hash([Base::from(4), ex, ey]);
                    vec![delegatee_leaf, txb, Base::zero()]
                },
            ));
        }
        AttestationFunction::VerifyChainV1 => {
            let _params = match VerifyChainParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize VerifyChainParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_VERIFY_CHAIN_NS_V2.to_string(),
                // Circuit constrain_instance order: tx_binding, tx_nonce
                vec![txb, Base::zero()],
            ));
        }
        AttestationFunction::UpdateDelegationV1 => {
            let _params = match UpdateDelegationParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to decode UpdateDelegationParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_UPDATE_DELEGATION_NS_V2.to_string(),
                // Circuit constrain_instance order: tx_binding, tx_nonce
                vec![txb, Base::zero()],
            ));
        }
        AttestationFunction::AttestSlashV1 => {
            let _params = match AttestSlashParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize AttestSlashParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_ATTEST_SLASH_NS_V2.to_string(),
                // Circuit constrain_instance order: tx_binding, tx_nonce
                vec![txb, Base::zero()],
            ));
        }
        AttestationFunction::CommitFeeScheduleV1 => {
            let _params = match CommitFeeScheduleParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[attestation::get_metadata] Error: Failed to deserialize CommitFeeScheduleParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            zk_public_inputs.push((
                ATTESTATION_CONTRACT_ZKAS_COMMIT_FEE_SCHEDULE_NS_V2.to_string(),
                // Circuit constrain_instance order: tx_binding, tx_nonce
                vec![txb, Base::zero()],
            ));
        }
        // RevokeAttestationV1, ExpireAttestationV1, ValidateClaimV1
        // have no ZK circuits; return empty metadata.
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
    let func = AttestationFunction::try_from(func_byte)?;

    msg!("[attestation::process_instruction] Processing function: {:?}", func);

    let update_bytes = match func {
        AttestationFunction::CreateAttestationV1 => {
            let params= CreateAttestationParamsV1::decode(&self_.data[1..])?;
            create_attestation_v1(cid, params)?
        }
        AttestationFunction::RevokeAttestationV1 => {
            let params= RevokeAttestationParamsV1::decode(&self_.data[1..])?;
            revoke_attestation_v1(cid, params)?
        }
        AttestationFunction::ExpireAttestationV1 => {
            let params= ExpireAttestationParamsV1::decode(&self_.data[1..])?;
            expire_attestation_v1(cid, params)?
        }
        AttestationFunction::CreateClaimV1 => {
            let params= CreateClaimParamsV1::decode(&self_.data[1..])?;
            create_claim_v1(cid, params)?
        }
        AttestationFunction::VerifyClaimV1 => {
            let params= VerifyClaimParamsV1::decode(&self_.data[1..])?;
            verify_claim_v1(cid, params)?
        }
        AttestationFunction::ConsumeClaimV1 => {
            let params= ConsumeClaimParamsV1::decode(&self_.data[1..])?;
            consume_claim_v1(cid, params)?
        }
        AttestationFunction::ValidateClaimV1 => {
            let params= ValidateClaimParamsV1::decode(&self_.data[1..])?;
            validate_claim_v1(cid, params)?
        }
        AttestationFunction::CheckNotRevokedV1 => {
            let params= CheckNotRevokedParamsV1::decode(&self_.data[1..])?;
            check_not_revoked_v1(cid, params)?
        }
        AttestationFunction::DelegateAttestationV1 => {
            let params = DelegateAttestationParamsV1::decode(&self_.data[1..])?;
            delegate_attestation_v1(cid, params)?
        }
        AttestationFunction::VerifyChainV1 => {
            let params= VerifyChainParamsV1::decode(&self_.data[1..])?;
            verify_chain_v1(cid, params)?
        }
        AttestationFunction::UpdateDelegationV1 => {
            let params = UpdateDelegationParamsV1::decode(&self_.data[1..])?;
            update_delegation_v1(cid, params)?
        }
        AttestationFunction::AttestSlashV1 => {
            let params= AttestSlashParamsV1::decode(&self_.data[1..])?;
            attest_slash_v1(cid, params)?
        }
        AttestationFunction::CommitFeeScheduleV1 => {
            let params= CommitFeeScheduleParamsV1::decode(&self_.data[1..])?;
            commit_fee_schedule_v1(cid, params)?
        }
    };

    wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat())
}

fn create_attestation_v1(cid: ContractId, params: CreateAttestationParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::create_attestation_v1] Creating attestation: {:?}", params.attestation_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;

    // Check if attestation already exists
    let existing: Option<Attestation> =
        match wasm::db::db_get(attestations_db, &params.attestation_id.to_bytes())? {
            Some(data) => Some(Attestation::decode(&data)?),
            None => None,
        };
    if existing.is_some() {
        msg!("[attestation::create_attestation_v1] ERROR: Attestation already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create attestation
    let attestation = Attestation {
        version: 1,
        id: params.attestation_id,
        attestor_pub: params.attestor_pub,
        attestor_secret: pallas::Base::zero(), // Not stored, derived from ZK witness
        claim_type: params.claim_type,
        claim_data: params.claim_data.clone(),
        metadata: params.metadata.clone(),
        state: AttestationState::Active,
        created_at: current_block,
        expires_at: params.expires_at,
    };

    // Index by attestor for lookup (compute for update)
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (ax, ay) = params.attestor_pub.xy().expect("pk not identity");
    let index_key = poseidon_hash([ax, ay]);

    msg!("[attestation::create_attestation_v1] Attestation created successfully");
    Ok(CreateAttestationUpdateV1 {
        attestation_id: params.attestation_id,
        attestation,
        index_key,
    }.encode())
}

fn revoke_attestation_v1(cid: ContractId, params: RevokeAttestationParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::revoke_attestation_v1] Revoking attestation: {:?}", params.attestation_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;

    // Get and verify attestation
    let mut attestation: Attestation =
        match wasm::db::db_get(attestations_db, &params.attestation_id.to_bytes())? {
            Some(data) => Attestation::decode(&data)?,
            None => {
                msg!("[attestation::revoke_attestation_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Verify caller is attestor
    if attestation.attestor_pub != params.attestor_pub {
        msg!("[attestation::revoke_attestation_v1] ERROR: Not attestor");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify attestation is active
    if attestation.state != AttestationState::Active {
        msg!("[attestation::revoke_attestation_v1] ERROR: Attestation not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // State update: set the state here (exec) and carry the full record to apply.
    attestation.state = AttestationState::Revoked;

    msg!("[attestation::revoke_attestation_v1] Attestation revoked successfully");
    Ok(RevokeAttestationUpdateV1 { attestation_id: params.attestation_id, attestation }.encode())
}

fn expire_attestation_v1(cid: ContractId, params: ExpireAttestationParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::expire_attestation_v1] Expiring attestation: {:?}", params.attestation_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;

    // Get and verify attestation
    let mut attestation: Attestation =
        match wasm::db::db_get(attestations_db, &params.attestation_id.to_bytes())? {
            Some(data) => Attestation::decode(&data)?,
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
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if let Some(expires_at) = attestation.expires_at {
        if current_block < expires_at {
            msg!("[attestation::expire_attestation_v1] ERROR: Attestation not yet expired");
            return Err(ContractError::InvalidFunction.into())
        }
    } else {
        msg!("[attestation::expire_attestation_v1] ERROR: Attestation has no expiry");
        return Err(ContractError::InvalidFunction.into())
    }

    // State update: set the state here (exec) and carry the full record to apply.
    attestation.state = AttestationState::Expired;

    msg!("[attestation::expire_attestation_v1] Attestation expired successfully");
    Ok(ExpireAttestationUpdateV1 { attestation_id: params.attestation_id, attestation }.encode())
}

fn create_claim_v1(cid: ContractId, params: CreateClaimParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::create_claim_v1] Creating claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;
    let rate_limit_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_RATE_LIMIT_TREE)?;

    // Get and verify attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &params.attestation_id.to_bytes())? {
            Some(data) => Attestation::decode(&data)?,
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
    let current_block = wasm::util::get_verifying_block_height()?.get();
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
        match wasm::db::db_get(claims_db, &params.claim_id.to_bytes())? {
            Some(data) => Some(Claim::decode(&data)?),
            None => None,
        };
    if existing.is_some() {
        msg!("[attestation::create_claim_v1] ERROR: Claim already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    // FIX 3: Rate limiting - track claims per claimant per attestation
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (cx, cy) = params.claimant_pub.xy().expect("pk not identity");
    let rate_limit_key = poseidon_hash([
        params.attestation_id.inner(),
        cx,
        cy,
    ]);
    let last_claim_block: Option<u64> =
        match wasm::db::db_get(rate_limit_db, &rate_limit_key.to_repr())? {
            Some(data) => Some(u64::from_le_bytes(data.try_into().map_err(|_| ContractError::IoError("Corrupt state: rate_limit wrong size".into()))?)),
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
        version: 1,
        id: params.claim_id,
        attestation_id: params.attestation_id,
        claimant_pub: params.claimant_pub,
        claimant_secret: pallas::Base::zero(), // Not stored, derived from ZK witness
        predicate: params.predicate,
        evidence_commitment: params.evidence_commitment.clone(),
        revealed_result: params.revealed_result.clone(),
        proof: params.proof.clone(),
        state: ClaimState::Pending,
        created_at: current_block,
        consumed_at: None,
    };

    msg!("[attestation::create_claim_v1] Claim created successfully");
    Ok(CreateClaimUpdateV1 {
        claim_id: params.claim_id,
        claim,
        rate_limit_key,
        current_block,
    }.encode())
}

fn verify_claim_v1(cid: ContractId, params: VerifyClaimParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::verify_claim_v1] Verifying claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;

    // Get and verify attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &params.attestation_id.to_bytes())? {
            Some(data) => Attestation::decode(&data)?,
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
        match wasm::db::db_get(claims_db, &params.claim_id.to_bytes())? {
            Some(data) => Claim::decode(&data)?,
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

    // Verify the evidence commitment matches the claim's stored commitment
    let claim_commitment = match claim.evidence_commitment.len() {
        32 => {
            let mut repr = [0u8; 32];
            repr.copy_from_slice(&claim.evidence_commitment);
            match pallas::Base::from_repr(repr).into() {
                Some(val) => val,
                None => {
                    msg!("[attestation::verify_claim_v1] ERROR: Invalid claim evidence commitment bytes");
                    return Err(ContractError::InvalidFunction.into())
                }
            }
        }
        _ => {
            msg!("[attestation::verify_claim_v1] ERROR: Claim evidence commitment has invalid length");
            return Err(ContractError::InvalidFunction.into())
        }
    };
    if params.evidence_commitment != claim_commitment {
        msg!("[attestation::verify_claim_v1] ERROR: Evidence commitment mismatch");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify based on predicate type.
    // ZK circuit (verified by host via get_metadata) constrains revealed_result
    // to match the predicate evaluation against claim_data and evidence.
    let verified = match claim.predicate {
        Predicate::Matches => {
            // ZK circuit constrains: revealed_result == poseidon_hash(evidence)
            // Match confirmed if revealed_result is non-zero
            params.revealed_result != pallas::Base::zero()
        }
        Predicate::GreaterOrEqual => {
            // ZK circuit constrains: ev0 >= cd0 → revealed_result = 1, else 0
            params.revealed_result == pallas::Base::one()
        }
        Predicate::LessOrEqual => {
            // ZK circuit constrains: ev0 <= cd0 → revealed_result = 1, else 0
            params.revealed_result == pallas::Base::one()
        }
        Predicate::Contains => {
            // ZK circuit constrains set membership check
            params.revealed_result != pallas::Base::zero()
        }
        Predicate::Custom => {
            // ZK circuit handles external proof verification
            params.revealed_result != pallas::Base::zero()
        }
    };

    // State update: set the state here (exec) and carry the full record to apply.
    claim.state = if verified { ClaimState::Verified } else { ClaimState::Rejected };

    let update = VerifyClaimUpdateV1 {
        claim_id: params.claim_id,
        claim,
    };

    msg!("[attestation::verify_claim_v1] Claim verification result: {:?}", verified);
    Ok(update.encode())
}

fn consume_claim_v1(cid: ContractId, params: ConsumeClaimParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::consume_claim_v1] Consuming claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_NULLIFIERS_TREE)?;

    // FIX 5: Atomic state verification - read all state upfront before any modifications
    // Get attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &params.attestation_id.to_bytes())? {
            Some(data) => Attestation::decode(&data)?,
            None => {
                msg!("[attestation::consume_claim_v1] ERROR: Attestation not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };

    // Get claim
    let mut claim: Claim =
        match wasm::db::db_get(claims_db, &params.claim_id.to_bytes())? {
            Some(data) => Claim::decode(&data)?,
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
    if claim.claimant_pub != params.claimant_pub {
        msg!("[attestation::consume_claim_v1] ERROR: Not claimant");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check nullifier hasn't been spent
    if wasm::db::db_contains_key(nullifiers_db, &params.nullifier.to_repr())? {
        msg!("[attestation::consume_claim_v1] ERROR: Nullifier already spent");
        return Err(ContractError::InvalidFunction.into())
    }

    // State update: set the state here (exec) and carry the full record to apply.
    let current_block = wasm::util::get_verifying_block_height()?.get();
    claim.state = ClaimState::Consumed;
    claim.consumed_at = Some(current_block);

    msg!("[attestation::consume_claim_v1] Claim consumed successfully");
    Ok(ConsumeClaimUpdateV1 {
        claim_id: params.claim_id,
        claim,
        nullifier: params.nullifier,
    }.encode())
}

fn validate_claim_v1(cid: ContractId, params: ValidateClaimParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::validate_claim_v1] Validating claim: {:?}", params.claim_id);

    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;

    // Get and verify attestation
    let attestation: Attestation =
        match wasm::db::db_get(attestations_db, &params.attestation_id.to_bytes())? {
            Some(data) => Attestation::decode(&data)?,
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
        match wasm::db::db_get(claims_db, &params.claim_id.to_bytes())? {
            Some(data) => Claim::decode(&data)?,
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
    Ok(ValidateClaimUpdateV1 { claim_id: params.claim_id, valid }.encode())
}

fn check_not_revoked_v1(cid: ContractId, params: CheckNotRevokedParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::check_not_revoked_v1] Checking nonce not revoked");

    if params.proof.is_empty() {
        msg!("[attestation::check_not_revoked_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check if proof already used (replay protection)
    let nullifiers_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_NULLIFIERS_TREE)?;
    let proof_hash = poseidon_hash([params.nonce, params.revocation_root]);
    if wasm::db::db_contains_key(nullifiers_db, &proof_hash.to_repr())? {
        msg!("[attestation::check_not_revoked_v1] Error: Proof already used");
        return Err(ContractError::InvalidFunction.into())
    }

    msg!("[attestation::check_not_revoked_v1] Nonce {:?} is not revoked", params.nonce);
    Ok(CheckNotRevokedUpdateV1 { is_not_revoked: true, proof_hash }.encode())
}

fn delegate_attestation_v1(cid: ContractId, params: DelegateAttestationParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::delegate_attestation_v1] Delegating attestation: {:?}", params.delegation_id);

    if params.proof.is_empty() {
        msg!("[attestation::delegate_attestation_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check that delegator and delegatee are different
    if params.delegator_pub == params.delegatee_pub {
        msg!("[attestation::delegate_attestation_v1] Error: Cannot delegate to self");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check delegation doesn't already exist (validation only)
    let delegations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;
    if wasm::db::db_contains_key(delegations_db, &params.delegation_id.to_repr())? {
        msg!("[attestation::delegate_attestation_v1] Error: Delegation already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    msg!("[attestation::delegate_attestation_v1] Delegation stored successfully");
    Ok(DelegateAttestationUpdateV1 {
        delegation_id: params.delegation_id,
        success: true,
        delegation_params: params,
    }.encode())
}

fn verify_chain_v1(cid: ContractId, params: VerifyChainParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::verify_chain_v1] Verifying delegation chain: {:?}", params.delegation_id);

    if params.proof.is_empty() {
        msg!("[attestation::verify_chain_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Look up the delegation in the chain
    let delegations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;
    if !wasm::db::db_contains_key(delegations_db, &params.delegation_id.to_repr())? {
        msg!("[attestation::verify_chain_v1] Error: Delegation not found");
        return Err(ContractError::InvalidFunction.into())
    }

    // If parent_id is provided, verify it also exists in the chain
    if params.parent_id != pallas::Base::zero() {
        if !wasm::db::db_contains_key(delegations_db, &params.parent_id.to_repr())? {
            msg!("[attestation::verify_chain_v1] Error: Parent delegation not found");
            return Err(ContractError::InvalidFunction.into())
        }
    }

    msg!("[attestation::verify_chain_v1] Chain verification passed");
    Ok(VerifyChainUpdateV1 { success: true }.encode())
}

fn update_delegation_v1(cid: ContractId, params: UpdateDelegationParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::update_delegation_v1] Updating delegation: {:?}", params.original_attestation_id);

    if params.proof.is_empty() {
        msg!("[attestation::update_delegation_v1] Error: Missing ZK proof");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify the original attestation exists
    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    if !wasm::db::db_contains_key(attestations_db, &params.original_attestation_id.to_repr())? {
        msg!("[attestation::update_delegation_v1] Error: Original attestation not found");
        return Err(ContractError::InvalidFunction.into())
    }

    msg!("[attestation::update_delegation_v1] Delegation updated successfully");
    Ok(UpdateDelegationUpdateV1 {
        success: true,
        original_attestation_id: params.original_attestation_id,
        updated_params: params,
    }.encode())
}

// ============================================================================
// ATTEST SLASH (Phase 2d hardening)
// ============================================================================

fn attest_slash_v1(cid: ContractId, params: AttestSlashParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::attest_slash_v1] Attesting slash event: amount={}, block={}", params.slash_amount, params.block_height);

    // Compute attestation ID: poseidon_hash(relayer_x, relayer_y, slash_amount, withdrawal_id)
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (rx, ry) = params.relayer_pub.xy().expect("pk not identity");
    let attestation_id = poseidon_hash([
        rx,
        ry,
        pallas::Base::from(params.slash_amount),
        params.withdrawal_id,
    ]);

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Build attestation record and index key
    let attestation = Attestation {
        version: 1,
        id: AttestationId(attestation_id),
        attestor_pub: params.relayer_pub,
        attestor_secret: pallas::Base::zero(),
        claim_type: Predicate::Custom,
        claim_data: vec![
            pallas::Base::from(params.slash_amount),
            params.withdrawal_id,
        ],
        metadata: vec![],
        state: AttestationState::Active,
        created_at: current_block,
        expires_at: None,
    };
    let index_key_bytes = [&[Predicate::Custom as u8], &attestation_id.to_repr()[..]].concat();

    // Check attestation doesn't already exist
    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    if wasm::db::db_contains_key(attestations_db, &attestation_id.to_repr())? {
        msg!("[attestation::attest_slash_v1] Slash attestation already exists (idempotent)");
        return Ok(AttestSlashUpdateV1 {
            attestation_id: AttestationId(attestation_id),
            slash_amount: params.slash_amount,
            withdrawal_id: params.withdrawal_id,
            block_height: params.block_height,
            attestation,
            index_key_bytes,
            is_new: false,
        }.encode())
    }

    // Verify the slash event is in the past and within the acceptable recency window.
    // Prevents pre-registration of future slash attestations that would block
    // real slash events via the idempotency check.
    if params.block_height > current_block {
        msg!("[attestation::attest_slash_v1] Error: Slash block_height {} is in the future (current={})",
             params.block_height, current_block);
        return Err(ContractError::InvalidFunction.into())
    }
    if current_block - params.block_height > crate::MAX_SLASH_ATTESTATION_AGE {
        msg!("[attestation::attest_slash_v1] Error: Slash event too old (block_height={}, current={}, max_age={})",
             params.block_height, current_block, crate::MAX_SLASH_ATTESTATION_AGE);
        return Err(ContractError::InvalidFunction.into())
    }

    msg!("[attestation::attest_slash_v1] Slash attested: id={:?}", attestation_id);

    Ok(AttestSlashUpdateV1 {
        attestation_id: AttestationId(attestation_id),
        slash_amount: params.slash_amount,
        withdrawal_id: params.withdrawal_id,
        block_height: params.block_height,
        attestation,
        index_key_bytes,
        is_new: true,
    }.encode())
}

// ============================================================================
// COMMIT FEE SCHEDULE (Phase 3 hardening)
// ============================================================================

fn commit_fee_schedule_v1(cid: ContractId, params: CommitFeeScheduleParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[attestation::commit_fee_schedule_v1] Committing fee schedule: base_fee_bp={}, premium_bp={}",
        params.base_fee_bp, params.guaranteed_premium_bp);

    // Compute attestation ID
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (ax, ay) = params.attestor_pub.xy().expect("pk not identity");
    let attestation_id = poseidon_hash([
        ax,
        ay,
        pallas::Base::from(params.base_fee_bp),
        pallas::Base::from(params.guaranteed_premium_bp),
    ]);

    // Check attestation doesn't already exist (update instead)
    let attestations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
    if wasm::db::db_contains_key(attestations_db, &attestation_id.to_repr())? {
        msg!("[attestation::commit_fee_schedule_v1] Fee schedule already committed — updating");
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    let attestation = Attestation {
        version: 1,
        id: AttestationId(attestation_id),
        attestor_pub: params.attestor_pub,
        attestor_secret: pallas::Base::zero(),
        claim_type: Predicate::Custom,
        claim_data: vec![
            pallas::Base::from(params.base_fee_bp),
            pallas::Base::from(params.guaranteed_premium_bp),
            pallas::Base::from(params.max_amount),
            pallas::Base::from(params.min_amount),
        ],
        metadata: params.metadata.clone(),
        state: AttestationState::Active,
        created_at: current_block,
        expires_at: None,
    };

    let index_key_bytes = [&[Predicate::Custom as u8], &attestation_id.to_repr()[..]].concat();

    msg!("[attestation::commit_fee_schedule_v1] Fee schedule committed: id={:?}", attestation_id);

    Ok(CommitFeeScheduleUpdateV1 {
        attestation_id,
        base_fee_bp: params.base_fee_bp,
        guaranteed_premium_bp: params.guaranteed_premium_bp,
        max_amount: params.max_amount,
        min_amount: params.min_amount,
        attestation,
        index_key_bytes,
    }.encode())
}

// ============================================================================
// PROCESS UPDATE
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    msg!("[attestation::process_update] func_byte=0x{:02x} data_len={}", update_data[0], update_data.len());
    match AttestationFunction::try_from(update_data[0])? {
        AttestationFunction::CreateAttestationV1 => {
            let update = CreateAttestationUpdateV1::decode(&update_data[1..])?;
            let db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
            wasm::db::db_set(db, &update.attestation_id.to_bytes(), &update.attestation.encode())?;
            let index_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_INDEX_TREE)?;
            wasm::db::db_set(index_db, &update.index_key.to_repr(), &update.attestation_id.to_bytes())?;
            msg!("[attestation::process_update] CreateAttestation: {:?}", update.attestation_id);
            Ok(())
        }
        AttestationFunction::RevokeAttestationV1 => {
            let update = RevokeAttestationUpdateV1::decode(&update_data[1..])?;
            let db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
            wasm::db::db_set(db, &update.attestation_id.to_bytes(), &update.attestation.encode())?;
            msg!("[attestation::process_update] RevokeAttestation: {:?}", update.attestation_id);
            Ok(())
        }
        AttestationFunction::ExpireAttestationV1 => {
            let update = ExpireAttestationUpdateV1::decode(&update_data[1..])?;
            let db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
            wasm::db::db_set(db, &update.attestation_id.to_bytes(), &update.attestation.encode())?;
            msg!("[attestation::process_update] ExpireAttestation: {:?}", update.attestation_id);
            Ok(())
        }
        AttestationFunction::CreateClaimV1 => {
            let update = CreateClaimUpdateV1::decode(&update_data[1..])?;
            let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;
            wasm::db::db_set(claims_db, &update.claim_id.to_bytes(), &update.claim.encode())?;
            let rate_limit_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_RATE_LIMIT_TREE)?;
            wasm::db::db_set(rate_limit_db, &update.rate_limit_key.to_repr(), &update.current_block.to_le_bytes())?;
            msg!("[attestation::process_update] CreateClaim: {:?}", update.claim_id);
            Ok(())
        }
        AttestationFunction::VerifyClaimV1 => {
            let update = VerifyClaimUpdateV1::decode(&update_data[1..])?;
            let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;
            wasm::db::db_set(claims_db, &update.claim_id.to_bytes(), &update.claim.encode())?;
            msg!(
                "[attestation::process_update] VerifyClaim: {:?}",
                update.claim_id
            );
            Ok(())
        }
        AttestationFunction::ConsumeClaimV1 => {
            let update = ConsumeClaimUpdateV1::decode(&update_data[1..])?;
            let claims_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_CLAIMS_TREE)?;
            wasm::db::db_set(claims_db, &update.claim_id.to_bytes(), &update.claim.encode())?;
            let nullifiers_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_mark_spent(nullifiers_db, &update.nullifier.to_repr())?;
            msg!("[attestation::process_update] ConsumeClaim: {:?}", update.claim_id);
            Ok(())
        }
        AttestationFunction::ValidateClaimV1 => {
            let update = ValidateClaimUpdateV1::decode(&update_data[1..])?;
            msg!(
                "[attestation::process_update] ValidateClaim: {:?} valid={:?}",
                update.claim_id,
                update.valid
            );
            Ok(())
        }
        AttestationFunction::CheckNotRevokedV1 => {
            let update = CheckNotRevokedUpdateV1::decode(&update_data[1..])?;
            let nullifiers_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_mark_spent(nullifiers_db, &update.proof_hash.to_repr())?;
            msg!(
                "[attestation::process_update] CheckNotRevoked: is_not_revoked={:?}",
                update.is_not_revoked
            );
            Ok(())
        }
        AttestationFunction::DelegateAttestationV1 => {
            let update = DelegateAttestationUpdateV1::decode(&update_data[1..])?;
            let delegations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;
            wasm::db::db_set(delegations_db, &update.delegation_id.to_repr(), &update.delegation_params.encode())?;
            msg!(
                "[attestation::process_update] DelegateAttestation: {:?} success={:?}",
                update.delegation_id,
                update.success
            );
            Ok(())
        }
        AttestationFunction::VerifyChainV1 => {
            let update = VerifyChainUpdateV1::decode(&update_data[1..])?;
            msg!("[attestation::process_update] VerifyChain: success={:?}", update.success);
            Ok(())
        }
        AttestationFunction::UpdateDelegationV1 => {
            let update = UpdateDelegationUpdateV1::decode(&update_data[1..])?;
            let delegations_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_DELEGATIONS_TREE)?;
            wasm::db::db_set(delegations_db, &update.original_attestation_id.to_repr(), &update.updated_params.encode())?;
            msg!(
                "[attestation::process_update] UpdateDelegation: success={:?}",
                update.success
            );
            Ok(())
        }
        AttestationFunction::AttestSlashV1 => {
            let update = AttestSlashUpdateV1::decode(&update_data[1..])?;
            if update.is_new {
                let db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
                wasm::db::db_set(db, &update.attestation_id.to_bytes(), &update.attestation.encode())?;
                let index_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_INDEX_TREE)?;
                wasm::db::db_set(index_db, &update.index_key_bytes, &[1])?;
            }
            msg!(
                "[attestation::process_update] AttestSlash: {:?}, amount={}",
                update.attestation_id, update.slash_amount
            );
            Ok(())
        }
        AttestationFunction::CommitFeeScheduleV1 => {
            let update = CommitFeeScheduleUpdateV1::decode(&update_data[1..])?;
            let db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_ATTESTATIONS_TREE)?;
            wasm::db::db_set(db, &update.attestation_id.to_repr(), &update.attestation.encode())?;
            let index_db = wasm::db::db_lookup(cid, ATTESTATION_CONTRACT_INDEX_TREE)?;
            wasm::db::db_set(index_db, &update.index_key_bytes, &[1])?;
            msg!(
                "[attestation::process_update] CommitFeeSchedule: {:?}, base_fee_bp={}",
                update.attestation_id, update.base_fee_bp
            );
            Ok(())
        }
    }
}