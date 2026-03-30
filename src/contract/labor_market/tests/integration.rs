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

//! Labor Market contract integration tests

use darkfi_labor_market_contract::{
    model::{
        AcceptJobParamsV1, CancelJobParamsV1, ConfirmDeliveryParamsV1, CreateJobParamsV1,
        DeliveryType, DisputeParamsV1, Job, JobState, RefundParamsV1, SubmitDeliverableParamsV1,
        SubmitGitDeliverableParamsV1,
    },
    LaborMarketFunction,
    // Constants
    LABOR_CONTRACT_JOBS_TREE, LABOR_CONTRACT_NULLIFIERS_TREE,
    LABOR_CONTRACT_SPENT_FLAGS_TREE, LABOR_CONTRACT_INFO_TREE,
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
}

#[test]
fn test_labor_market_function_enum_invalid() {
    assert!(LaborMarketFunction::try_from(0xFF).is_err());
    assert!(LaborMarketFunction::try_from(0x08).is_err());
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

#[test]
fn test_job_state_from_u8() {
    assert_eq!(JobState::try_from(0), Ok(JobState::Created));
    assert_eq!(JobState::try_from(1), Ok(JobState::InProgress));
    assert_eq!(JobState::try_from(2), Ok(JobState::Delivered));
    assert_eq!(JobState::try_from(3), Ok(JobState::Confirmed));
    assert_eq!(JobState::try_from(4), Ok(JobState::Disputed));
    assert_eq!(JobState::try_from(5), Ok(JobState::Refunded));
    assert_eq!(JobState::try_from(6), Ok(JobState::Cancelled));
    assert!(JobState::try_from(7).is_err());
    assert!(JobState::try_from(255).is_err());
}

#[test]
fn test_delivery_type_from_u8() {
    assert_eq!(DeliveryType::try_from(0), Ok(DeliveryType::Generic));
    assert_eq!(DeliveryType::try_from(1), Ok(DeliveryType::Git));
    assert!(DeliveryType::try_from(2).is_err());
    assert!(DeliveryType::try_from(255).is_err());
}

#[test]
fn test_job_encoding() {
    let job = Job {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        employer_pubkey: [
            darkfi_sdk::pasta::pallas::Base::from(2),
            darkfi_sdk::pasta::pallas::Base::from(3),
        ],
        worker_pubkey: None,
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(4),
        delivery_type: DeliveryType::Generic,
        payment_amount: 1000,
        payment_token: darkfi_sdk::pasta::pallas::Base::ONE,
        payment_commit: [
            darkfi_sdk::pasta::pallas::Base::from(5),
            darkfi_sdk::pasta::pallas::Base::from(6),
        ],
        deadline_block: 100000,
        state: JobState::Created,
        dao_escrow_bulla: None,
    };

    let encoded = bincode::serde::encode_to_vec(&job, bincode::config::standard()).unwrap();
    let decoded: Job = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.id, job.id);
    assert_eq!(decoded.payment_amount, job.payment_amount);
    assert_eq!(decoded.state, job.state);
    assert_eq!(decoded.delivery_type, job.delivery_type);
}

#[test]
fn test_create_job_params_encoding() {
    let params = CreateJobParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        employer_pub_x: darkfi_sdk::pasta::pallas::Base::from(2),
        employer_pub_y: darkfi_sdk::pasta::pallas::Base::from(3),
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(4),
        delivery_type: 0,
        payment_amount: 1000,
        payment_token: darkfi_sdk::pasta::pallas::Base::ONE,
        payment_commit_x: darkfi_sdk::pasta::pallas::Base::from(5),
        payment_commit_y: darkfi_sdk::pasta::pallas::Base::from(6),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: CreateJobParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.payment_amount, params.payment_amount);
    assert_eq!(decoded.delivery_type, params.delivery_type);
}

#[test]
fn test_accept_job_params_encoding() {
    let params = AcceptJobParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        worker_pub_x: darkfi_sdk::pasta::pallas::Base::from(2),
        worker_pub_y: darkfi_sdk::pasta::pallas::Base::from(3),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: AcceptJobParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.worker_pub_x, params.worker_pub_x);
}

#[test]
fn test_submit_deliverable_params_encoding() {
    let params = SubmitDeliverableParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        claim_id: darkfi_sdk::pasta::pallas::Base::from(2),
        worker_pub_x: darkfi_sdk::pasta::pallas::Base::from(3),
        worker_pub_y: darkfi_sdk::pasta::pallas::Base::from(4),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(5),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: SubmitDeliverableParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
}

#[test]
fn test_submit_git_deliverable_params_encoding() {
    let params = SubmitGitDeliverableParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        claim_id: darkfi_sdk::pasta::pallas::Base::from(2),
        worker_pub_x: darkfi_sdk::pasta::pallas::Base::from(3),
        worker_pub_y: darkfi_sdk::pasta::pallas::Base::from(4),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(5),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: SubmitGitDeliverableParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.claim_id, params.claim_id);
}

#[test]
fn test_confirm_delivery_params_encoding() {
    let params = ConfirmDeliveryParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        employer_pub_x: darkfi_sdk::pasta::pallas::Base::from(2),
        employer_pub_y: darkfi_sdk::pasta::pallas::Base::from(3),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(4),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: ConfirmDeliveryParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
}

#[test]
fn test_dispute_params_encoding() {
    let params = DisputeParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        disputer_pub_x: darkfi_sdk::pasta::pallas::Base::from(2),
        disputer_pub_y: darkfi_sdk::pasta::pallas::Base::from(3),
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(4),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(5),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: DisputeParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
}

#[test]
fn test_refund_params_encoding() {
    let params = RefundParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        employer_pub_x: darkfi_sdk::pasta::pallas::Base::from(2),
        employer_pub_y: darkfi_sdk::pasta::pallas::Base::from(3),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(4),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: RefundParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
}

#[test]
fn test_cancel_job_params_encoding() {
    let params = CancelJobParamsV1 {
        proof: vec![1, 2, 3],
        job_id: darkfi_sdk::pasta::pallas::Base::from(1),
        employer_pub_x: darkfi_sdk::pasta::pallas::Base::from(2),
        employer_pub_y: darkfi_sdk::pasta::pallas::Base::from(3),
    };

    let encoded = bincode::serde::encode_to_vec(&params, bincode::config::standard()).unwrap();
    let decoded: CancelJobParamsV1 = bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap().0;

    assert_eq!(decoded.job_id, params.job_id);
    assert_eq!(decoded.employer_pub_x, params.employer_pub_x);
}

#[test]
fn test_constants() {
    assert_eq!(LABOR_CONTRACT_JOBS_TREE, "jobs");
    assert_eq!(LABOR_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(LABOR_CONTRACT_SPENT_FLAGS_TREE, "spent_flags");
    assert_eq!(LABOR_CONTRACT_INFO_TREE, "info");
}