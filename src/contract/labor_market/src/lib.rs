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

//! DarkWow Labor Market Contract
//!
//! A job/labor market contract using escrow and DAO governance.
//! Enables trustless conditional payments for completed work.
//!
//! Trust model:
//! - Employer creates job, deposits payment in escrow
//! - Worker accepts job, delivers work before deadline
//! - Work verified off-chain (zip hash or git commit hash)
//! - Employer confirms -> payment to worker
//! - Timeout -> refund to employer
//! - Dispute -> DAO governance resolution
//!
//! Delivery types:
//! - Generic: Worker submits hash(zip_file) as proof of work
//! - Git: Worker submits git commit hash as proof of work
//!
//! Privacy properties:
//! - Payment hidden in Pedersen commitment
//! - Parties hidden (public keys derived from secrets)
//! - Dispute reason hashed for privacy

use dwow_sdk::define_contract_function;

define_contract_function!(LaborMarketFunction {
    CreateJobV1 = 0x00,
    AcceptJobV1 = 0x01,
    SubmitDeliverableV1 = 0x02,
    SubmitGitDeliverableV1 = 0x03,
    ConfirmDeliveryV1 = 0x04,
    DisputeV1 = 0x05,
    RefundV1 = 0x06,
    CancelV1 = 0x07,
    CreateJobWithMilestonesV1 = 0x08,
    SubmitMilestoneV1 = 0x09,
    ConfirmMilestoneV1 = 0x0a,
    InitiateDisputeV1 = 0x0b,
    // O-Cap enabled functions
    CreateJobWithCapabilityV1 = 0x0c,
    AcceptJobWithCapabilityV1 = 0x0d,
    CreateJobWithMilestonesAndCapabilityV1 = 0x0e,
});

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// These are the different sled trees that will be created
pub const LABOR_CONTRACT_JOBS_TREE: &str = "jobs";
pub const LABOR_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const LABOR_CONTRACT_SPENT_FLAGS_TREE: &str = "spent_flags";
pub const LABOR_CONTRACT_INFO_TREE: &str = "info";

// These are keys inside the info tree
pub const LABOR_CONTRACT_DB_VERSION: &[u8] = b"db_version";

// zkas circuit namespaces
pub const LABOR_CONTRACT_ZKAS_CREATE_JOB_NS_V1: &str = "CreateJob";
pub const LABOR_CONTRACT_ZKAS_ACCEPT_JOB_NS_V1: &str = "AcceptJob";
pub const LABOR_CONTRACT_ZKAS_SUBMIT_DELIVERABLE_NS_V1: &str = "SubmitDeliverable";
pub const LABOR_CONTRACT_ZKAS_SUBMIT_GIT_DELIVERABLE_NS_V1: &str = "SubmitGitDeliverable";
pub const LABOR_CONTRACT_ZKAS_CONFIRM_DELIVERY_NS_V1: &str = "ConfirmDelivery";
pub const LABOR_CONTRACT_ZKAS_DISPUTE_NS_V1: &str = "Dispute";
pub const LABOR_CONTRACT_ZKAS_REFUND_NS_V1: &str = "Refund";
pub const LABOR_CONTRACT_ZKAS_ACCEPT_JOB_WITH_CAPABILITY_NS_V1: &str = "AcceptJobWithCapability";
pub const LABOR_CONTRACT_ZKAS_MILESTONE_PAYMENT_NS_V1: &str = "MilestonePayment";
pub const LABOR_CONTRACT_ZKAS_CREATE_JOB_WITH_MILESTONES_NS_V1: &str = "CreateJobWithMilestones";
pub const LABOR_CONTRACT_ZKAS_CREATE_JOB_WITH_MILESTONES_AND_CAPABILITY_NS_V1: &str = "CreateJobWithMilestonesAndCapability";
