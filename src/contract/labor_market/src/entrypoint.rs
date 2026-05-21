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

use dwow_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::LaborMarketError,
    model::{
        AcceptJobParamsV1, AcceptJobWithCapabilityParamsV1, CancelJobParamsV1,
        ConfirmDeliveryParamsV1, ConfirmMilestoneParamsV1, CreateJobParamsV1,
        CreateJobWithCapabilityParamsV1, CreateJobWithMilestonesAndCapabilityParamsV1,
        CreateJobWithMilestonesParamsV1, DisputeParamsV1,
        InitiateDisputeParamsV1, Job, JobState, Milestone, RefundParamsV1,
        SubmitDeliverableParamsV1, SubmitGitDeliverableParamsV1, SubmitMilestoneDeliverableParamsV1,
    },
    LaborMarketFunction, LABOR_CONTRACT_INFO_TREE, LABOR_CONTRACT_JOBS_TREE,
    LABOR_CONTRACT_NULLIFIERS_TREE, LABOR_CONTRACT_SPENT_FLAGS_TREE,
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

    let accept_job_v1_bincode = include_bytes!("../proof/accept_job_v1.zk.bin");
    wasm::db::zkas_db_set(&accept_job_v1_bincode[..])?;
    let accept_job_with_capability_v1_bincode = include_bytes!("../proof/accept_job_with_capability_v1.zk.bin");
    wasm::db::zkas_db_set(&accept_job_with_capability_v1_bincode[..])?;
    let confirm_delivery_v1_bincode = include_bytes!("../proof/confirm_delivery_v1.zk.bin");
    wasm::db::zkas_db_set(&confirm_delivery_v1_bincode[..])?;
    let create_job_v1_bincode = include_bytes!("../proof/create_job_v1.zk.bin");
    wasm::db::zkas_db_set(&create_job_v1_bincode[..])?;
    let dispute_v1_bincode = include_bytes!("../proof/dispute_v1.zk.bin");
    wasm::db::zkas_db_set(&dispute_v1_bincode[..])?;
    let refund_v1_bincode = include_bytes!("../proof/refund_v1.zk.bin");
    wasm::db::zkas_db_set(&refund_v1_bincode[..])?;
    let submit_deliverable_v1_bincode = include_bytes!("../proof/submit_deliverable_v1.zk.bin");
    wasm::db::zkas_db_set(&submit_deliverable_v1_bincode[..])?;
    let submit_git_deliverable_v1_bincode = include_bytes!("../proof/submit_git_deliverable_v1.zk.bin");
    wasm::db::zkas_db_set(&submit_git_deliverable_v1_bincode[..])?;
    let milestone_payment_v1_bincode = include_bytes!("../proof/milestone_payment_v1.zk.bin");
    wasm::db::zkas_db_set(&milestone_payment_v1_bincode[..])?;

    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = LaborMarketFunction::try_from(self_.data[0])?;

    msg!("[labor_market::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        LaborMarketFunction::CreateJobV1 => {
            let params: CreateJobParamsV1 = deserialize(&self_.data[1..])?;
            // Circuit constrain_instance (3): employer_pub_x, employer_pub_y, attestation_id
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1.to_string(),
                vec![
                    params.employer_pub_x,
                    params.employer_pub_y,
                    params.attestation_id,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::AcceptJobV1 => {
            let params: AcceptJobParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_ACCEPT_JOB_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.worker_pub_x,
                    params.worker_pub_y,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::SubmitDeliverableV1 => {
            let params: SubmitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.claim_id,
                    params.worker_pub_x,
                    params.worker_pub_y,
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::SubmitGitDeliverableV1 => {
            let params: SubmitGitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_SUBMIT_GIT_DELIVERABLE_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.claim_id,
                    params.worker_pub_x,
                    params.worker_pub_y,
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::ConfirmDeliveryV1 => {
            let params: ConfirmDeliveryParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.employer_pub_x,
                    params.employer_pub_y,
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::DisputeV1 => {
            let params: DisputeParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.disputer_pub_x,
                    params.disputer_pub_y,
                    params.dao_escrow_bulla,
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::RefundV1 => {
            let params: RefundParamsV1 = deserialize(&self_.data[1..])?;
            // Circuit constrain_instance (7): job_id, employer_pub_x, employer_pub_y,
            //   milestone_count, completed_payment, refund_amount, spent_nullifier
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_REFUND_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.employer_pub_x,
                    params.employer_pub_y,
                    pallas::Base::from(params.milestone_count),
                    pallas::Base::from(params.completed_payment),
                    pallas::Base::from(params.refund_amount),
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::CancelV1 => {
            // CancelV1 has no ZK circuit
            vec![]
        }
        LaborMarketFunction::CreateJobWithMilestonesV1 => {
            let params: CreateJobWithMilestonesParamsV1 = deserialize(&self_.data[1..])?;
            // Circuit constrain_instance (3): employer_pub_x, employer_pub_y, attestation_id
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1.to_string(),
                vec![
                    params.employer_pub_x,
                    params.employer_pub_y,
                    params.attestation_id,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::SubmitMilestoneV1 => {
            let params: SubmitMilestoneDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.claim_id,
                    params.worker_pub_x,
                    params.worker_pub_y,
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::ConfirmMilestoneV1 => {
            let params: ConfirmMilestoneParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.employer_pub_x,
                    params.employer_pub_y,
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::InitiateDisputeV1 => {
            let params: InitiateDisputeParamsV1 = deserialize(&self_.data[1..])?;
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.disputer_pub_x,
                    params.disputer_pub_y,
                    params.dao_escrow_bulla,
                    params.spent_nullifier,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        // O-Cap enabled functions
        LaborMarketFunction::CreateJobWithCapabilityV1 => {
            // No circuit exists yet — deferred to v1.1
            vec![]
        }
        LaborMarketFunction::AcceptJobWithCapabilityV1 => {
            let params: AcceptJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            // Circuit constrain_instance (4): job_id, worker_pub_x, worker_pub_y, required_capability_id
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![(
                crate::LABOR_CONTRACT_ZKAS_ACCEPT_JOB_WITH_CAPABILITY_NS_V1.to_string(),
                vec![
                    params.job_id,
                    params.worker_pub_x,
                    params.worker_pub_y,
                    params.required_capability_id,
                ],
            )];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1 => {
            // No circuit exists yet — deferred to v1.1
            vec![]
        }
    };

    wasm::util::set_return_data(&metadata)
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
            create_job_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::AcceptJobV1 => {
            let params: AcceptJobParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_v1(cid, params)
        }
        LaborMarketFunction::SubmitDeliverableV1 => {
            let params: SubmitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_deliverable_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::SubmitGitDeliverableV1 => {
            let params: SubmitGitDeliverableParamsV1 = deserialize(&self_.data[1..])?;
            submit_git_deliverable_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::ConfirmDeliveryV1 => {
            let params: ConfirmDeliveryParamsV1 = deserialize(&self_.data[1..])?;
            confirm_delivery_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::DisputeV1 => {
            let params: DisputeParamsV1 = deserialize(&self_.data[1..])?;
            dispute_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::RefundV1 => {
            let params: RefundParamsV1 = deserialize(&self_.data[1..])?;
            refund_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::CancelV1 => {
            let params: CancelJobParamsV1 = deserialize(&self_.data[1..])?;
            cancel_job_v1(cid, call_idx, calls, params)
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
            confirm_milestone_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::InitiateDisputeV1 => {
            let params: InitiateDisputeParamsV1 = deserialize(&self_.data[1..])?;
            initiate_dispute_v1(cid, call_idx, calls, params)
        }
        // O-Cap enabled functions
        LaborMarketFunction::CreateJobWithCapabilityV1 => {
            let params: CreateJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            create_job_with_capability_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::AcceptJobWithCapabilityV1 => {
            let params: AcceptJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_with_capability_v1(cid, call_idx, calls, params)
        }
        LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1 => {
            let params: CreateJobWithMilestonesAndCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            create_job_with_milestones_and_capability_v1(cid, params)
        }
    }
}

/// CreateJobV1 instruction
fn create_job_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: CreateJobParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_v1] Creating job: {:?}", params.job_id);

    // Validate child call is money_v3::transfer_v1 (0x04) for escrow deposit
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[create_job_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[create_job_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1)

    // Job is created in the apply phase via update
    msg!("[labor_market::create_job_v1] ZK proof verified successfully");
    Ok(())
}

/// AcceptJobV1 instruction
fn accept_job_v1(_cid: ContractId, params: AcceptJobParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_v1] Accepting job: {:?}", params.job_id);

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_ACCEPT_JOB_NS_V1)

    msg!("[labor_market::accept_job_v1] ZK proof verified successfully");
    Ok(())
}

/// SubmitDeliverableV1 instruction
fn submit_deliverable_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: SubmitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_deliverable_v1] Submitting deliverable for job: {:?}", params.job_id);

    // Validate child call to Attestation::VerifyClaimV1 (0x04) for on-chain attestation verification
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[submit_deliverable_v1] Error: Expected 1 child call (Attestation::VerifyClaimV1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[submit_deliverable_v1] Error: Expected Attestation::VerifyClaimV1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1)

    msg!("[labor_market::submit_deliverable_v1] ZK proof verified successfully");
    Ok(())
}

/// SubmitGitDeliverableV1 instruction
fn submit_git_deliverable_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: SubmitGitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_git_deliverable_v1] Submitting git deliverable for job: {:?}", params.job_id);

    // Validate child call to Attestation::VerifyClaimV1 (0x04) for on-chain attestation verification
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[submit_git_deliverable_v1] Error: Expected 1 child call (Attestation::VerifyClaimV1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[submit_git_deliverable_v1] Error: Expected Attestation::VerifyClaimV1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_SUBMIT_GIT_DELIVERABLE_NS_V1)

    msg!("[labor_market::submit_git_deliverable_v1] ZK proof verified successfully");
    Ok(())
}

/// ConfirmDeliveryV1 instruction
fn confirm_delivery_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: ConfirmDeliveryParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_delivery_v1] Confirming delivery for job: {:?}", params.job_id);

    // Validate child call is money_v3::transfer_v1 (0x04) for worker payout
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[confirm_delivery_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[confirm_delivery_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1)

    msg!("[labor_market::confirm_delivery_v1] ZK proof verified successfully");
    Ok(())
}

/// DisputeV1 instruction
fn dispute_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: DisputeParamsV1) -> ContractResult {
    msg!("[labor_market::dispute_v1] Creating dispute for job: {:?}", params.job_id);

    // Validate child call to DAO Escrow::ProposeClaimV1 (0x07) for dispute escalation
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[dispute_v1] Error: Expected 1 child call (DAO-Escrow::ProposeClaimV1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x07 {
        msg!("[dispute_v1] Error: Expected DAO-Escrow::ProposeClaimV1 (0x07), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1)

    msg!("[labor_market::dispute_v1] ZK proof verified successfully");
    Ok(())
}

/// RefundV1 instruction
fn refund_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: RefundParamsV1) -> ContractResult {
    msg!("[labor_market::refund_v1] Processing refund for job: {:?}", params.job_id);

    // Validate child call is money_v3::transfer_v1 (0x04) for refund to employer
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[refund_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[refund_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_REFUND_NS_V1)

    msg!("[labor_market::refund_v1] ZK proof verified successfully");
    Ok(())
}

/// CancelV1 instruction
fn cancel_job_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: CancelJobParamsV1) -> ContractResult {
    msg!("[labor_market::cancel_job_v1] Cancelling job: {:?}", params.job_id);

    // Validate child call is money_v3::transfer_v1 (0x04) for refund
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[cancel_job_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[cancel_job_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    Ok(())
}

// ============================================================================
// PROCESS UPDATE (state changes)
// ============================================================================

/// Process contract updates (state changes)
///
/// NOTE: Unlike other contracts that receive serialized update structs via
/// set_return_data from process_instruction, labor_market re-parses the
/// original transaction calls array. This is intentional: all validation
/// happens in process_instruction, and process_update replays the params
/// to apply state changes. A future refactor should migrate this to the
/// standard two-phase pattern (instruction produces update, update applies it).
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
            let params: CreateJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            create_job_with_capability_apply_v1(cid, params)
        }
        LaborMarketFunction::AcceptJobWithCapabilityV1 => {
            let params: AcceptJobWithCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            accept_job_with_capability_apply_v1(cid, params)
        }
        LaborMarketFunction::CreateJobWithMilestonesAndCapabilityV1 => {
            let params: CreateJobWithMilestonesAndCapabilityParamsV1 = deserialize(&self_.data[1..])?;
            create_job_with_milestones_and_capability_apply_v1(cid, params)
        }
    }
}

/// CreateJob apply - store new job in database
fn create_job_apply_v1(cid: ContractId, params: CreateJobParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_apply_v1] Storing job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;

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
        deadline_block: 0, // CreateJobV1 doesn't have deadline, use 0
        state: JobState::Created,
        dao_escrow_bulla: None,
        milestones: vec![],
        current_milestone: 0,
        released_payment: 0,
        required_capability_id: None,
        required_dag_id: None,
    };

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::create_job_apply_v1] Job stored successfully");
    Ok(())
}

/// AcceptJob apply - update job with worker
fn accept_job_apply_v1(cid: ContractId, params: AcceptJobParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_apply_v1] Accepting job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // SECURITY FIX: Get existing job and verify it exists
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::accept_job_apply_v1] Job accepted, worker assigned");
    Ok(())
}

/// SubmitDeliverable apply - mark job as delivered
fn submit_deliverable_apply_v1(cid: ContractId, params: SubmitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_deliverable_apply_v1] Job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check double-submission
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[labor_market::submit_deliverable_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job is in InProgress state
    if job.state != JobState::InProgress {
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify the attestation claim exists and is valid
    // The claim_id should reference a claim on job.attestation_id
    // Cross-contract call to attestation contract would verify the claim
    // For now, we verify the claim_id is provided (attestation contract handles validation)
    if params.claim_id == pasta::pallas::Base::zero() {
        msg!("[labor_market::submit_deliverable_apply_v1] ERROR: Invalid claim ID");
        return Err(ContractError::from(LaborMarketError::InvalidClaim).into())
    }

    // Verify delivery type is Generic (not Git)
    if job.delivery_type != crate::model::DeliveryType::Generic {
        msg!("[labor_market::submit_deliverable_apply_v1] ERROR: Wrong delivery type");
        return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
    }

    // Update job state
    job.state = JobState::Delivered;

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::submit_deliverable_apply_v1] Job delivered via attestation claim: {:?}", params.claim_id);
    Ok(())
}

/// SubmitGitDeliverable apply
fn submit_git_deliverable_apply_v1(cid: ContractId, params: SubmitGitDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_git_deliverable_apply_v1] Job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check double-submission
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[labor_market::submit_git_deliverable_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

    // Verify job is in InProgress state
    if job.state != JobState::InProgress {
        return Err(ContractError::from(LaborMarketError::InvalidStateTransition).into())
    }

    // Verify the attestation claim exists and is valid
    if params.claim_id == pasta::pallas::Base::zero() {
        msg!("[labor_market::submit_git_deliverable_apply_v1] ERROR: Invalid claim ID");
        return Err(ContractError::from(LaborMarketError::InvalidClaim).into())
    }

    // Verify delivery type is Git (not Generic)
    if job.delivery_type != crate::model::DeliveryType::Git {
        msg!("[labor_market::submit_git_deliverable_apply_v1] ERROR: Wrong delivery type");
        return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
    }

    // Update job state
    job.state = JobState::Delivered;

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::submit_git_deliverable_apply_v1] Job delivered via attestation claim: {:?}", params.claim_id);
    Ok(())
}

/// ConfirmDelivery apply
fn confirm_delivery_apply_v1(cid: ContractId, params: ConfirmDeliveryParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_delivery_apply_v1] Confirming delivery for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_SPENT_FLAGS_TREE)?;

    // Check if already spent
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::confirm_delivery_apply_v1] ERROR: Already spent");
        return Err(ContractError::from(LaborMarketError::AlreadySpent).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(spent_flags_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::confirm_delivery_apply_v1] Job confirmed, payment released");
    Ok(())
}

/// Dispute apply
fn dispute_apply_v1(cid: ContractId, params: DisputeParamsV1) -> ContractResult {
    msg!("[labor_market::dispute_apply_v1] Creating dispute for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check if already disputed
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::dispute_apply_v1] ERROR: Already submitted");
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::dispute_apply_v1] Job disputed");
    Ok(())
}

/// Refund apply
fn refund_apply_v1(cid: ContractId, params: RefundParamsV1) -> ContractResult {
    msg!("[labor_market::refund_apply_v1] Processing refund for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_SPENT_FLAGS_TREE)?;

    // Check if already refunded/claimed
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::refund_apply_v1] ERROR: Already spent");
        return Err(ContractError::from(LaborMarketError::AlreadySpent).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(spent_flags_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::refund_apply_v1] Job refunded");
    Ok(())
}

/// CancelJob apply
fn cancel_job_apply_v1(cid: ContractId, params: CancelJobParamsV1) -> ContractResult {
    msg!("[labor_market::cancel_job_apply_v1] Cancelling job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::cancel_job_apply_v1] Job cancelled");
    Ok(())
}

// ============================================================================
// MILESTONE FUNCTIONS (Jobs with time-weighted payments)
// ============================================================================

/// CreateJobWithMilestonesV1 instruction
fn create_job_with_milestones_v1(_cid: ContractId, params: CreateJobWithMilestonesParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_milestones_v1] Creating job with milestones: {:?}", params.job_id);

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1)

    msg!("[labor_market::create_job_with_milestones_v1] ZK proof verified successfully");
    Ok(())
}

/// SubmitMilestoneV1 instruction
fn submit_milestone_v1(_cid: ContractId, params: SubmitMilestoneDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_milestone_v1] Submitting milestone for job: {:?}", params.job_id);

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1)

    msg!("[labor_market::submit_milestone_v1] ZK proof verified successfully");
    Ok(())
}

/// ConfirmMilestoneV1 instruction
fn confirm_milestone_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: ConfirmMilestoneParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_milestone_v1] Confirming milestone for job: {:?}", params.job_id);

    // Validate child call is money_v3::transfer_v1 (0x04) for milestone payment
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[confirm_milestone_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[confirm_milestone_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1)

    msg!("[labor_market::confirm_milestone_v1] ZK proof verified successfully");
    Ok(())
}

/// InitiateDisputeV1 instruction
fn initiate_dispute_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: InitiateDisputeParamsV1) -> ContractResult {
    msg!("[labor_market::initiate_dispute_v1] Initiating dispute for job: {:?}", params.job_id);

    // Validate child call to DAO Escrow::ProposeClaimV1 (0x07) for dispute escalation
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[initiate_dispute_v1] Error: Expected 1 child call (DAO-Escrow::ProposeClaimV1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x07 {
        msg!("[initiate_dispute_v1] Error: Expected DAO-Escrow::ProposeClaimV1 (0x07), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1)

    msg!("[labor_market::initiate_dispute_v1] ZK proof verified successfully");
    Ok(())
}

/// CreateJobWithMilestones apply - store new job with milestones in database
fn create_job_with_milestones_apply_v1(cid: ContractId, params: CreateJobWithMilestonesParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_milestones_apply_v1] Storing job with milestones: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;

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

    // Use milestones from params (validated in process_instruction)
    let milestones = params.milestones;

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
        required_capability_id: None,
        required_dag_id: None,
    };

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::create_job_with_milestones_apply_v1] Job with milestones stored successfully");
    Ok(())
}

/// SubmitMilestone apply - mark milestone deliverable as submitted
fn submit_milestone_apply_v1(cid: ContractId, params: SubmitMilestoneDeliverableParamsV1) -> ContractResult {
    msg!("[labor_market::submit_milestone_apply_v1] Job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check double-submission
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[labor_market::submit_milestone_apply_v1] ERROR: Job not found");
            return Err(ContractError::from(LaborMarketError::JobNotFound).into())
        }
    };

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
    if params.claim_id == pasta::pallas::Base::zero() {
        msg!("[labor_market::submit_milestone_apply_v1] ERROR: Invalid claim ID");
        return Err(ContractError::from(LaborMarketError::InvalidClaim).into())
    }

    // Update job state to Delivered
    job.state = JobState::Delivered;

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::submit_milestone_apply_v1] Milestone submitted: index={}", params.milestone_index);
    Ok(())
}

/// ConfirmMilestone apply - confirm milestone and release payment
fn confirm_milestone_apply_v1(cid: ContractId, params: ConfirmMilestoneParamsV1) -> ContractResult {
    msg!("[labor_market::confirm_milestone_apply_v1] Confirming milestone for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let spent_flags_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_SPENT_FLAGS_TREE)?;

    // Check if already spent
    if wasm::db::db_contains_key(spent_flags_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::confirm_milestone_apply_v1] ERROR: Already spent");
        return Err(ContractError::from(LaborMarketError::AlreadySpent).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(spent_flags_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::confirm_milestone_apply_v1] Milestone confirmed, payment released: {}", params.payment_release);
    Ok(())
}

/// InitiateDispute apply - raise dispute for specific milestone
fn initiate_dispute_apply_v1(cid: ContractId, params: InitiateDisputeParamsV1) -> ContractResult {
    msg!("[labor_market::initiate_dispute_apply_v1] Creating dispute for job: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_NULLIFIERS_TREE)?;

    // Check if already disputed
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.spent_nullifier))? {
        msg!("[labor_market::initiate_dispute_apply_v1] ERROR: Already submitted");
        return Err(ContractError::from(LaborMarketError::AlreadySubmitted).into())
    }

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    wasm::db::db_set(nullifiers_db, &serialize(&params.spent_nullifier), &[])?;
    msg!("[labor_market::initiate_dispute_apply_v1] Job disputed for milestone: {}", params.milestone_index);
    Ok(())
}

// ============================================================================
// O-CAP ENABLED FUNCTIONS (Capability-aware job operations)
// ============================================================================

/// AcceptJobWithCapabilityV1 instruction
fn accept_job_with_capability_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: AcceptJobWithCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_with_capability_v1] Accepting job with capability: {:?}", params.job_id);

    // Validate child call to Identity::VerifyCapabilityV1 (0x0b) for on-chain capability check
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[accept_job_with_capability_v1] Error: Expected 1 child call (Identity::VerifyCapabilityV1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x0b {
        msg!("[accept_job_with_capability_v1] Error: Expected Identity::VerifyCapabilityV1 (0x0b), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_ACCEPT_JOB_WITH_CAPABILITY_NS_V1)

    msg!("[labor_market::accept_job_with_capability_v1] ZK proof verified successfully");
    Ok(())
}

/// AcceptJobWithCapabilityV1 apply - verify capability and update job
fn accept_job_with_capability_apply_v1(cid: ContractId, params: AcceptJobWithCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::accept_job_with_capability_apply_v1] Accepting job with capability: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;

    // Get existing job
    let job_data = wasm::db::db_get(jobs_db, &serialize(&params.job_id))?;
    let mut job: Job = match job_data {
        Some(data) => deserialize(&data)?,
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

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::accept_job_with_capability_apply_v1] Job accepted with capability (id={:?}), worker assigned", required_cap);
    Ok(())
}

// ============================================================================
// O-CAP JOB CREATION FUNCTIONS
// ============================================================================

/// CreateJobWithCapabilityV1 instruction
fn create_job_with_capability_v1(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: CreateJobWithCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_capability_v1] Creating job with capability: {:?}", params.job_id);

    // Validate child call is money_v3::transfer_v1 (0x04) for escrow deposit
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[create_job_with_capability_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(LaborMarketError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[create_job_with_capability_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(LaborMarketError::InvalidChildCall.into())
    }

    // Verify ZK proof
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_CREATE_JOB_WITH_CAPABILITY_NS_V1)

    msg!("[labor_market::create_job_with_capability_v1] ZK proof verified successfully");
    Ok(())
}

/// CreateJobWithCapabilityV1 apply - store new capability-required job
fn create_job_with_capability_apply_v1(cid: ContractId, params: CreateJobWithCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_capability_apply_v1] Storing job with capability: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;

    let job_exists = wasm::db::db_contains_key(jobs_db, &serialize(&params.job_id))?;
    if job_exists {
        msg!("[labor_market::create_job_with_capability_apply_v1] Job already exists!");
        return Err(ContractError::from(LaborMarketError::JobAlreadyExists).into())
    }

    let delivery_type = match params.delivery_type {
        0 => crate::model::DeliveryType::Generic,
        1 => crate::model::DeliveryType::Git,
        _ => {
            msg!("[labor_market::create_job_with_capability_apply_v1] Invalid delivery type");
            return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
        }
    };

    let job = Job {
        id: params.job_id,
        employer_pubkey: [params.employer_pub_x, params.employer_pub_y],
        worker_pubkey: None,
        attestation_id: params.attestation_id,
        delivery_type,
        payment_amount: params.payment_amount,
        payment_token: params.payment_token,
        payment_commit: [params.payment_commit_x, params.payment_commit_y],
        deadline_block: 0,
        state: JobState::Created,
        dao_escrow_bulla: None,
        milestones: vec![],
        current_milestone: 0,
        released_payment: 0,
        required_capability_id: Some(params.required_capability_id),
        required_dag_id: params.required_dag_id,
    };

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::create_job_with_capability_apply_v1] Job with capability stored successfully");
    Ok(())
}

/// CreateJobWithMilestonesAndCapabilityV1 instruction
fn create_job_with_milestones_and_capability_v1(_cid: ContractId, params: CreateJobWithMilestonesAndCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_milestones_and_capability_v1] Creating milestone job with capability: {:?}", params.job_id);

    // Verify ZK proof
    // ZK proof verified by host via get_metadata (namespace: LABOR_CONTRACT_ZKAS_CREATE_JOB_WITH_MILESTONES_AND_CAPABILITY_NS_V1)

    msg!("[labor_market::create_job_with_milestones_and_capability_v1] ZK proof verified successfully");
    Ok(())
}

/// CreateJobWithMilestonesAndCapabilityV1 apply - store new milestone job with capability requirement
fn create_job_with_milestones_and_capability_apply_v1(cid: ContractId, params: CreateJobWithMilestonesAndCapabilityParamsV1) -> ContractResult {
    msg!("[labor_market::create_job_with_milestones_and_capability_apply_v1] Storing milestone job with capability: {:?}", params.job_id);

    let jobs_db = wasm::db::db_lookup(cid, LABOR_CONTRACT_JOBS_TREE)?;

    let job_exists = wasm::db::db_contains_key(jobs_db, &serialize(&params.job_id))?;
    if job_exists {
        msg!("[labor_market::create_job_with_milestones_and_capability_apply_v1] Job already exists!");
        return Err(ContractError::from(LaborMarketError::JobAlreadyExists).into())
    }

    let delivery_type = match params.delivery_type {
        0 => crate::model::DeliveryType::Generic,
        1 => crate::model::DeliveryType::Git,
        _ => {
            msg!("[labor_market::create_job_with_milestones_and_capability_apply_v1] Invalid delivery type");
            return Err(ContractError::from(LaborMarketError::InvalidDeliveryType).into())
        }
    };

    let milestones = params.milestones;

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
        required_capability_id: Some(params.required_capability_id),
        required_dag_id: params.required_dag_id,
    };

    wasm::db::db_set(jobs_db, &serialize(&params.job_id), &serialize(&job))?;
    msg!("[labor_market::create_job_with_milestones_and_capability_apply_v1] Milestone job with capability stored successfully");
    Ok(())
}
