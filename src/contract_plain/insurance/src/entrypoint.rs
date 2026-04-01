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

//! Plain Insurance Contract Entrypoint
//!
//! # Architecture
//!
//! This contract uses a hybrid ZK/plain approach:
//!
//! | Operation | Method | Why |
//! |-----------|--------|-----|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Policy commitment | ZK (Pedersen) | Privacy-preserving |
//! | Premium calculation | Native Rust | Needs `base_div` (not in ZK) |
//! | Claims verification | Hybrid | ZK for sound parts, plain for complex |
//!
//! # Privacy
//!
//! This is a **partial transparency** contract. Most state is public on-chain.
//! Actual personal details are NOT stored on-chain (only hashes).
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.

use darkfi_sdk::{
    crypto::{poseidon_hash, schnorr::SchnorrPublic, ContractId},
    dark_tree::DarkLeaf,
    error::GenericResult,
    msg, wasm, ContractCall,
};
use darkfi_sdk::pasta::pallas::Base;
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::error::InsurancePlainError;
use crate::model::{
    ActivatePolicyParamsV1, ActivatePolicyUpdateV1, ApproveClaimParamsV1, ApproveClaimUpdateV1,
    CancelPolicyParamsV1, CancelPolicyUpdateV1, Claim, ClaimStatus, CreatePolicyParamsV1,
    CreatePolicyUpdateV1, FileClaimParamsV1, FileClaimUpdateV1, PayClaimParamsV1, PayClaimUpdateV1,
    Policy, PolicyState, RejectClaimParamsV1, RejectClaimUpdateV1,
};
use crate::InsurancePlainFunction;

// Database trees
const POLICIES_TREE: &str = "policies";
const CLAIMS_TREE: &str = "claims";

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    wasm::db::db_init(cid, POLICIES_TREE)?;
    wasm::db::db_init(cid, CLAIMS_TREE)?;
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
    let func = InsurancePlainFunction::try_from(self_.data[0])?;

    let update_data = match func {
        InsurancePlainFunction::CreatePolicyV1 => {
            create_policy_process_instruction_v1(cid, call_idx, calls)?
        }
        InsurancePlainFunction::ActivatePolicyV1 => {
            activate_policy_process_instruction_v1(cid, call_idx, calls)?
        }
        InsurancePlainFunction::FileClaimV1 => {
            file_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsurancePlainFunction::ApproveClaimV1 => {
            approve_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsurancePlainFunction::RejectClaimV1 => {
            reject_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsurancePlainFunction::PayClaimV1 => {
            pay_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsurancePlainFunction::CancelPolicyV1 => {
            cancel_policy_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> GenericResult<()> {
    match InsurancePlainFunction::try_from(update_data[0])? {
        InsurancePlainFunction::CreatePolicyV1 => {
            let update: CreatePolicyUpdateV1 = deserialize(&update_data[1..])?;
            create_policy_process_update_v1(cid, update)
        }
        InsurancePlainFunction::ActivatePolicyV1 => {
            let update: ActivatePolicyUpdateV1 = deserialize(&update_data[1..])?;
            activate_policy_process_update_v1(cid, update)
        }
        InsurancePlainFunction::FileClaimV1 => {
            let update: FileClaimUpdateV1 = deserialize(&update_data[1..])?;
            file_claim_process_update_v1(cid, update)
        }
        InsurancePlainFunction::ApproveClaimV1 => {
            let update: ApproveClaimUpdateV1 = deserialize(&update_data[1..])?;
            approve_claim_process_update_v1(cid, update)
        }
        InsurancePlainFunction::RejectClaimV1 => {
            let update: RejectClaimUpdateV1 = deserialize(&update_data[1..])?;
            reject_claim_process_update_v1(cid, update)
        }
        InsurancePlainFunction::PayClaimV1 => {
            let update: PayClaimUpdateV1 = deserialize(&update_data[1..])?;
            pay_claim_process_update_v1(cid, update)
        }
        InsurancePlainFunction::CancelPolicyV1 => {
            let update: CancelPolicyUpdateV1 = deserialize(&update_data[1..])?;
            cancel_policy_process_update_v1(cid, update)
        }
    }
}

// =============================================================================
// CREATE POLICY
// =============================================================================

fn create_policy_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CreatePolicyParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_plain::create_policy] Creating policy for: {:?}", params.policyholder);

    // Validate coverage period
    if params.end_block <= params.start_block {
        return Err(InsurancePlainError::InvalidCoveragePeriod.into())
    }

    // Validate premium
    if params.premium_amount == 0 {
        return Err(InsurancePlainError::InsufficientPremium.into())
    }

    // Derive policy ID from policy details
    let policy_id = derive_policy_id(&params);

    // Check policy doesn't already exist
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    if wasm::db::db_contains_key(db, &serialize(&policy_id))? {
        return Err(InsurancePlainError::PolicyAlreadyExists.into())
    }

    // Verify policyholder signature
    // OPCODE PLACEHOLDER: When Schnorr verification is in ZK, policyholder would be constrained
    let mut signature_msg = vec![];
    params.policyholder.x().encode(&mut signature_msg)?;
    params.policyholder.y().encode(&mut signature_msg)?;
    params.details_hash.encode(&mut signature_msg)?;
    params.coverage_amount.encode(&mut signature_msg)?;
    params.premium_amount.encode(&mut signature_msg)?;
    params.start_block.encode(&mut signature_msg)?;
    params.end_block.encode(&mut signature_msg)?;

    if !params.policyholder.verify(&signature_msg, &params.signature) {
        return Err(InsurancePlainError::InvalidSignature.into())
    }

    let update = CreatePolicyUpdateV1 {
        policy_id,
        policyholder: params.policyholder,
        details_hash: params.details_hash,
        coverage_amount: params.coverage_amount,
        premium_paid: params.premium_amount,
        start_block: params.start_block,
        end_block: params.end_block,
    };

    msg!(
        "[insurance_plain::create_policy] Policy {:?} created with coverage: {}",
        update.policy_id,
        update.coverage_amount
    );
    Ok(serialize(&update))
}

fn create_policy_process_update_v1(cid: ContractId, update: CreatePolicyUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;

    let policy = Policy {
        id: update.policy_id,
        policyholder: update.policyholder,
        details_hash: update.details_hash,
        coverage_amount: update.coverage_amount,
        premium_paid: update.premium_paid,
        payment_token: Base::zero(),
        start_block: update.start_block,
        end_block: update.end_block,
        state: PolicyState::Created,
        total_claims: 0,
        total_payouts: 0,
        policyholder_signature: None,
    };

    wasm::db::db_set(db, &serialize(&update.policy_id), &serialize(&policy))?;
    msg!("[insurance_plain::create_policy::update] Policy stored");

    Ok(())
}

// =============================================================================
// ACTIVATE POLICY
// =============================================================================

fn activate_policy_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: ActivatePolicyParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_plain::activate_policy] Activating policy {:?}", params.policy_id);

    // Look up policy
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy: Policy = match wasm::db::db_get(db, &serialize(&params.policy_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::PolicyNotFound.into()),
    };

    // Check policy is in Created state
    if policy.state != PolicyState::Created {
        return Err(InsurancePlainError::InvalidPolicyState.into())
    }

    // Check coverage period hasn't started yet
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= policy.start_block {
        return Err(InsurancePlainError::CoverageAlreadyStarted.into())
    }

    // Verify caller signature
    let mut signature_msg = vec![];
    params.policy_id.encode(&mut signature_msg)?;

    if !params.caller.verify(&signature_msg, &params.signature) {
        return Err(InsurancePlainError::InvalidSignature.into())
    }

    // Update policy state
    policy.state = PolicyState::Active;

    let update = ActivatePolicyUpdateV1 {
        policy_id: params.policy_id,
    };

    wasm::db::db_set(db, &serialize(&params.policy_id), &serialize(&policy))?;
    msg!("[insurance_plain::activate_policy] Policy {:?} activated", params.policy_id);
    Ok(serialize(&update))
}

fn activate_policy_process_update_v1(_cid: ContractId, _update: ActivatePolicyUpdateV1) -> GenericResult<()> {
    msg!("[insurance_plain::activate_policy::update] Policy activation confirmed");
    Ok(())
}

// =============================================================================
// FILE CLAIM
// =============================================================================

fn file_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: FileClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_plain::file_claim] Filing claim for policy {:?}", params.policy_id);

    // Look up policy
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy: Policy = match wasm::db::db_get(db, &serialize(&params.policy_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::PolicyNotFound.into()),
    };

    // Check policy is Active
    if policy.state != PolicyState::Active {
        return Err(InsurancePlainError::PolicyNotActive.into())
    }

    // Check coverage period is still valid
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= policy.end_block {
        return Err(InsurancePlainError::CoverageAlreadyStarted.into())
    }

    // Validate claim amount
    if params.claim_amount == 0 {
        return Err(InsurancePlainError::InvalidClaimAmount.into())
    }

    // Claim amount cannot exceed coverage
    if params.claim_amount > policy.coverage_amount {
        return Err(InsurancePlainError::CoverageRatioExceeded.into())
    }

    // Verify policyholder signature
    let mut signature_msg = vec![];
    params.policy_id.encode(&mut signature_msg)?;
    params.claim_amount.encode(&mut signature_msg)?;
    params.details_hash.encode(&mut signature_msg)?;

    if !policy.policyholder.verify(&signature_msg, &params.signature) {
        return Err(InsurancePlainError::InvalidSignature.into())
    }

    // Derive claim ID
    let claim_id = derive_claim_id(&params, current_block);

    // Update policy state
    policy.state = PolicyState::Claimed;
    policy.total_claims += 1;

    let update = FileClaimUpdateV1 {
        claim_id,
        policy_id: params.policy_id,
        claim_amount: params.claim_amount,
        details_hash: params.details_hash,
        filed_at_block: current_block,
    };

    wasm::db::db_set(db, &serialize(&params.policy_id), &serialize(&policy))?;
    msg!(
        "[insurance_plain::file_claim] Claim {:?} filed for policy {:?}",
        claim_id,
        params.policy_id
    );
    Ok(serialize(&update))
}

fn file_claim_process_update_v1(cid: ContractId, update: FileClaimUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;

    let claim = Claim {
        id: update.claim_id,
        policy_id: update.policy_id,
        claim_amount: update.claim_amount,
        details_hash: update.details_hash,
        verified_loss: 0,
        status: ClaimStatus::Pending,
        filed_at_block: update.filed_at_block,
        processed_at_block: None,
    };

    wasm::db::db_set(db, &serialize(&update.claim_id), &serialize(&claim))?;
    msg!("[insurance_plain::file_claim::update] Claim stored");

    Ok(())
}

// =============================================================================
// APPROVE CLAIM
// =============================================================================

fn approve_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: ApproveClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_plain::approve_claim] Approving claim {:?}", params.claim_id);

    // Look up claim
    let claims_db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;
    let mut claim: Claim = match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::ClaimNotFound.into()),
    };

    // Check claim is Pending
    if claim.status != ClaimStatus::Pending {
        return Err(InsurancePlainError::ClaimAlreadyProcessed.into())
    }

    // Verify approver signature
    let mut signature_msg = vec![];
    params.claim_id.encode(&mut signature_msg)?;
    params.verified_loss.encode(&mut signature_msg)?;
    params.coverage_ratio.encode(&mut signature_msg)?;

    // Note: In a real system, this would be verified against an oracle or DAO public key
    // OPCODE PLACEHOLDER: When oracle/DAO verification is available, verify signature
    // For now, we include the signature in the message for future verification
    let _ = signature_msg;

    // Calculate approved amount using coverage ratio
    // OPCODE PLACEHOLDER: When base_div is in ZK, could use ZK constraints
    // Currently uses native Rust (visible on-chain)
    // Formula: approved = verified_loss * coverage_ratio / 10000
    let approved_amount = calculate_approved_amount(params.verified_loss, params.coverage_ratio)?;

    // Verify approved amount doesn't exceed claim amount
    if approved_amount > claim.claim_amount {
        return Err(InsurancePlainError::CoverageRatioExceeded.into())
    }

    // Update claim
    claim.verified_loss = params.verified_loss;
    claim.status = ClaimStatus::Approved;

    let update = ApproveClaimUpdateV1 {
        claim_id: params.claim_id,
        policy_id: claim.policy_id,
        verified_loss: params.verified_loss,
        coverage_ratio: params.coverage_ratio,
        approved_amount,
    };

    wasm::db::db_set(claims_db, &serialize(&params.claim_id), &serialize(&claim))?;
    msg!(
        "[insurance_plain::approve_claim] Claim {:?} approved for amount: {}",
        params.claim_id,
        approved_amount
    );
    Ok(serialize(&update))
}

fn approve_claim_process_update_v1(cid: ContractId, update: ApproveClaimUpdateV1) -> GenericResult<()> {
    // Update is informational - claim already updated in process_instruction
    msg!(
        "[insurance_plain::approve_claim::update] Claim approval confirmed: {}",
        update.approved_amount
    );
    Ok(())
}

// =============================================================================
// REJECT CLAIM
// =============================================================================

fn reject_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: RejectClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_plain::reject_claim] Rejecting claim {:?}", params.claim_id);

    // Look up claim
    let claims_db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;
    let mut claim: Claim = match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::ClaimNotFound.into()),
    };

    // Check claim is Pending
    if claim.status != ClaimStatus::Pending {
        return Err(InsurancePlainError::ClaimAlreadyProcessed.into())
    }

    // Verify rejector signature
    // OPCODE PLACEHOLDER: When oracle/DAO verification is available, verify signature
    let _ = params.signature;

    // Update claim status
    claim.status = ClaimStatus::Rejected;

    let update = RejectClaimUpdateV1 {
        claim_id: params.claim_id,
    };

    wasm::db::db_set(claims_db, &serialize(&params.claim_id), &serialize(&claim))?;

    // Also restore policy state to Active
    let policies_db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy: Policy = match wasm::db::db_get(policies_db, &serialize(&claim.policy_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::PolicyNotFound.into()),
    };

    policy.state = PolicyState::Active;
    wasm::db::db_set(policies_db, &serialize(&claim.policy_id), &serialize(&policy))?;

    msg!("[insurance_plain::reject_claim] Claim {:?} rejected", params.claim_id);
    Ok(serialize(&update))
}

fn reject_claim_process_update_v1(_cid: ContractId, _update: RejectClaimUpdateV1) -> GenericResult<()> {
    msg!("[insurance_plain::reject_claim::update] Claim rejection confirmed");
    Ok(())
}

// =============================================================================
// PAY CLAIM
// =============================================================================

fn pay_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: PayClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_plain::pay_claim] Paying claim {:?}", params.claim_id);

    // Look up claim
    let claims_db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;
    let mut claim: Claim = match wasm::db::db_get(claims_db, &serialize(&params.claim_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::ClaimNotFound.into()),
    };

    // Check claim is Approved
    if claim.status != ClaimStatus::Approved {
        return Err(InsurancePlainError::ClaimAlreadyProcessed.into())
    }

    // Verify payout amount matches approved amount
    // This would typically involve cross-contract call to money contract
    // OPCODE PLACEHOLDER: When money transfer is available, verify funds moved

    // Verify signature
    // OPCODE PLACEHOLDER: When payment verification is available, verify signature
    let _ = params.signature;

    // Update claim status
    claim.status = ClaimStatus::Paid;
    claim.processed_at_block = Some(wasm::util::get_verifying_block_height()? as u64);

    let update = PayClaimUpdateV1 {
        claim_id: params.claim_id,
        payout_amount: params.payout_amount,
    };

    wasm::db::db_set(claims_db, &serialize(&params.claim_id), &serialize(&claim))?;

    // Update policy totals
    let policies_db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy: Policy = match wasm::db::db_get(policies_db, &serialize(&claim.policy_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::PolicyNotFound.into()),
    };

    policy.total_payouts = policy.total_payouts.saturating_add(params.payout_amount);
    wasm::db::db_set(policies_db, &serialize(&claim.policy_id), &serialize(&policy))?;

    msg!(
        "[insurance_plain::pay_claim] Claim {:?} paid: {}",
        params.claim_id,
        params.payout_amount
    );
    Ok(serialize(&update))
}

fn pay_claim_process_update_v1(_cid: ContractId, _update: PayClaimUpdateV1) -> GenericResult<()> {
    msg!("[insurance_plain::pay_claim::update] Claim payout confirmed");
    Ok(())
}

// =============================================================================
// CANCEL POLICY
// =============================================================================

fn cancel_policy_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CancelPolicyParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance_plain::cancel_policy] Cancelling policy {:?}", params.policy_id);

    // Look up policy
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy: Policy = match wasm::db::db_get(db, &serialize(&params.policy_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(InsurancePlainError::PolicyNotFound.into()),
    };

    // Check policy is in Created or Active state
    if policy.state != PolicyState::Created && policy.state != PolicyState::Active {
        return Err(InsurancePlainError::InvalidPolicyState.into())
    }

    // Verify policyholder signature
    let mut signature_msg = vec![];
    params.policy_id.encode(&mut signature_msg)?;

    if !policy.policyholder.verify(&signature_msg, &params.signature) {
        return Err(InsurancePlainError::InvalidSignature.into())
    }

    // Calculate refund (premium paid minus any claims)
    let refund_amount = policy.premium_paid.saturating_sub(policy.total_payouts);

    // Update policy state
    policy.state = PolicyState::Cancelled;

    let update = CancelPolicyUpdateV1 {
        policy_id: params.policy_id,
        refund_amount,
    };

    wasm::db::db_set(db, &serialize(&params.policy_id), &serialize(&policy))?;
    msg!(
        "[insurance_plain::cancel_policy] Policy {:?} cancelled, refund: {}",
        params.policy_id,
        refund_amount
    );
    Ok(serialize(&update))
}

fn cancel_policy_process_update_v1(_cid: ContractId, _update: CancelPolicyUpdateV1) -> GenericResult<()> {
    msg!("[insurance_plain::cancel_policy::update] Policy cancellation confirmed");
    Ok(())
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Derive a unique policy ID from policy parameters
fn derive_policy_id(params: &CreatePolicyParamsV1) -> Base {
    poseidon_hash([
        params.policyholder.x(),
        params.policyholder.y(),
        params.details_hash,
        Base::from(params.coverage_amount),
        Base::from(params.premium_amount),
        Base::from(params.start_block),
        Base::from(params.end_block),
    ])
}

/// Derive a unique claim ID from claim parameters
fn derive_claim_id(params: &FileClaimParamsV1, current_block: u64) -> Base {
    poseidon_hash([
        params.policy_id,
        Base::from(params.claim_amount),
        params.details_hash,
        Base::from(current_block),
    ])
}

/// Calculate approved claim amount based on verified loss and coverage ratio
/// PRIVACY NOTICE: This calculation is visible on-chain.
/// OPCODE PLACEHOLDER: When base_div is in ZK, this could be private.
fn calculate_approved_amount(verified_loss: u64, coverage_ratio: u64) -> GenericResult<u64> {
    // coverage_ratio is in basis points (e.g., 8000 = 80%)
    // approved = verified_loss * coverage_ratio / 10000
    // Using cross-multiplication to avoid division issues
    let multiplier = coverage_ratio;
    let divisor = 10000u64;

    // Check for overflow: verified_loss * coverage_ratio
    let (product, overflowed) = verified_loss.overflowing_mul(multiplier);
    if overflowed {
        return Err(InsurancePlainError::ArithmeticOverflow.into())
    }

    // Divide by 10000 using integer division
    // This loses precision but is acceptable for insurance
    let approved = product / divisor;

    Ok(approved)
}