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

//! Insurance Contract Entrypoint
//!
//! ZK insurance contract with full policy lifecycle management.
//!
//! ## Policy State Machine
//!
//! ```text
//! Created -> Active -> Expired
//!    |          |
//!    |          +-> Claimed -> Approved -> Paid
//!    |                         |
//!    |                         +-> Rejected (back to Active)
//!    +-> Cancelled
//! ```
//!
//! ## Functions
//!
//! | Function | Purpose |
//! |----------|---------|
//! | CreatePolicyV1 | Create policy with coverage, premium, period |
//! | ActivatePolicyV1 | Activate after premium paid |
//! | FileClaimV1 | File claim during coverage |
//! | ApproveClaimV1 | Approve with `approved = loss * coverage_ratio / 10000` |
//! | RejectClaimV1 | Reject pending claim |
//! | PayClaimV1 | Execute payout |
//! | CancelPolicyV1 | Cancel and refund |

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::error::InsuranceError;
use crate::model::*;
use crate::InsuranceFunction;

// Database trees
const POLICIES_TREE: &str = "policies";
const CLAIMS_TREE: &str = "claims";

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize the insurance contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    wasm::db::db_init(cid, POLICIES_TREE)?;
    wasm::db::db_init(cid, CLAIMS_TREE)?;
    msg!("[insurance::init_contract] Insurance contract initialized");
    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

/// Process instruction (verify state transition and produce update)
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = InsuranceFunction::try_from(self_.data[0])?;

    let update_data = match func {
        InsuranceFunction::CreatePolicyV1 => {
            create_policy_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceFunction::ActivatePolicyV1 => {
            activate_policy_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceFunction::FileClaimV1 => {
            file_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceFunction::ApproveClaimV1 => {
            approve_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceFunction::RejectClaimV1 => {
            reject_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceFunction::PayClaimV1 => {
            pay_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceFunction::CancelPolicyV1 => {
            cancel_policy_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Process update (write new state after verification)
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match InsuranceFunction::try_from(update_data[0])? {
        InsuranceFunction::CreatePolicyV1 => {
            let update: CreatePolicyUpdateV1 = deserialize(&update_data[1..])?;
            create_policy_process_update_v1(cid, update)
        }
        InsuranceFunction::ActivatePolicyV1 => {
            let update: ActivatePolicyUpdateV1 = deserialize(&update_data[1..])?;
            activate_policy_process_update_v1(cid, update)
        }
        InsuranceFunction::FileClaimV1 => {
            let update: FileClaimUpdateV1 = deserialize(&update_data[1..])?;
            file_claim_process_update_v1(cid, update)
        }
        InsuranceFunction::ApproveClaimV1 => {
            let update: ApproveClaimUpdateV1 = deserialize(&update_data[1..])?;
            approve_claim_process_update_v1(cid, update)
        }
        InsuranceFunction::RejectClaimV1 => {
            let update: RejectClaimUpdateV1 = deserialize(&update_data[1..])?;
            reject_claim_process_update_v1(cid, update)
        }
        InsuranceFunction::PayClaimV1 => {
            let update: PayClaimUpdateV1 = deserialize(&update_data[1..])?;
            pay_claim_process_update_v1(cid, update)
        }
        InsuranceFunction::CancelPolicyV1 => {
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CreatePolicyParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance::create_policy] Creating policy for: {:?}", params.policyholder);

    // Validate coverage period
    if params.end_block <= params.start_block {
        return Err(InsuranceError::InvalidCoveragePeriod.into())
    }

    // Validate premium
    if params.premium == 0 {
        return Err(InsuranceError::InsufficientPremium.into())
    }

    // Derive policy ID from policy details
    let policy_id = Policy::derive_id(
        &params.policyholder,
        params.details_hash,
        params.coverage_amount,
        params.premium,
        params.start_block,
        params.end_block,
    );

    // Check policy doesn't already exist
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut key_bytes = vec![];
    policy_id.encode(&mut key_bytes)?;
    if wasm::db::db_contains_key(db, &key_bytes)? {
        return Err(InsuranceError::PolicyAlreadyExists.into())
    }

    let update = CreatePolicyUpdateV1 {
        policy_id,
        policyholder: params.policyholder,
        details_hash: params.details_hash,
        coverage_amount: params.coverage_amount,
        premium: params.premium,
        coverage_ratio: 10000, // Default to 100% coverage ratio
        payment_token: params.payment_token,
        start_block: params.start_block,
        end_block: params.end_block,
    };

    msg!(
        "[insurance::create_policy] Policy {:?} created with coverage: {}",
        update.policy_id,
        update.coverage_amount
    );

    let mut update_bytes = vec![];
    update.encode(&mut update_bytes)?;
    Ok(update_bytes)
}

fn create_policy_process_update_v1(cid: ContractId, update: CreatePolicyUpdateV1) -> ContractResult {
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;

    let policy = Policy {
        id: update.policy_id,
        policyholder: update.policyholder,
        details_hash: update.details_hash,
        coverage_amount: update.coverage_amount,
        premium: update.premium,
        coverage_ratio: update.coverage_ratio,
        payment_token: update.payment_token,
        start_block: update.start_block,
        end_block: update.end_block,
        status: PolicyStatus::Created,
        total_claims: 0,
        total_payouts: 0,
    };

    let mut key_bytes = vec![];
    update.policy_id.encode(&mut key_bytes)?;
    let mut value_bytes = vec![];
    policy.encode(&mut value_bytes)?;
    wasm::db::db_set(db, &key_bytes, &value_bytes)?;
    msg!("[insurance::create_policy::update] Policy stored");

    Ok(())
}

// =============================================================================
// ACTIVATE POLICY
// =============================================================================

fn activate_policy_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: ActivatePolicyParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance::activate_policy] Activating policy {:?}", params.policy_id);

    // Look up policy
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut key_bytes = vec![];
    params.policy_id.encode(&mut key_bytes)?;
    let policy_data = match wasm::db::db_get(db, &key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::PolicyNotFound.into()),
    };
    let mut policy: Policy = deserialize(&policy_data)?;

    // Check policy is in Created state
    if policy.status != PolicyStatus::Created {
        return Err(InsuranceError::InvalidPolicyState.into())
    }

    // Check coverage period hasn't started yet
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= policy.start_block {
        return Err(InsuranceError::CoverageAlreadyStarted.into())
    }

    // Update policy state
    policy.status = PolicyStatus::Active;

    let update = ActivatePolicyUpdateV1 {
        policy_id: params.policy_id,
    };

    let mut value_bytes = vec![];
    policy.encode(&mut value_bytes)?;
    wasm::db::db_set(db, &key_bytes, &value_bytes)?;
    msg!("[insurance::activate_policy] Policy {:?} activated", params.policy_id);

    let mut update_bytes = vec![];
    update.encode(&mut update_bytes)?;
    Ok(update_bytes)
}

fn activate_policy_process_update_v1(_cid: ContractId, _update: ActivatePolicyUpdateV1) -> ContractResult {
    msg!("[insurance::activate_policy::update] Policy activation confirmed");
    Ok(())
}

// =============================================================================
// FILE CLAIM
// =============================================================================

fn file_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: FileClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance::file_claim] Filing claim for policy {:?}", params.policy_id);

    // Look up policy
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut key_bytes = vec![];
    params.policy_id.encode(&mut key_bytes)?;
    let policy_data = match wasm::db::db_get(db, &key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::PolicyNotFound.into()),
    };
    let mut policy: Policy = deserialize(&policy_data)?;

    // Check policy is Active
    if policy.status != PolicyStatus::Active {
        return Err(InsuranceError::PolicyNotActive.into())
    }

    // Check coverage period is still valid
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= policy.end_block {
        return Err(InsuranceError::CoverageAlreadyStarted.into())
    }

    // Validate claim amount
    if params.claim_amount == 0 {
        return Err(InsuranceError::InvalidClaimAmount.into())
    }

    // Claim amount cannot exceed coverage
    if params.claim_amount > policy.coverage_amount {
        return Err(InsuranceError::CoverageRatioExceeded.into())
    }

    // Derive claim ID
    let claim_id = Claim::derive_id(params.policy_id, params.claim_amount, params.details_hash, current_block);

    // Update policy state
    policy.status = PolicyStatus::Claimed;
    policy.total_claims += 1;

    let update = FileClaimUpdateV1 {
        claim_id,
        policy_id: params.policy_id,
        claim_amount: params.claim_amount,
        details_hash: params.details_hash,
        filed_at_block: current_block,
    };

    let mut value_bytes = vec![];
    policy.encode(&mut value_bytes)?;
    wasm::db::db_set(db, &key_bytes, &value_bytes)?;
    msg!(
        "[insurance::file_claim] Claim {:?} filed for policy {:?}",
        claim_id,
        params.policy_id
    );

    let mut update_bytes = vec![];
    update.encode(&mut update_bytes)?;
    Ok(update_bytes)
}

fn file_claim_process_update_v1(cid: ContractId, update: FileClaimUpdateV1) -> ContractResult {
    let db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;

    let claim = Claim {
        id: update.claim_id,
        policy_id: update.policy_id,
        claim_amount: update.claim_amount,
        details_hash: update.details_hash,
        verified_loss: 0,
        approved_amount: 0,
        status: ClaimStatus::Pending,
        filed_at_block: update.filed_at_block,
        processed_at_block: None,
    };

    let mut key_bytes = vec![];
    update.claim_id.encode(&mut key_bytes)?;
    let mut value_bytes = vec![];
    claim.encode(&mut value_bytes)?;
    wasm::db::db_set(db, &key_bytes, &value_bytes)?;
    msg!("[insurance::file_claim::update] Claim stored");

    Ok(())
}

// =============================================================================
// APPROVE CLAIM
// =============================================================================

fn approve_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: ApproveClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance::approve_claim] Approving claim {:?}", params.claim_id);

    // Look up claim
    let claims_db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;
    let mut claim_key_bytes = vec![];
    params.claim_id.encode(&mut claim_key_bytes)?;
    let claim_data = match wasm::db::db_get(claims_db, &claim_key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::ClaimNotFound.into()),
    };
    let mut claim: Claim = deserialize(&claim_data)?;

    // Check claim is Pending
    if claim.status != ClaimStatus::Pending {
        return Err(InsuranceError::ClaimAlreadyProcessed.into())
    }

    // Look up policy to get coverage ratio
    let policies_db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy_key_bytes = vec![];
    claim.policy_id.encode(&mut policy_key_bytes)?;
    let policy_data = match wasm::db::db_get(policies_db, &policy_key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::PolicyNotFound.into()),
    };
    let policy: Policy = deserialize(&policy_data)?;

    // Calculate approved amount using coverage ratio
    // Formula: approved = verified_loss * coverage_ratio / 10000
    // Using base_div in ZK circuit for privacy-preserving calculation
    let approved_amount = calculate_approved_amount(params.verified_loss, params.coverage_ratio)?;

    // Verify approved amount doesn't exceed claim amount
    if approved_amount > claim.claim_amount {
        return Err(InsuranceError::CoverageRatioExceeded.into())
    }

    // Verify approved amount doesn't exceed policy coverage
    if approved_amount > policy.coverage_amount {
        return Err(InsuranceError::CoverageRatioExceeded.into())
    }

    // Update claim
    claim.verified_loss = params.verified_loss;
    claim.approved_amount = approved_amount;
    claim.status = ClaimStatus::Approved;

    let update = ApproveClaimUpdateV1 {
        claim_id: params.claim_id,
        policy_id: claim.policy_id,
        verified_loss: params.verified_loss,
        coverage_ratio: params.coverage_ratio,
        approved_amount,
    };

    let mut value_bytes = vec![];
    claim.encode(&mut value_bytes)?;
    wasm::db::db_set(claims_db, &claim_key_bytes, &value_bytes)?;
    msg!(
        "[insurance::approve_claim] Claim {:?} approved for amount: {}",
        params.claim_id,
        approved_amount
    );

    let mut update_bytes = vec![];
    update.encode(&mut update_bytes)?;
    Ok(update_bytes)
}

fn approve_claim_process_update_v1(_cid: ContractId, update: ApproveClaimUpdateV1) -> ContractResult {
    msg!(
        "[insurance::approve_claim::update] Claim approval confirmed: {}",
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: RejectClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance::reject_claim] Rejecting claim {:?}", params.claim_id);

    // Look up claim
    let claims_db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;
    let mut claim_key_bytes = vec![];
    params.claim_id.encode(&mut claim_key_bytes)?;
    let claim_data = match wasm::db::db_get(claims_db, &claim_key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::ClaimNotFound.into()),
    };
    let mut claim: Claim = deserialize(&claim_data)?;

    // Check claim is Pending
    if claim.status != ClaimStatus::Pending {
        return Err(InsuranceError::ClaimAlreadyProcessed.into())
    }

    // Update claim status
    claim.status = ClaimStatus::Rejected;

    let update = RejectClaimUpdateV1 {
        claim_id: params.claim_id,
        policy_id: claim.policy_id,
    };

    let mut value_bytes = vec![];
    claim.encode(&mut value_bytes)?;
    wasm::db::db_set(claims_db, &claim_key_bytes, &value_bytes)?;

    // Also restore policy state to Active
    let policies_db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy_key_bytes = vec![];
    claim.policy_id.encode(&mut policy_key_bytes)?;
    let policy_data = match wasm::db::db_get(policies_db, &policy_key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::PolicyNotFound.into()),
    };
    let mut policy: Policy = deserialize(&policy_data)?;

    policy.status = PolicyStatus::Active;
    let mut policy_value_bytes = vec![];
    policy.encode(&mut policy_value_bytes)?;
    wasm::db::db_set(policies_db, &policy_key_bytes, &policy_value_bytes)?;

    msg!("[insurance::reject_claim] Claim {:?} rejected", params.claim_id);

    let mut update_bytes = vec![];
    update.encode(&mut update_bytes)?;
    Ok(update_bytes)
}

fn reject_claim_process_update_v1(_cid: ContractId, _update: RejectClaimUpdateV1) -> ContractResult {
    msg!("[insurance::reject_claim::update] Claim rejection confirmed");
    Ok(())
}

// =============================================================================
// PAY CLAIM
// =============================================================================

fn pay_claim_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: PayClaimParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance::pay_claim] Paying claim {:?}", params.claim_id);

    // Look up claim
    let claims_db = wasm::db::db_lookup(cid, CLAIMS_TREE)?;
    let mut claim_key_bytes = vec![];
    params.claim_id.encode(&mut claim_key_bytes)?;
    let claim_data = match wasm::db::db_get(claims_db, &claim_key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::ClaimNotFound.into()),
    };
    let mut claim: Claim = deserialize(&claim_data)?;

    // Check claim is Approved
    if claim.status != ClaimStatus::Approved {
        return Err(InsuranceError::ClaimAlreadyProcessed.into())
    }

    // Verify payout amount matches approved amount
    if params.payout_amount != claim.approved_amount {
        return Err(InsuranceError::CoverageRatioExceeded.into())
    }

    // Update claim status
    claim.status = ClaimStatus::Paid;
    claim.processed_at_block = Some(wasm::util::get_verifying_block_height()? as u64);

    let update = PayClaimUpdateV1 {
        claim_id: params.claim_id,
        payout_amount: params.payout_amount,
    };

    let mut claim_value_bytes = vec![];
    claim.encode(&mut claim_value_bytes)?;
    wasm::db::db_set(claims_db, &claim_key_bytes, &claim_value_bytes)?;

    // Update policy totals
    let policies_db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut policy_key_bytes = vec![];
    claim.policy_id.encode(&mut policy_key_bytes)?;
    let policy_data = match wasm::db::db_get(policies_db, &policy_key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::PolicyNotFound.into()),
    };
    let mut policy: Policy = deserialize(&policy_data)?;

    policy.total_payouts = policy.total_payouts.saturating_add(params.payout_amount);
    let mut policy_value_bytes = vec![];
    policy.encode(&mut policy_value_bytes)?;
    wasm::db::db_set(policies_db, &policy_key_bytes, &policy_value_bytes)?;

    msg!(
        "[insurance::pay_claim] Claim {:?} paid: {}",
        params.claim_id,
        params.payout_amount
    );

    let mut update_bytes = vec![];
    update.encode(&mut update_bytes)?;
    Ok(update_bytes)
}

fn pay_claim_process_update_v1(_cid: ContractId, _update: PayClaimUpdateV1) -> ContractResult {
    msg!("[insurance::pay_claim::update] Claim payout confirmed");
    Ok(())
}

// =============================================================================
// CANCEL POLICY
// =============================================================================

fn cancel_policy_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: CancelPolicyParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[insurance::cancel_policy] Cancelling policy {:?}", params.policy_id);

    // Look up policy
    let db = wasm::db::db_lookup(cid, POLICIES_TREE)?;
    let mut key_bytes = vec![];
    params.policy_id.encode(&mut key_bytes)?;
    let policy_data = match wasm::db::db_get(db, &key_bytes)? {
        Some(data) => data,
        None => return Err(InsuranceError::PolicyNotFound.into()),
    };
    let mut policy: Policy = deserialize(&policy_data)?;

    // Check policy is in Created or Active state
    if policy.status != PolicyStatus::Created && policy.status != PolicyStatus::Active {
        return Err(InsuranceError::InvalidPolicyState.into())
    }

    // Calculate refund (premium paid minus any payouts)
    let refund_amount = policy.premium.saturating_sub(policy.total_payouts);

    // Update policy state
    policy.status = PolicyStatus::Cancelled;

    let update = CancelPolicyUpdateV1 {
        policy_id: params.policy_id,
        refund_amount,
    };

    let mut value_bytes = vec![];
    policy.encode(&mut value_bytes)?;
    wasm::db::db_set(db, &key_bytes, &value_bytes)?;
    msg!(
        "[insurance::cancel_policy] Policy {:?} cancelled, refund: {}",
        params.policy_id,
        refund_amount
    );

    let mut update_bytes = vec![];
    update.encode(&mut update_bytes)?;
    Ok(update_bytes)
}

fn cancel_policy_process_update_v1(_cid: ContractId, _update: CancelPolicyUpdateV1) -> ContractResult {
    msg!("[insurance::cancel_policy::update] Policy cancellation confirmed");
    Ok(())
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Calculate approved claim amount based on verified loss and coverage ratio
///
/// Formula: approved = verified_loss * coverage_ratio / 10000
///
/// Note: This is the same calculation done in the ZK circuit using base_div.
/// This helper exists for cases where the calculation must be done in Rust
/// (e.g., when setting up circuit witnesses).
fn calculate_approved_amount(verified_loss: u64, coverage_ratio: u64) -> ContractResult<u64> {
    // coverage_ratio is in basis points (e.g., 8000 = 80%)
    // approved = verified_loss * coverage_ratio / 10000

    // Check for overflow: verified_loss * coverage_ratio
    let (product, overflowed) = verified_loss.overflowing_mul(coverage_ratio);
    if overflowed {
        return Err(InsuranceError::ArithmeticOverflow.into())
    }

    // Divide by 10000 using integer division
    let approved = product / 10000;

    Ok(approved)
}