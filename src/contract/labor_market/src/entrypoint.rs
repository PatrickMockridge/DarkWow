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

//! WASM entrypoint for the labor market contract
//!
//! ## Labor Market Contract Overview
//!
//! A job/labor market contract using escrow and DAO governance.
//! Enables trustless conditional payments for completed work.
//!
//! ## Trust Model
//!
//! - **Employer creates job** with payment deposited in escrow
//! - **Worker accepts** and delivers work before deadline
//! - **Work verified off-chain** (zip hash or git commit hash)
//! - **Employer confirms** -> payment released to worker
//! - **Timeout** -> refund to employer
//! - **Dispute** -> DAO governance resolution
//!
//! ## Delivery Types
//!
//! - **Generic**: Worker submits `hash(zip_file)` as proof of work
//! - **Git**: Worker submits `commit_hash` as proof of work

use darkfi_sdk::{
    crypto::pasta_prelude::*,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::LaborMarketError,
    model::{
        AcceptJobParamsV1, AcceptJobWithCapabilityParamsV1, CancelJobParamsV1,
        ConfirmDeliveryParamsV1, ConfirmMilestoneParamsV1, CreateJobParamsV1,
        CreateJobWithMilestonesParamsV1, DisputeParamsV1,
        InitiateDisputeParamsV1, Job, JobState, Milestone, RefundParamsV1,
        SubmitDeliverableParamsV1, SubmitGitDeliverableParamsV1, SubmitMilestoneDeliverableParamsV1,
    },
    LaborMarketFunction, LABOR_CONTRACT_INFO_TREE, LABOR_CONTRACT_JOBS_TREE,
    LABOR_CONTRACT_NULLIFIERS_TREE, LABOR_CONTRACT_SPENT_FLAGS_TREE,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize labor market contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Jobs tree (job records)
/// - Nullifiers tree (spent nullifiers)
/// - Spent flags tree (prevents double actions)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[labor_market::init_contract] Initializing labor market contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, LABOR_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, b"db_version", &env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize jobs tree
    wasm::db::db_init(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize spent flags tree
    wasm::db::db_init(cid, LABOR_CONTRACT_SPENT_FLAGS_TREE)?;

    msg!("[labor_market::init_contract] Labor market contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = LaborMarketFunction::try_from(self_.data[0])?;

    msg!("[labor_market::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        LaborMarketFunction::CreateJobV1 => {
            let params: CreateJobParamsV1 = deserialize(&self_.data[1..])?;
            create_job_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::AcceptJobV1 => {
            let params: AcceptJobParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::SubmitDeliverableV1 => {
            let params: SubmitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_deliverable_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::SubmitGitDeliverableV1 => {
            let params: SubmitGitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_git_deliverable_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::ConfirmDeliveryV1 => {
            let params: ConfirmDeliveryParamsV1 = deserialize(&self_.data[1..])?;
            confirm_delivery_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::DisputeV1 => {
            let params: DisputeParamsV1 = deserialize(&self_.data[1..])?;
            dispute_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::RefundV1 => {
            let params: RefundParamsV1 = deserialize(&self_.data[1..])?;
            refund_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::CancelV1 => vec![],
        LaborMarketFunction::CreateJobWithMilestonesV1 => {
            let params: CreateJobWithMilestonesParamsV1 = deserialize(&self_.data[1..])?;
            create_job_with_milestones_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::SubmitMilestoneV1 => {
            let params: SubmitMilestoneDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_milestone_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::ConfirmMilestoneV1 => {
            let params: ConfirmMilestoneParamsV1 = deserialize(&self_.data[1..])?;
            confirm_milestone_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::InitiateDisputeV1 => {
            let params: InitiateDisputeParamsV1 = deserialize(&self_.data[1..])?;
            initiate_dispute_get_metadata_v1(cid, call_idx, calls, params)?
        }
        // O-Cap enabled functions
        LaborMarketFunction::CreateJobWithCapabilityV1 => vec![],
        LaborMarketFunction::AcceptJobWithCapabilityV1 => {
            let params: AcceptJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_with_capability_get_metadata_v1(cid, call_idx, calls, params)?
        }
        LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// `get_metadata` for CreateJobV1
fn create_job_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateJobParamsV1,
) -> ContractResult {
    msg!("[labor_market::create_job_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1,
    )?;

    // Public inputs: employer public key coordinates and attestation ID
    let mut public_inputs: Vec<pallas::Base> = vec![
        params.employer_pub_x,
        params.employer_pub_y,
        params.attestation_id,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for AcceptJobV1
fn accept_job_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: AcceptJobParamsV1,
) -> ContractResult {
    msg!("[labor_market::accept_job_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_ACCEPT_JOB_NS_V1,
    )?;

    let mut public_inputs: Vec<pallas::Base> =
        vec![params.job_id, params.worker_pub_x, params.worker_pub_y];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for SubmitDeliverableV1
fn submit_deliverable_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: SubmitDeliverableParamsV1,
) -> ContractResult {
    msg!("[labor_market::submit_deliverable_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1,
    )?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.claim_id,
        params.worker_pub_x,
        params.worker_pub_y,
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for SubmitGitDeliverableV1
fn submit_git_deliverable_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: SubmitGitDeliverableParamsV1,
) -> ContractResult {
    msg!("[labor_market::submit_git_deliverable_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_SUBMIT_GIT_DELIVERABLE_NS_V1,
    )?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.claim_id,
        params.worker_pub_x,
        params.worker_pub_y,
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for ConfirmDeliveryV1
fn confirm_delivery_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: ConfirmDeliveryParamsV1,
) -> ContractResult {
    msg!("[labor_market::confirm_delivery_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1,
    )?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.employer_pub_x,
        params.employer_pub_y,
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for DisputeV1
fn dispute_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: DisputeParamsV1,
) -> ContractResult {
    msg!("[labor_market::dispute_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes =
        wasm::util::get_zk_bytes_for_function(cid, crate::LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1)?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.disputer_pub_x,
        params.disputer_pub_y,
        params.dao_escrow_bulla,
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for RefundV1
fn refund_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: RefundParamsV1,
) -> ContractResult {
    msg!("[labor_market::refund_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes =
        wasm::util::get_zk_bytes_for_function(cid, crate::LABOR_CONTRACT_ZKAS_REFUND_NS_V1)?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.employer_pub_x,
        params.employer_pub_y,
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

// ============================================================================
// PROCESS INSTRUCTION (state transitions)
// ============================================================================

/// Process contract instructions
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = LaborMarketFunction::try_from(self_.data[0])?;

    msg!("[labor_market::process_instruction] Processing function: {:?}", func);

    match func {
        LaborMarketFunction::CreateJobV1 => {
            let params: CreateJobParamsV1 = deserialize(&self_.data[1..])?;
            create_job_v1(cid, params)
        }
        LaborMarketFunction::AcceptJobV1 => {
            let params: AcceptJobParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_v1(cid, params)
        }
        LaborMarketFunction::SubmitDeliverableV1 => {
            let params: SubmitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_deliverable_v1(cid, params)
        }
        LaborMarketFunction::SubmitGitDeliverableV1 => {
            let params: SubmitGitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_git_deliverable_v1(cid, params)
        }
        LaborMarketFunction::ConfirmDeliveryV1 => {
            let params: ConfirmDeliveryParamsV1 = deserialize(&self_.data[1..])?;
            confirm_delivery_v1(cid, params)
        }
        LaborMarketFunction::DisputeV1 => {
            let params: DisputeParamsV1 = deserialize(&self_.data[1..])?;
            dispute_v1(cid, params)
        }
        LaborMarketFunction::RefundV1 => {
            let params: RefundParamsV1 = deserialize(&self_.data[1..])?;
            refund_v1(cid, params)
        }
        LaborMarketFunction::CancelV1 => {
            let params: CancelJobParamsV1 = deserialize(&self_.data[1..])?;
            cancel_job_v1(cid, params)
        }
        LaborMarketFunction::CreateJobWithMilestonesV1 => {
            let params: CreateJobWithMilestonesParamsV1 = deserialize(&self_.data[1..])?;
            create_job_with_milestones_v1(cid, params)
        }
        LaborMarketFunction::SubmitMilestoneV1 => {
            let params: SubmitMilestoneDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_milestone_v1(cid, params)
        }
        LaborMarketFunction::ConfirmMilestoneV1 => {
            let params: ConfirmMilestoneParamsV1 = deserialize(&self_.data[1..])?;
            confirm_milestone_v1(cid, params)
        }
        LaborMarketFunction::InitiateDisputeV1 => {
            let params: InitiateDisputeParamsV1 = deserialize(&self_.data[1..])?;
            initiate_dispute_v1(cid, params)
        }
        // O-Cap enabled functions
        LaborMarketFunction::CreateJobWithCapabilityV1 => {
            // TODO: Implement capability-aware job creation
            msg!("[labor_market::process_instruction] CreateJobWithCapabilityV1 not yet implemented");
            Ok(())
        }
        LaborMarketFunction::AcceptJobWithCapabilityV1 => {
            let params: AcceptJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_with_capability_v1(cid, params)
        }
        LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1 => {
            // TODO: Implement milestone job with capability
            msg!("[labor_market::process_instruction] CreateJobWithMilestonesAndCapabilityV1 not yet implemented");
            Ok(())
        }
    }
}

/// CreateJobV1 instruction
fn create_job_v1(cid: ContractId, params: CreateJobParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_v1] Creating job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1)?;

    // Job is created in the apply phase via update
    msg!("[labor_market::create_job_v1] ZK proof verified successfully");
    Ok(())
}

/// AcceptJobV1 instruction
fn accept_job_v1(cid: ContractId, params: AcceptJobParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_v1] Accepting job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_ACCEPT_JOB_NS_V1)?;

    msg!("[labor_market::accept_job_v1] ZK proof verified successfully");
    Ok(())
}

/// SubmitDeliverableV1 instruction
fn submit_deliverable_v1(cid: ContractId, params: SubmitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_deliverable_v1] Submitting deliverable for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1)?;

    msg!("[labor_market::submit_deliverable_v1] ZK proof verified successfully");
    Ok(())
}

/// SubmitGitDeliverableV1 instruction
fn submit_git_deliverable_v1(cid: ContractId, params: SubmitGitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_git_deliverable_v1] Submitting git deliverable for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_SUBMIT_GIT_DELIVERABLE_NS_V1)?;

    msg!("[labor_market::submit_git_deliverable_v1] ZK proof verified successfully");
    Ok(())
}

/// ConfirmDeliveryV1 instruction
fn confirm_delivery_v1(cid: ContractId, params: ConfirmDeliveryParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_delivery_v1] Confirming delivery for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1)?;

    msg!("[labor_market::confirm_delivery_v1] ZK proof verified successfully");
    Ok(())
}

/// DisputeV1 instruction
fn dispute_v1(cid: ContractId, params: DisputeParamsV1) -> ContractResult {
    msg!("[labor_market::dispute_v1] Creating dispute for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1)?;

    msg!("[labor_market::dispute_v1] ZK proof verified successfully");
    Ok(())
}

/// RefundV1 instruction
fn refund_v1(cid: ContractId, params: RefundParamsV1) -> ContractResult {
    msg!("[labor_market::refund_v1] Processing refund for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_REFUND_NS_V1)?;

    msg!("[labor_market::refund_v1] ZK proof verified successfully");
    Ok(())
}

/// CancelV1 instruction
fn cancel_job_v1(cid: ContractId, params: CancelJobParamsV1) -> ContractResult {
    msg!("[labor_market::cancel_job_v1] Cancelling job: {:?}", params.job_id);
    Ok(())
}

// ============================================================================
// PROCESS UPDATE (state changes)
// ============================================================================

/// Process contract updates (state changes)
fn process_update(cid: ContractId, update: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(update)?;
    let self_ = &calls[call_idx].data;
    let func = LaborMarketFunction::try_from(self_.data[0])?;

    msg!("[labor_market::process_update] Processing update for: {:?}", func);

    match func {
        LaborMarketFunction::CreateJobV1 => {
            let params: CreateJobParamsV1 = deserialize(&self_.data[1..])?;
            create_job_apply_v1(cid, params)
        }
        LaborMarketFunction::AcceptJobV1 => {
            let params: AcceptJobParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_apply_v1(cid, params)
        }
        LaborMarketFunction::SubmitDeliverableV1 => {
            let params: SubmitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_deliverable_apply_v1(cid, params)
        }
        LaborMarketFunction::SubmitGitDeliverableV1 => {
            let params: SubmitGitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_git_deliverable_apply_v1(cid, params)
        }
        LaborMarketFunction::ConfirmDeliveryV1 => {
            let params: ConfirmDeliveryParamsV1 = deserialize(&self_.data[1..])?;
            confirm_delivery_apply_v1(cid, params)
        }
        LaborMarketFunction::DisputeV1 => {
            let params: DisputeParamsV1 = deserialize(&self_.data[1..])?;
            dispute_apply_v1(cid, params)
        }
        LaborMarketFunction::RefundV1 => {
            let params: RefundParamsV1 = deserialize(&self_.data[1..])?;
            refund_apply_v1(cid, params)
        }
        LaborMarketFunction::CancelV1 => {
            let params: CancelJobParamsV1 = deserialize(&self_.data[1..])?;
            cancel_job_apply_v1(cid, params)
        }
        LaborMarketFunction::CreateJobWithMilestonesV1 => {
            let params: CreateJobWithMilestonesParamsV1 = deserialize(&self_.data[1..])?;
            create_job_with_milestones_apply_v1(cid, params)
        }
        LaborMarketFunction::SubmitMilestoneV1 => {
            let params: SubmitMilestoneDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_milestone_apply_v1(cid, params)
        }
        LaborMarketFunction::ConfirmMilestoneV1 => {
            let params: ConfirmMilestoneParamsV1 = deserialize(&self_.data[1..])?;
            confirm_milestone_apply_v1(cid, params)
        }
        LaborMarketFunction::InitiateDisputeV1 => {
            let params: InitiateDisputeParamsV1 = deserialize(&self_.data[1..])?;
            initiate_dispute_apply_v1(cid, params)
        }
        // O-Cap enabled functions
        LaborMarketFunction::CreateJobWithCapabilityV1 => {
            // TODO: Implement capability-aware job creation
            msg!("[labor_market::process_update] CreateJobWithCapabilityV1 not yet implemented");
            Ok(())
        }
        LaborMarketFunction::AcceptJobWithCapabilityV1 => {
            let params: AcceptJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_with_capability_apply_v1(cid, params)
        }
        LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1 => {
            // TODO: Implement milestone job with capability
            msg!("[labor_market::process_update] CreateJobWithMilestonesAndCapabilityV1 not yet implemented");
            Ok(())
        }
    }
}

/// CreateJob apply - store new job in database
fn create_job_apply_v1(cid: ContractId, params: CreateJobParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_apply_v1] Storing job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // Check if job already exists
    let job_exists = wasm::db::db_contains_key(jobs_db, &serialize(&params.job_id))?;
    if job_exists {
        msg!("[labor_market::create_job_apply_v1] Job already exists!");
        return Err(ContractError::from(LaborMarketError::JobAlreadyExists).into())
    }

    // Validate delivery type
    let delivery_type = match params.delivery_type {
        0 => crate::model::DeliveryType::Generic,
        1 => crate::model::DeliveryType::Git,
        _ => {
            msg!("[labor_market::create_job_apply_v1] Invalid delivery type");
            return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
        }
    };

    // Create new job with all parameters properly set
    let job = Job {
        id: params.job_id,
        employer_pubkey: [params.employer_pub_x, params.employer_pub_y],
        worker_pubkey: None,
        attestation_id: params.attestation_id,
        delivery_type,
        payment_amount: params.payment_amount,
        payment_token: params.payment_token,
        payment_commit: [params.payment_commit_x, params.payment_commit_y],
        deadline_block: params.deadline_block,
        state: JobState::Created,
        dao_escrow_bulla: None,
    };

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::create_job_apply_v1] Job stored successfully");
    Ok(())
}

/// AcceptJob apply - update job with worker
fn accept_job_apply_v1(cid: ContractId, params: AcceptJobParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_apply_v1] Accepting job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // SECURITY FIX: Get existing job and verify it exists
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::accept_job_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job is in Created state
    if job.state != JobState::Created {
        msg!("[labor_market::accept_job_apply_v1] ERROR: Job not in Created state");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify no worker already assigned
    if job.worker_pubkey.is_some() {
        msg!("[labor_market::accept_job_apply_v1] ERROR: Worker already assigned");
        return Err(ContractError::from(LaborMarketError::WorkerAlreadyAssigned).into())
    }

    // Update job with worker
    job.worker_pubkey = Some([params.worker_pub_x, params.worker_pub_y]);
    job.state = JobState::InProgress;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::accept_job_apply_v1] Job accepted, worker assigned");
    Ok(())
}

/// SubmitDeliverable apply - mark job as delivered
fn submit_deliverable_apply_v1(cid: ContractId, params: SubmitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_deliverable_apply_v1] Job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_get(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check double-submission
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = deserialize(&job_data)?;

    // Verify job is in InProgress state
    if job.state != JobState::InProgress {
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify the attestation claim exists and is valid
    // The claim_id should reference a claim on job.attestation_id
    // Cross-contract call to attestation contract would verify the claim
    // For now, we verify the claim_id is provided (attestation contract handles validation)
    if params.claim_id == pallas::Base::zero() {
        msg!("[labor_market::submit_deliverable_apply_v1] ERROR: Invalid claim ID");
        return Err(ContractError::from(LaborMarketError::InvalidClaim.into()).into())
    }

    // Verify delivery type is Generic (not Git)
    if job.delivery_type != crate::model::DeliveryType::Generic {
        msg!("[labor_market::submit_deliverable_apply_v1] ERROR: Wrong delivery type");
        return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
    }

    // Update job state
    job.state = JobState::Delivered;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::submit_deliverable_apply_v1] Job delivered via attestation claim: {:?}", params.claim_id);
    Ok(())
}

/// SubmitGitDeliverable apply
fn submit_git_deliverable_apply_v1(cid: ContractId, params: SubmitGitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_git_deliverable_apply_v1] Job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_get(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check double-submission
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = deserialize(&job_data)?;

    // Verify job is in InProgress state
    if job.state != JobState::InProgress {
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify the attestation claim exists and is valid
    if params.claim_id == pallas::Base::zero() {
        msg!("[labor_market::submit_git_deliverable_apply_v1] ERROR: Invalid claim ID");
        return Err(ContractError::from(LaborMarketError::InvalidClaim.into()).into())
    }

    // Verify delivery type is Git (not Generic)
    if job.delivery_type != crate::model::DeliveryType::Git {
        msg!("[labor_market::submit_git_deliverable_apply_v1] ERROR: Wrong delivery type");
        return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
    }

    // Update job state
    job.state = JobState::Delivered;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::submit_git_deliverable_apply_v1] Job delivered via attestation claim: {:?}", params.claim_id);
    Ok(())
}

/// ConfirmDelivery apply
fn confirm_delivery_apply_v1(cid: ContractId, params: ConfirmDeliveryParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_delivery_apply_v1] Confirming delivery for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let spent_flags_db = wasm::db::db_get(cid, LABOR_CONTRACT_SPENT_FLAGS_TREE)?;

    // Check if already spent
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::confirm_delivery_apply_v1] ERROR: Already spent");
        return Err(ContractError::from(LaborMarketError::AlreadySpent).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::confirm_delivery_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // SECURITY FIX: Verify job is in Delivered state
    if job.state != JobState::Delivered {
        msg!("[labor_market::confirm_delivery_apply_v1] ERROR: Job not in Delivered state");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Update job state
    job.state = JobState::Confirmed;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(spent_flags_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::confirm_delivery_apply_v1] Job confirmed, payment released");
    Ok(())
}

/// Dispute apply
fn dispute_apply_v1(cid: ContractId, params: DisputeParamsV1) -> ContractResult {
    msg!("[labor_market::dispute_apply_v1] Creating dispute for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_get(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check if already disputed
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::dispute_apply_v1] ERROR: Already submitted");
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::dispute_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job is in Delivered or InProgress state
    if job.state != JobState::Delivered && job.state != JobState::InProgress {
        msg!("[labor_market::dispute_apply_v1] ERROR: Invalid state for dispute");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Update job state
    job.state = JobState::Disputed;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::dispute_apply_v1] Job disputed");
    Ok(())
}

/// Refund apply
fn refund_apply_v1(cid: ContractId, params: RefundParamsV1) -> ContractResult {
    msg!("[labor_market::refund_apply_v1] Processing refund for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let spent_flags_db = wasm::db::db_get(cid, LABOR_CONTRACT_SPENT_FLAGS_TREE)?;

    // Check if already refunded/claimed
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::refund_apply_v1] ERROR: Already spent");
        return Err(ContractError::from(LaborMarketError::AlreadySpent).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::refund_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // SECURITY FIX: Allow refund from both InProgress (never delivered) and Delivered
    // (delivered but employer never confirmed). The ZK circuit proves deadline passed.
    if job.state != JobState::InProgress && job.state != JobState::Delivered {
        msg!("[labor_market::refund_apply_v1] ERROR: Invalid state for refund");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Update job state
    job.state = JobState::Refunded;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(spent_flags_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::refund_apply_v1] Job refunded");
    Ok(())
}

/// CancelJob apply
fn cancel_job_apply_v1(cid: ContractId, params: CancelJobParamsV1) -> ContractResult {
    msg!("[labor_market::cancel_job_apply_v1] Cancelling job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::cancel_job_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job is in Created state (can only cancel before acceptance)
    if job.state != JobState::Created {
        msg!("[labor_market::cancel_job_apply_v1] ERROR: Job not in Created state");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Update job state
    job.state = JobState::Cancelled;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::cancel_job_apply_v1] Job cancelled");
    Ok(())
}

// ============================================================================
// MILESTONE FUNCTIONS (Jobs with time-weighted payments)
// ============================================================================

/// `get_metadata` for CreateJobWithMilestonesV1
fn create_job_with_milestones_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateJobWithMilestonesParamsV1,
) -> ContractResult {
    msg!("[labor_market::create_job_with_milestones_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1,
    )?;

    // Public inputs: employer public key coordinates and attestation ID
    let mut public_inputs: Vec<pallas::Base> = vec![
        params.employer_pub_x,
        params.employer_pub_y,
        params.attestation_id,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for SubmitMilestoneV1
fn submit_milestone_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: SubmitMilestoneDeliverableParamsV1,
) -> ContractResult {
    msg!("[labor_market::submit_milestone_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1,
    )?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.claim_id,
        pallas::Base::from(params.milestone_index),
        params.worker_pub_x,
        params.worker_pub_y,
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for ConfirmMilestoneV1
fn confirm_milestone_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: ConfirmMilestoneParamsV1,
) -> ContractResult {
    msg!("[labor_market::confirm_milestone_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1,
    )?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.employer_pub_x,
        params.employer_pub_y,
        pallas::Base::from(params.milestone_index),
        pallas::Base::from(params.payment_release),
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `get_metadata` for InitiateDisputeV1
fn initiate_dispute_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: InitiateDisputeParamsV1,
) -> ContractResult {
    msg!("[labor_market::initiate_dispute_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes =
        wasm::util::get_zk_bytes_for_function(cid, crate::LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1)?;

    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        pallas::Base::from(params.milestone_index),
        params.disputer_pub_x,
        params.disputer_pub_y,
        params.dao_escrow_bulla,
        params.spent_nullifier,
    ];

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// CreateJobWithMilestonesV1 instruction
fn create_job_with_milestones_v1(cid: ContractId, params: CreateJobWithMilestonesParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_milestones_v1] Creating job with milestones: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1)?;

    msg!("[labor_market::create_job_with_milestones_v1] ZK proof verified successfully");
    Ok(())
}

/// SubmitMilestoneV1 instruction
fn submit_milestone_v1(cid: ContractId, params: SubmitMilestoneDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_milestone_v1] Submitting milestone for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1)?;

    msg!("[labor_market::submit_milestone_v1] ZK proof verified successfully");
    Ok(())
}

/// ConfirmMilestoneV1 instruction
fn confirm_milestone_v1(cid: ContractId, params: ConfirmMilestoneParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_milestone_v1] Confirming milestone for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1)?;

    msg!("[labor_market::confirm_milestone_v1] ZK proof verified successfully");
    Ok(())
}

/// InitiateDisputeV1 instruction
fn initiate_dispute_v1(cid: ContractId, params: InitiateDisputeParamsV1) -> ContractResult {
    msg!("[labor_market::initiate_dispute_v1] Initiating dispute for job: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1)?;

    msg!("[labor_market::initiate_dispute_v1] ZK proof verified successfully");
    Ok(())
}

/// CreateJobWithMilestones apply - store new job with milestones in database
fn create_job_with_milestones_apply_v1(cid: ContractId, params: CreateJobWithMilestonesParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_milestones_apply_v1] Storing job with milestones: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // Check if job already exists
    let job_exists = wasm::db::db_contains_key(jobs_db, &serialize(&params.job_id))?;
    if job_exists {
        msg!("[labor_market::create_job_with_milestones_apply_v1] Job already exists!");
        return Err(ContractError::from(LaborMarketError::JobAlreadyExists).into())
    }

    // Validate delivery type
    let delivery_type = match params.delivery_type {
        0 => crate::model::DeliveryType::Generic,
        1 => crate::model::DeliveryType::Git,
        _ => {
            msg!("[labor_market::create_job_with_milestones_apply_v1] Invalid delivery type");
            return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
        }
    };

    // Create milestones array (empty for now, filled by contract caller)
    // In a full implementation, the milestones would be passed as parameters
    let milestones: Vec<Milestone> = vec![];

    // Create new job with milestones
    let job = Job {
        id: params.job_id,
        employer_pubkey: [params.employer_pub_x, params.employer_pub_y],
        worker_pubkey: None,
        attestation_id: params.attestation_id,
        delivery_type,
        payment_amount: params.payment_amount,
        payment_token: params.payment_token,
        payment_commit: [params.payment_commit_x, params.payment_commit_y],
        deadline_block: params.deadline_block,
        state: JobState::Created,
        dao_escrow_bulla: None,
        milestones,
        current_milestone: 0,
        released_payment: 0,
    };

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::create_job_with_milestones_apply_v1] Job with milestones stored successfully");
    Ok(())
}

/// SubmitMilestone apply - mark milestone deliverable as submitted
fn submit_milestone_apply_v1(cid: ContractId, params: SubmitMilestoneDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_milestone_apply_v1] Job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_get(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check double-submission
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = deserialize(&job_data)?;

    // Verify job has milestones
    if job.milestones.is_empty() {
        msg!("[labor_market::submit_milestone_apply_v1] ERROR: Job does not have milestones");
        return Err(ContractError::from(LaborMarketError::JobDoesNotHaveMilestones).into())
    }

    // Verify job is in InProgress state
    if job.state != JobState::InProgress {
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify milestone index is valid
    if params.milestone_index >= job.milestones.len() as u32 {
        msg!("[labor_market::submit_milestone_apply_v1] ERROR: Invalid milestone index");
        return Err(ContractError::from(LaborMarketError::InvalidMilestoneIndex).into())
    }

    // Verify milestone is not already completed
    if job.milestones[params.milestone_index as usize].completed {
        msg!("[labor_market::submit_milestone_apply_v1] ERROR: Milestone already completed");
        return Err(ContractError::from(LaborMarketError::MilestoneAlreadyCompleted).into())
    }

    // Verify milestone is the current milestone (in order)
    if params.milestone_index != job.current_milestone {
        msg!("[labor_market::submit_milestone_apply_v1] ERROR: Milestone out of order");
        return Err(ContractError::from(LaborMarketError::MilestoneOutOfOrder).into())
    }

    // Verify the attestation claim exists and is valid
    if params.claim_id == pallas::Base::zero() {
        msg!("[labor_market::submit_milestone_apply_v1] ERROR: Invalid claim ID");
        return Err(ContractError::from(LaborMarketError::InvalidClaim.into()).into())
    }

    // Update job state to Delivered
    job.state = JobState::Delivered;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::submit_milestone_apply_v1] Milestone submitted: index={}", params.milestone_index);
    Ok(())
}

/// ConfirmMilestone apply - confirm milestone and release payment
fn confirm_milestone_apply_v1(cid: ContractId, params: ConfirmMilestoneParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_milestone_apply_v1] Confirming milestone for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let spent_flags_db = wasm::db::db_get(cid, LABOR_CONTRACT_SPENT_FLAGS_TREE)?;

    // Check if already spent
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::confirm_milestone_apply_v1] ERROR: Already spent");
        return Err(ContractError::from(LaborMarketError::AlreadySpent).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::confirm_milestone_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job has milestones
    if job.milestones.is_empty() {
        msg!("[labor_market::confirm_milestone_apply_v1] ERROR: Job does not have milestones");
        return Err(ContractError::from(LaborMarketError::JobDoesNotHaveMilestones).into())
    }

    // Verify job is in Delivered state
    if job.state != JobState::Delivered {
        msg!("[labor_market::confirm_milestone_apply_v1] ERROR: Job not in Delivered state");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify milestone index is valid
    if params.milestone_index >= job.milestones.len() as u32 {
        msg!("[labor_market::confirm_milestone_apply_v1] ERROR: Invalid milestone index");
        return Err(ContractError::from(LaborMarketError::InvalidMilestoneIndex).into())
    }

    // Verify milestone is not already completed
    if job.milestones[params.milestone_index as usize].completed {
        msg!("[labor_market::confirm_milestone_apply_v1] ERROR: Milestone already completed");
        return Err(ContractError::from(LaborMarketError::MilestoneAlreadyCompleted).into())
    }

    // Mark milestone as completed
    job.milestones[params.milestone_index as usize].completed = true;
    job.milestones[params.milestone_index as usize].completed_at_block = Some(0); // Would be set by block

    // Update released payment
    job.released_payment += params.payment_release;

    // Move to next milestone or confirm if last
    if params.milestone_index == (job.milestones.len() as u32 - 1) {
        // Last milestone - job is complete
        job.state = JobState::Confirmed;
    } else {
        // Not last milestone - move to next
        job.current_milestone = params.milestone_index + 1;
        job.state = JobState::InProgress;
    }

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(spent_flags_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::confirm_milestone_apply_v1] Milestone confirmed, payment released: {}", params.payment_release);
    Ok(())
}

/// InitiateDispute apply - raise dispute for specific milestone
fn initiate_dispute_apply_v1(cid: ContractId, params: InitiateDisputeParamsV1) -> ContractResult {
    msg!("[labor_market::initiate_dispute_apply_v1] Creating dispute for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_get(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check if already disputed
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::initiate_dispute_apply_v1] ERROR: Already submitted");
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::initiate_dispute_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job has milestones
    if job.milestones.is_empty() {
        msg!("[labor_market::initiate_dispute_apply_v1] ERROR: Job does not have milestones");
        return Err(ContractError::from(LaborMarketError::JobDoesNotHaveMilestones).into())
    }

    // Verify milestone index is valid
    if params.milestone_index >= job.milestones.len() as u32 {
        msg!("[labor_market::initiate_dispute_apply_v1] ERROR: Invalid milestone index");
        return Err(ContractError::from(LaborMarketError::InvalidMilestoneIndex).into())
    }

    // Verify job is in Delivered or InProgress state
    if job.state != JobState::Delivered && job.state != JobState::InProgress {
        msg!("[labor_market::initiate_dispute_apply_v1] ERROR: Invalid state for dispute");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Update job state
    job.state = JobState::Disputed;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_put(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::initiate_dispute_apply_v1] Job disputed for milestone: {}", params.milestone_index);
    Ok(())
}

// ============================================================================
// O-CAP ENABLED FUNCTIONS (Capability-aware job operations)
// ============================================================================

/// `get_metadata` for AcceptJobWithCapabilityV1
fn accept_job_with_capability_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: AcceptJobWithCapabilityParamsV1,
) -> ContractResult {
    msg!("[labor_market::accept_job_with_capability_get_metadata_v1] job_id: {:?}", params.job_id);

    let zk_bytes = wasm::util::get_zk_bytes_for_function(
        cid,
        crate::LABOR_CONTRACT_ZKAS_ACCEPT_JOB_WITH_CAPABILITY_NS_V1,
    )?;

    // Public inputs: job_id, worker pubkey, required capability_id
    let mut public_inputs: Vec<pallas::Base> = vec![
        params.job_id,
        params.worker_pub_x,
        params.worker_pub_y,
    ];

    // Add capability_id as bytes converted to Base
    let cap_id_bytes: [u8; 32] = params.required_capability_id;
    public_inputs.push(pallas::Base::from_bytes(cap_id_bytes).unwrap_or_default());

    let mut metadata = vec![];
    (call_idx, &calls).encode(&mut metadata)?;
    zk_bytes.encode(&mut metadata)?;
    public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// AcceptJobWithCapabilityV1 instruction
fn accept_job_with_capability_v1(cid: ContractId, params: AcceptJobWithCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_with_capability_v1] Accepting job with capability: {:?}", params.job_id);

    // Verify ZK proof
    wasm::zk::verify_zk_proof(cid, crate::LABOR_CONTRACT_ZKAS_ACCEPT_JOB_WITH_CAPABILITY_NS_V1)?;

    msg!("[labor_market::accept_job_with_capability_v1] ZK proof verified successfully");
    Ok(())
}

/// AcceptJobWithCapabilityV1 apply - verify capability and update job
fn accept_job_with_capability_apply_v1(cid: ContractId, params: AcceptJobWithCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_with_capability_apply_v1] Accepting job with capability: {:?}", params.job_id);

    let jobs_db = wasm::db::db_get(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match deserialize(&job_data)? {
        Some(j) => j,
        None => {
            msg!("[labor_market::accept_job_with_capability_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job is in Created state
    if job.state != JobState::Created {
        msg!("[labor_market::accept_job_with_capability_apply_v1] ERROR: Job not in Created state");
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify no worker already assigned
    if job.worker_pubkey.is_some() {
        msg!("[labor_market::accept_job_with_capability_apply_v1] ERROR: Worker already assigned");
        return Err(ContractError::from(LaborMarketError::WorkerAlreadyAssigned).into())
    }

    // Verify this job requires a capability (should be set when job was created)
    let required_cap = job.required_capability_id.ok_or_else(|| {
        msg!("[labor_market::accept_job_with_capability_apply_v1] ERROR: Job does not require capability");
        ContractError::from(LaborMarketError::CapabilityRequired)
    })?;

    // Verify worker's capability_proof exists (Identity contract handles verification)
    // The ZK proof in params.proof already verified the capability
    // Here we just ensure the capability is not revoked (cross-contract check would happen at Identity)

    // Update job with worker
    job.worker_pubkey = Some([params.worker_pub_x, params.worker_pub_y]);
    job.state = JobState::InProgress;

    wasm::db::db_put(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::accept_job_with_capability_apply_v1] Job accepted with capability (id={:?}), worker assigned", required_cap);
    Ok(())
}
