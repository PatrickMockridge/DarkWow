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

//! Labor Market contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::pasta::pallas;
use dwow_labor_market_contract::{
    model::{
        AcceptJobParamsV1, CancelJobParamsV1, ConfirmDeliveryParamsV1, CreateJobParamsV1,
        CreateJobWithCapabilityParamsV1, CreateJobWithMilestonesAndCapabilityParamsV1,
        CreateJobWithMilestonesParamsV1, DeliveryType, DisputeParamsV1, InitiateDisputeParamsV1,
        Job, JobState, Milestone, RefundParamsV1, SubmitDeliverableParamsV1,
        SubmitGitDeliverableParamsV1, SubmitMilestoneDeliverableParamsV1, ConfirmMilestoneParamsV1,
        AcceptJobWithCapabilityParamsV1,
    },
    LaborMarketFunction,
    // Constants
    LABOR_CONTRACT_JOBS_TREE, LABOR_CONTRACT_SPENT_FLAGS_TREE,
    LABOR_CONTRACT_INFO_TREE, LABOR_CONTRACT_NULLIFIERS_TREE,
};

#[test]
fn test_labor_market_function_enum_valid() {
    assert!(LaborMarketFunction::try_from(0x00).is_ok()); // CreateJobV1
    assert!(LaborMarketFunction::try_from(0x01).is_ok()); // AcceptJobV1
    assert!(LaborMarketFunction::try_from(0x02).is_ok()); // SubmitDeliverableV1
    assert!(LaborMarketFunction::try_from(0x03).is_ok()); // SubmitGitDeliverableV1
    assert!(LaborMarketFunction::try_from(0x04).is_ok()); // ConfirmDeliveryV1
    assert!(LaborMarketFunction::try_from(0x05).is_ok()); // DisputeV1
    assert!(LaborMarketFunction::try_from(0x06).is_ok()); // RefundV1
    assert!(LaborMarketFunction::try_from(0x07).is_ok()); // CancelV1
    assert!(LaborMarketFunction::try_from(0x08).is_ok()); // CreateJobWithMilestonesV1
    assert!(LaborMarketFunction::try_from(0x09).is_ok()); // SubmitMilestoneV1
    assert!(LaborMarketFunction::try_from(0x0a).is_ok()); // ConfirmMilestoneV1
    assert!(LaborMarketFunction::try_from(0x0b).is_ok()); // InitiateDisputeV1
    assert!(LaborMarketFunction::try_from(0x0c).is_ok()); // CreateJobWithCapabilityV1
    assert!(LaborMarketFunction::try_from(0x0d).is_ok()); // AcceptJobWithCapabilityV1
    assert!(LaborMarketFunction::try_from(0x0e).is_ok()); // CreateJobWithMilestonesAndCapabilityV1
}

#[test]
fn test_labor_market_function_enum_invalid() {
    assert!(LaborMarketFunction::try_from(0xFF).is_err());
    assert!(LaborMarketFunction::try_from(0x0f).is_err());
    assert!(LaborMarketFunction::try_from(0x10).is_err());
}

#[test]
fn test_delivery_type_default() {
    assert_eq!(DeliveryType::default(), DeliveryType::Generic);
}

#[test]
fn test_job_state_default() {
    assert_eq!(JobState::default(), JobState::Created);
}

// Note: JobState and DeliveryType do not implement TryFrom<u8> or From<u8>
// They only implement SerialEncodable/SerialDecodable for binary serialization

#[test]
fn test_milestone_default() {
    let milestone = Milestone::default();
    assert_eq!(milestone.index, 0);
    assert_eq!(milestone.payment_amount, 0);
    assert_eq!(milestone.deadline_block, 0);
    assert!(!milestone.completed);
    assert!(milestone.completed_at_block.is_none());
}

#[test]
fn test_job_encoding() {
    let job = Job {
        id: pallas::Base::from(1),
        employer_pubkey: [
            pallas::Base::from(2),
            pallas::Base::from(3),
        ],
        worker_pubkey: None,
        attestation_id: pallas::Base::from(4),
        delivery_type: DeliveryType::Generic,
        payment_amount: 1000,
        payment_token: pallas::Base::from(1),
        payment_commit: [
            pallas::Base::from(5),
            pallas::Base::from(6),
        ],
        deadline_block: 100000,
        state: JobState::Created,
        dao_escrow_bulla: None,
        milestones: vec![],
        current_milestone: 0,
        released_payment: 0,
        required_capability_id: None,
        required_dag_id: None,
    };

    let encoded = serialize(&job);
    let decoded: Job = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, job.id);
    assert_eq!(decoded.payment_amount, job.payment_amount);
    assert_eq!(decoded.state, job.state);
    assert_eq!(decoded.delivery_type, job.delivery_type);
    assert_eq!(decoded.milestones.len(), job.milestones.len());
    if !job.milestones.is_empty() {
        assert_eq!(decoded.milestones[0].index, job.milestones[0].index);
        assert_eq!(decoded.milestones[0].payment_amount, job.milestones[0].payment_amount);
        assert_eq!(decoded.milestones[0].completed, job.milestones[0].completed);
    }
    assert_eq!(decoded.current_milestone, job.current_milestone);
    assert_eq!(decoded.released_payment, job.released_payment);
    assert_eq!(decoded.required_capability_id, job.required_capability_id);
    assert_eq!(decoded.required_dag_id, job.required_dag_id);
}

#[test]
fn test_job_with_milestones_encoding() {
    let job = Job {
        id: pallas::Base::from(1),
        employer_pubkey: [
            pallas::Base::from(2),
            pallas::Base::from(3),
        ],
        worker_pubkey: Some([
            pallas::Base::from(10),
            pallas::Base::from(11),
        ]),
        attestation_id: pallas::Base::from(4),
        delivery_type: DeliveryType::Generic,
        payment_amount: 3000,
        payment_token: pallas::Base::from(1),
        payment_commit: [
            pallas::Base::from(5),
            pallas::Base::from(6),
        ],
        deadline_block: 100000,
        state: JobState::InProgress,
        dao_escrow_bulla: Some(pallas::Base::from(7)),
        milestones: vec![
            Milestone {
                index: 0,
                payment_amount: 1000,
                deadline_block: 30000,
                completed: true,
                completed_at_block: Some(25000),
            },
            Milestone {
                index: 1,
                payment_amount: 2000,
                deadline_block: 60000,
                completed: false,
                completed_at_block: None,
            },
        ],
        current_milestone: 1,
        released_payment: 1000,
        required_capability_id: None,
        required_dag_id: None,
    };

    let encoded = serialize(&job);
    let decoded: Job = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, job.id);
    assert_eq!(decoded.payment_amount, job.payment_amount);
    assert_eq!(decoded.state, job.state);
    assert_eq!(decoded.current_milestone, 1);
    assert_eq!(decoded.released_payment, 1000);
    assert_eq!(decoded.milestones.len(), 2);
    assert_eq!(decoded.milestones[0].completed, true);
    assert_eq!(decoded.milestones[1].completed, false);
}

#[test]
fn test_job_with_capability_encoding() {
    let job = Job {
        id: pallas::Base::from(1),
        employer_pubkey: [
            pallas::Base::from(2),
            pallas::Base::from(3),
        ],
        worker_pubkey: None,
        attestation_id: pallas::Base::from(4),
        delivery_type: DeliveryType::Git,
        payment_amount: 5000,
        payment_token: pallas::Base::from(1),
        payment_commit: [
            pallas::Base::from(5),
            pallas::Base::from(6),
        ],
        deadline_block: 200000,
        state: JobState::Created,
        dao_escrow_bulla: None,
        milestones: vec![],
        current_milestone: 0,
        released_payment: 0,
        required_capability_id: Some([1u8; 32]),
        required_dag_id: Some([2u8; 32]),
    };

    let encoded = serialize(&job);
    let decoded: Job = deserialize(&encoded).unwrap();

    assert_eq!(decoded.required_capability_id, Some([1u8; 32]));
    assert_eq!(decoded.required_dag_id, Some([2u8; 32]));
}

#[test]
fn test_create_job_params_encoding() {
    let params = CreateJobParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
        attestation_id: pallas::Base::from(4),
        delivery_type: 0,
        payment_amount: 1000,
        payment_token: pallas::Base::from(1),
        payment_commit_x: pallas::Base::from(5),
        payment_commit_y: pallas::Base::from(6),
    };

    let encoded = serialize(&params);
    let decoded: CreateJobParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.payment_amount, params.payment_amount);
    assert_eq!(decoded.delivery_type, params.delivery_type);
}

#[test]
fn test_accept_job_params_encoding() {
    let params = AcceptJobParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        worker_pub_x: pallas::Base::from(2),
        worker_pub_y: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: AcceptJobParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.worker_pub_x, params.worker_pub_x);
}

#[test]
fn test_submit_deliverable_params_encoding() {
    let params = SubmitDeliverableParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        claim_id: pallas::Base::from(2),
        worker_pub_x: pallas::Base::from(3),
        worker_pub_y: pallas::Base::from(4),
        spent_nullifier: pallas::Base::from(5),
    };

    let encoded = serialize(&params);
    let decoded: SubmitDeliverableParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
}

#[test]
fn test_submit_git_deliverable_params_encoding() {
    let params = SubmitGitDeliverableParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        claim_id: pallas::Base::from(2),
        worker_pub_x: pallas::Base::from(3),
        worker_pub_y: pallas::Base::from(4),
        spent_nullifier: pallas::Base::from(5),
    };

    let encoded = serialize(&params);
    let decoded: SubmitGitDeliverableParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.claim_id, params.claim_id);
}

#[test]
fn test_confirm_delivery_params_encoding() {
    let params = ConfirmDeliveryParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
        spent_nullifier: pallas::Base::from(4),
    };

    let encoded = serialize(&params);
    let decoded: ConfirmDeliveryParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
}

#[test]
fn test_dispute_params_encoding() {
    let params = DisputeParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        disputer_pub_x: pallas::Base::from(2),
        disputer_pub_y: pallas::Base::from(3),
        dao_escrow_bulla: pallas::Base::from(4),
        spent_nullifier: pallas::Base::from(5),
    };

    let encoded = serialize(&params);
    let decoded: DisputeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
}

#[test]
fn test_refund_params_encoding() {
    let params = RefundParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
        spent_nullifier: pallas::Base::from(4),
    };

    let encoded = serialize(&params);
    let decoded: RefundParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
}

#[test]
fn test_cancel_job_params_encoding() {
    let params = CancelJobParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: CancelJobParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.employer_pub_x, params.employer_pub_x);
}

#[test]
fn test_create_job_with_milestones_params_encoding() {
    let params = CreateJobWithMilestonesParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
        attestation_id: pallas::Base::from(4),
        delivery_type: 0,
        payment_amount: 3000,
        payment_token: pallas::Base::from(1),
        payment_commit_x: pallas::Base::from(5),
        payment_commit_y: pallas::Base::from(6),
        deadline_block: 100000,
        milestone_count: 3,
    };

    let encoded = serialize(&params);
    let decoded: CreateJobWithMilestonesParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.payment_amount, params.payment_amount);
    assert_eq!(decoded.milestone_count, params.milestone_count);
}

#[test]
fn test_submit_milestone_deliverable_params_encoding() {
    let params = SubmitMilestoneDeliverableParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        milestone_index: 1,
        claim_id: pallas::Base::from(2),
        worker_pub_x: pallas::Base::from(3),
        worker_pub_y: pallas::Base::from(4),
        spent_nullifier: pallas::Base::from(5),
    };

    let encoded = serialize(&params);
    let decoded: SubmitMilestoneDeliverableParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.milestone_index, params.milestone_index);
    assert_eq!(decoded.claim_id, params.claim_id);
}

#[test]
fn test_confirm_milestone_params_encoding() {
    let params = ConfirmMilestoneParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        milestone_index: 0,
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
        payment_release: 1000,
        spent_nullifier: pallas::Base::from(4),
    };

    let encoded = serialize(&params);
    let decoded: ConfirmMilestoneParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.milestone_index, params.milestone_index);
    assert_eq!(decoded.payment_release, params.payment_release);
}

#[test]
fn test_initiate_dispute_params_encoding() {
    let params = InitiateDisputeParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        milestone_index: 1,
        disputer_pub_x: pallas::Base::from(2),
        disputer_pub_y: pallas::Base::from(3),
        dao_escrow_bulla: pallas::Base::from(4),
        spent_nullifier: pallas::Base::from(5),
    };

    let encoded = serialize(&params);
    let decoded: InitiateDisputeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.milestone_index, params.milestone_index);
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
}

#[test]
fn test_create_job_with_capability_params_encoding() {
    let params = CreateJobWithCapabilityParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
        attestation_id: pallas::Base::from(4),
        delivery_type: 1,
        payment_amount: 5000,
        payment_token: pallas::Base::from(1),
        payment_commit_x: pallas::Base::from(5),
        payment_commit_y: pallas::Base::from(6),
        required_capability_id: [1u8; 32],
        required_dag_id: Some([2u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: CreateJobWithCapabilityParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.required_capability_id, params.required_capability_id);
    assert_eq!(decoded.required_dag_id, params.required_dag_id);
}

#[test]
fn test_accept_job_with_capability_params_encoding() {
    let params = AcceptJobWithCapabilityParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        worker_pub_x: pallas::Base::from(2),
        worker_pub_y: pallas::Base::from(3),
        capability_proof: vec![4, 5, 6],
        capability_secret: [7u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: AcceptJobWithCapabilityParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.capability_secret, params.capability_secret);
}

#[test]
fn test_create_job_with_milestones_and_capability_params_encoding() {
    let params = CreateJobWithMilestonesAndCapabilityParamsV1 {
        proof: vec![1, 2, 3],
        job_id: pallas::Base::from(1),
        employer_pub_x: pallas::Base::from(2),
        employer_pub_y: pallas::Base::from(3),
        attestation_id: pallas::Base::from(4),
        delivery_type: 0,
        payment_amount: 6000,
        payment_token: pallas::Base::from(1),
        payment_commit_x: pallas::Base::from(5),
        payment_commit_y: pallas::Base::from(6),
        deadline_block: 150000,
        milestone_count: 2,
        required_capability_id: [1u8; 32],
        required_dag_id: None,
    };

    let encoded = serialize(&params);
    let decoded: CreateJobWithMilestonesAndCapabilityParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.payment_amount, params.payment_amount);
    assert_eq!(decoded.milestone_count, params.milestone_count);
    assert_eq!(decoded.required_capability_id, params.required_capability_id);
    assert_eq!(decoded.required_dag_id, params.required_dag_id);
}

#[test]
fn test_constants() {
    assert_eq!(LABOR_CONTRACT_JOBS_TREE, "jobs");
    assert_eq!(LABOR_CONTRACT_SPENT_FLAGS_TREE, "spent_flags");
    assert_eq!(LABOR_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(LABOR_CONTRACT_INFO_TREE, "info");
}