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

//! Plain Labor Market Contract Entrypoint
//!
//! # Architecture
//!
//! This contract uses a hybrid ZK/plain approach:
//!
//! | Operation | Method | Why |
//! |-----------|--------|-----|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Payment tracking | Native Rust | Needs arithmetic (visible) |
//! | Time-weighted release | Native Rust | Needs `base_div` (not in ZK) |
//! | Milestone verification | Hybrid | ZK for sound parts, plain for complex |
//!
//! # Privacy
//!
//! This is a **partial transparency** contract. Most state is public on-chain.
//! Actual work content is NOT stored on-chain (only hashes).
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.

use darkfi_sdk::{
    crypto::{poseidon_hash, schnorr::SchnorrPublic, ContractId},
    dark_tree::DarkLeaf,
    error::GenericResult,
    msg, wasm, ContractCall,
};
use darkfi_sdk::pasta::pallas::Base;
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::error::LaborMarketPlainError;
use crate::model::{
    AcceptJobParamsV1, AcceptJobUpdateV1, CancelJobParamsV1, CancelJobUpdateV1,
    ConfirmDeliverableParamsV1, ConfirmDeliverableUpdateV1, CreateJobParamsV1,
    CreateJobUpdateV1, DisputeParamsV1, DisputeUpdateV1, Job, JobState, Milestone,
    RefundParamsV1, RefundUpdateV1, SubmitDeliverableParamsV1, SubmitDeliverableUpdateV1,
};
use crate::LaborMarketPlainFunction;

// Database trees
const JOBS_TREE: &str = "jobs";

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    wasm::db::db_init(cid, JOBS_TREE)?;
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
    let func = LaborMarketPlainFunction::try_from(self_.data[0])?;

    let update_data = match func {
        LaborMarketPlainFunction::CreateJobV1 => {
            create_job_process_instruction_v1(cid, call_idx, calls)?
        }
        LaborMarketPlainFunction::AcceptJobV1 => {
            accept_job_process_instruction_v1(cid, call_idx, calls)?
        }
        LaborMarketPlainFunction::SubmitDeliverableV1 => {
            submit_deliverable_process_instruction_v1(cid, call_idx, calls)?
        }
        LaborMarketPlainFunction::ConfirmDeliverableV1 => {
            confirm_deliverable_process_instruction_v1(cid, call_idx, calls)?
        }
        LaborMarketPlainFunction::DisputeV1 => {
            dispute_process_instruction_v1(cid, call_idx, calls)?
        }
        LaborMarketPlainFunction::CancelV1 => {
            cancel_job_process_instruction_v1(cid, call_idx, calls)?
        }
        LaborMarketPlainFunction::RefundV1 => {
            refund_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> GenericResult<()> {
    match LaborMarketPlainFunction::try_from(update_data[0])? {
        LaborMarketPlainFunction::CreateJobV1 => {
            let update: CreateJobUpdateV1 = deserialize(&update_data[1..])?;
            create_job_process_update_v1(cid, update)
        }
        LaborMarketPlainFunction::AcceptJobV1 => {
            let update: AcceptJobUpdateV1 = deserialize(&update_data[1..])?;
            accept_job_process_update_v1(cid, update)
        }
        LaborMarketPlainFunction::SubmitDeliverableV1 => {
            let update: SubmitDeliverableUpdateV1 = deserialize(&update_data[1..])?;
            submit_deliverable_process_update_v1(cid, update)
        }
        LaborMarketPlainFunction::ConfirmDeliverableV1 => {
            let update: ConfirmDeliverableUpdateV1 = deserialize(&update_data[1..])?;
            confirm_deliverable_process_update_v1(cid, update)
        }
        LaborMarketPlainFunction::DisputeV1 => {
            let update: DisputeUpdateV1 = deserialize(&update_data[1..])?;
            dispute_process_update_v1(cid, update)
        }
        LaborMarketPlainFunction::CancelV1 => {
            let update: CancelJobUpdateV1 = deserialize(&update_data[1..])?;
            cancel_job_process_update_v1(cid, update)
        }
        LaborMarketPlainFunction::RefundV1 => {
            let update: RefundUpdateV1 = deserialize(&update_data[1..])?;
            refund_process_update_v1(cid, update)
        }
    }
}

// =============================================================================
// CREATE JOB
// =============================================================================

fn create_job_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CreateJobParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[labor_market_plain::create_job] Creating job with title hash: {:?}", params.title_hash);

    // Validate deadline
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if params.deadline_block <= current_block {
        return Err(LaborMarketPlainError::InvalidDeadline.into())
    }

    // Validate payment amount
    if params.total_payment == 0 {
        return Err(LaborMarketPlainError::InsufficientPayment.into())
    }

    // Validate milestones
    if params.milestones.is_empty() {
        return Err(LaborMarketPlainError::InvalidDeliverable.into())
    }

    // Derive job ID from job details
    let job_id = derive_job_id(&params, current_block);

    // Check job doesn't already exist
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;
    if wasm::db::db_contains_key(db, &serialize(&job_id))? {
        return Err(LaborMarketPlainError::JobAlreadyExists.into())
    }

    // Verify employer signature
    // OPCODE PLACEHOLDER: When Schnorr verification is in ZK, employer would be constrained
    let mut signature_msg = vec![];
    params.employer.x().encode(&mut signature_msg)?;
    params.employer.y().encode(&mut signature_msg)?;
    params.title_hash.encode(&mut signature_msg)?;
    params.specification_hash.encode(&mut signature_msg)?;
    params.total_payment.encode(&mut signature_msg)?;
    params.deadline_block.encode(&mut signature_msg)?;
    current_block.encode(&mut signature_msg)?;

    if !params.employer.verify(&signature_msg, &params.signature) {
        return Err(LaborMarketPlainError::InvalidSignature.into())
    }

    let update = CreateJobUpdateV1 {
        job_id,
        employer: params.employer,
        title_hash: params.title_hash,
        specification_hash: params.specification_hash,
        delivery_type: params.delivery_type,
        total_payment: params.total_payment,
        milestone_count: params.milestones.len() as u32,
        deadline_block: params.deadline_block,
        milestones: params.milestones.clone(),
    };

    msg!(
        "[labor_market_plain::create_job] Job {:?} created with {} milestones",
        update.job_id,
        update.milestone_count
    );
    Ok(serialize(&update))
}

fn create_job_process_update_v1(cid: ContractId, update: CreateJobUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;

    let job = Job {
        id: update.job_id,
        employer: update.employer,
        worker: None,
        title_hash: update.title_hash,
        specification_hash: update.specification_hash,
        delivery_type: update.delivery_type,
        total_payment: update.total_payment,
        payment_token: Base::zero(),
        state: JobState::Created,
        milestones: update.milestones,
        created_at_block: 0,
        deadline_block: update.deadline_block,
        released_payment: 0,
        employer_signature: None,
    };

    wasm::db::db_set(db, &serialize(&update.job_id), &serialize(&job))?;
    msg!("[labor_market_plain::create_job::update] Job stored");

    Ok(())
}

// =============================================================================
// ACCEPT JOB
// =============================================================================

fn accept_job_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: AcceptJobParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[labor_market_plain::accept_job] Accepting job {:?}", params.job_id);

    // Look up job
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;
    let mut job: Job = match wasm::db::db_get(db, &serialize(&params.job_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(LaborMarketPlainError::JobNotFound.into()),
    };

    // Check job is in Created state
    if job.state != JobState::Created {
        return Err(LaborMarketPlainError::InvalidJobState.into())
    }

    // Check deadline hasn't passed
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= job.deadline_block {
        return Err(LaborMarketPlainError::DeadlinePassed.into())
    }

    // Verify worker signature
    // OPCODE PLACEHOLDER: When Schnorr verification is in ZK, worker would be constrained
    let mut signature_msg = vec![];
    params.job_id.encode(&mut signature_msg)?;
    params.worker.x().encode(&mut signature_msg)?;
    params.worker.y().encode(&mut signature_msg)?;

    if !params.worker.verify(&signature_msg, &params.signature) {
        return Err(LaborMarketPlainError::InvalidSignature.into())
    }

    // Update job state
    job.state = JobState::InProgress;
    job.worker = Some(params.worker);

    let update = AcceptJobUpdateV1 {
        job_id: params.job_id,
        worker: params.worker,
    };

    wasm::db::db_set(db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market_plain::accept_job] Job {:?} accepted by {:?}", params.job_id, params.worker);
    Ok(serialize(&update))
}

fn accept_job_process_update_v1(cid: ContractId, update: AcceptJobUpdateV1) -> GenericResult<()> {
    // State already updated in process_instruction
    msg!("[labor_market_plain::accept_job::update] Job acceptance confirmed");
    Ok(())
}

// =============================================================================
// SUBMIT DELIVERABLE
// =============================================================================

fn submit_deliverable_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: SubmitDeliverableParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[labor_market_plain::submit_deliverable] Submitting deliverable for job {:?}, milestone {}",
        params.job_id,
        params.milestone_index
    );

    // Look up job
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;
    let job: Job = match wasm::db::db_get(db, &serialize(&params.job_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(LaborMarketPlainError::JobNotFound.into()),
    };

    // Check job is in InProgress state
    if job.state != JobState::InProgress {
        return Err(LaborMarketPlainError::InvalidJobState.into())
    }

    // Check milestone index is valid
    if params.milestone_index as usize >= job.milestones.len() {
        return Err(LaborMarketPlainError::MilestoneNotFound.into())
    }

    // Check milestone not already completed
    if job.milestones[params.milestone_index as usize].completed {
        return Err(LaborMarketPlainError::MilestoneAlreadyCompleted.into())
    }

    // Check deadline hasn't passed
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block >= job.deadline_block {
        return Err(LaborMarketPlainError::DeadlinePassed.into())
    }

    // Verify worker signature
    // OPCODE PLACEHOLDER: When Schnorr verification is in ZK, worker would be constrained
    let mut signature_msg = vec![];
    params.job_id.encode(&mut signature_msg)?;
    params.milestone_index.encode(&mut signature_msg)?;
    params.deliverable_hash.encode(&mut signature_msg)?;

    if let Some(worker) = &job.worker {
        if !worker.verify(&signature_msg, &params.signature) {
            return Err(LaborMarketPlainError::InvalidSignature.into())
        }
    } else {
        return Err(LaborMarketPlainError::UnauthorizedCaller.into())
    }

    let update = SubmitDeliverableUpdateV1 {
        job_id: params.job_id,
        milestone_index: params.milestone_index,
        deliverable_hash: params.deliverable_hash,
        submitted_at_block: current_block,
    };

    msg!(
        "[labor_market_plain::submit_deliverable] Deliverable submitted for job {:?}",
        params.job_id
    );
    Ok(serialize(&update))
}

fn submit_deliverable_process_update_v1(
    cid: ContractId,
    update: SubmitDeliverableUpdateV1,
) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;

    let mut job: Job = match wasm::db::db_get(db, &serialize(&update.job_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(LaborMarketPlainError::JobNotFound.into()),
    };

    // Mark milestone as completed
    // Note: We don't update state to Delivered here - that's done on confirmation
    if (update.milestone_index as usize) < job.milestones.len() {
        job.milestones[update.milestone_index as usize].completed = true;
        job.milestones[update.milestone_index as usize].completed_at_block =
            Some(update.submitted_at_block);
    }

    wasm::db::db_set(db, &serialize(&update.job_id), &serialize(&job))?;
    msg!("[labor_market_plain::submit_deliverable::update] Deliverable marked as submitted");

    Ok(())
}

// =============================================================================
// CONFIRM DELIVERABLE
// =============================================================================

fn confirm_deliverable_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: ConfirmDeliverableParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[labor_market_plain::confirm_deliverable] Confirming milestone {} for job {:?}",
        params.milestone_index,
        params.job_id
    );

    // Look up job
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;
    let mut job: Job = match wasm::db::db_get(db, &serialize(&params.job_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(LaborMarketPlainError::JobNotFound.into()),
    };

    // Check job is in InProgress or Delivered state
    if job.state != JobState::InProgress && job.state != JobState::Delivered {
        return Err(LaborMarketPlainError::InvalidJobState.into())
    }

    // Check milestone index is valid
    if params.milestone_index as usize >= job.milestones.len() {
        return Err(LaborMarketPlainError::MilestoneNotFound.into())
    }

    // Check milestone is completed
    if !job.milestones[params.milestone_index as usize].completed {
        return Err(LaborMarketPlainError::InvalidDeliverable.into())
    }

    // Verify employer signature
    // OPCODE PLACEHOLDER: When Schnorr verification is in ZK, employer would be constrained
    let mut signature_msg = vec![];
    params.job_id.encode(&mut signature_msg)?;
    params.milestone_index.encode(&mut signature_msg)?;

    if !job.employer.verify(&signature_msg, &params.signature) {
        return Err(LaborMarketPlainError::InvalidSignature.into())
    }

    // Calculate payment for this milestone
    let milestone_payment = job.milestones[params.milestone_index as usize].payment_amount;

    // OPCODE PLACEHOLDER: When base_div is in ZK, payment release could use ZK constraints
    // Currently uses native Rust arithmetic (visible on-chain)
    job.released_payment = job.released_payment.saturating_add(milestone_payment);

    // Check if all milestones are complete
    let all_complete = job.milestones.iter().all(|m| m.completed);
    if all_complete {
        job.state = JobState::Confirmed;
    } else {
        job.state = JobState::Delivered;
    }

    let update = ConfirmDeliverableUpdateV1 {
        job_id: params.job_id,
        milestone_index: params.milestone_index,
        payment_released: milestone_payment,
    };

    wasm::db::db_set(db, &serialize(&params.job_id), &serialize(&job))?;
    msg!(
        "[labor_market_plain::confirm_deliverable] Released payment of {} for milestone {}",
        milestone_payment,
        params.milestone_index
    );
    Ok(serialize(&update))
}

fn confirm_deliverable_process_update_v1(
    _cid: ContractId,
    _update: ConfirmDeliverableUpdateV1,
) -> GenericResult<()> {
    // State already updated in process_instruction
    msg!("[labor_market_plain::confirm_deliverable::update] Confirmation stored");
    Ok(())
}

// =============================================================================
// DISPUTE
// =============================================================================

fn dispute_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: DisputeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[labor_market_plain::dispute] Raising dispute for job {:?}", params.job_id);

    // Look up job
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;
    let mut job: Job = match wasm::db::db_get(db, &serialize(&params.job_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(LaborMarketPlainError::JobNotFound.into()),
    };

    // Check job is in InProgress or Delivered state
    if job.state != JobState::InProgress && job.state != JobState::Delivered {
        return Err(LaborMarketPlainError::InvalidJobState.into())
    }

    // Verify disputor signature
    let mut signature_msg = vec![];
    params.job_id.encode(&mut signature_msg)?;
    params.dispute_reason_hash.encode(&mut signature_msg)?;

    if !params.disputor.verify(&signature_msg, &params.signature) {
        return Err(LaborMarketPlainError::InvalidSignature.into())
    }

    // Update job state
    job.state = JobState::Disputed;

    let update = DisputeUpdateV1 {
        job_id: params.job_id,
        dispute_reason_hash: params.dispute_reason_hash,
    };

    wasm::db::db_set(db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market_plain::dispute] Job {:?} moved to disputed state", params.job_id);
    Ok(serialize(&update))
}

fn dispute_process_update_v1(_cid: ContractId, _update: DisputeUpdateV1) -> GenericResult<()> {
    msg!("[labor_market_plain::dispute::update] Dispute stored");
    Ok(())
}

// =============================================================================
// CANCEL JOB
// =============================================================================

fn cancel_job_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CancelJobParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[labor_market_plain::cancel] Cancelling job {:?}", params.job_id);

    // Look up job
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;
    let mut job: Job = match wasm::db::db_get(db, &serialize(&params.job_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(LaborMarketPlainError::JobNotFound.into()),
    };

    // Check job is in Created state (can only cancel before acceptance)
    if job.state != JobState::Created {
        return Err(LaborMarketPlainError::InvalidJobState.into())
    }

    // Verify employer signature
    let mut signature_msg = vec![];
    params.job_id.encode(&mut signature_msg)?;

    if !job.employer.verify(&signature_msg, &params.signature) {
        return Err(LaborMarketPlainError::InvalidSignature.into())
    }

    // Update job state
    job.state = JobState::Cancelled;

    // Calculate refund (total payment minus any released)
    let refund_amount = job.total_payment.saturating_sub(job.released_payment);

    let update = CancelJobUpdateV1 {
        job_id: params.job_id,
        refund_amount,
    };

    wasm::db::db_set(db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market_plain::cancel] Job {:?} cancelled, refund: {}", params.job_id, refund_amount);
    Ok(serialize(&update))
}

fn cancel_job_process_update_v1(_cid: ContractId, _update: CancelJobUpdateV1) -> GenericResult<()> {
    msg!("[labor_market_plain::cancel::update] Cancellation stored");
    Ok(())
}

// =============================================================================
// REFUND (After timeout)
// =============================================================================

fn refund_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: RefundParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[labor_market_plain::refund] Processing refund for job {:?}", params.job_id);

    // Look up job
    let db = wasm::db::db_lookup(cid, JOBS_TREE)?;
    let mut job: Job = match wasm::db::db_get(db, &serialize(&params.job_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(LaborMarketPlainError::JobNotFound.into()),
    };

    // Check job is in InProgress or Delivered state
    if job.state != JobState::InProgress && job.state != JobState::Delivered {
        return Err(LaborMarketPlainError::InvalidJobState.into())
    }

    // Check deadline has passed
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block < job.deadline_block {
        return Err(LaborMarketPlainError::InvalidDeadline.into())
    }

    // Verify caller is employer
    if job.employer != params.caller {
        return Err(LaborMarketPlainError::UnauthorizedCaller.into())
    }

    // Verify caller signature
    let mut signature_msg = vec![];
    params.job_id.encode(&mut signature_msg)?;

    if !params.caller.verify(&signature_msg, &params.signature) {
        return Err(LaborMarketPlainError::InvalidSignature.into())
    }

    // Calculate refund amount
    let refund_amount = job.total_payment.saturating_sub(job.released_payment);

    // Update job state
    job.state = JobState::Refunded;

    let update = RefundUpdateV1 {
        job_id: params.job_id,
        refund_amount,
    };

    wasm::db::db_set(db, &serialize(&params.job_id), &serialize(&job))?;
    msg!(
        "[labor_market_plain::refund] Refund of {} processed for job {:?}",
        refund_amount,
        params.job_id
    );
    Ok(serialize(&update))
}

fn refund_process_update_v1(_cid: ContractId, _update: RefundUpdateV1) -> GenericResult<()> {
    msg!("[labor_market_plain::refund::update] Refund stored");
    Ok(())
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Derive a unique job ID from job parameters
fn derive_job_id(params: &CreateJobParamsV1, current_block: u64) -> Base {
    poseidon_hash([
        params.employer.x(),
        params.employer.y(),
        params.title_hash,
        params.specification_hash,
        Base::from(params.total_payment),
        Base::from(params.deadline_block),
        Base::from(current_block),
    ])
}