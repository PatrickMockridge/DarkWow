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

//! DarkFi ZK Insurance Contract
//!
//! Privacy-preserving insurance contract with full policy lifecycle management.
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
//! ## Key Features
//!
//! - **ZK Policy Creation**: Create policies with ZK proofs for privacy
//! - **Coverage Ratio Calculation**: Uses `base_div` for privacy-preserving
//!   approved amount calculation: `approved = verified_loss * coverage_ratio / 10000`
//! - **Block-based Time Locks**: Coverage periods based on block heights
//! - **Claim Lifecycle**: Full claim workflow with approval/rejection
//!
//! ## Trust Model
//!
//! - **Policy Creation**: Policyholder must prove ownership via ZK
//! - **Premium Payment**: Verified via external proof before activation
//! - **Claim Filing**: Policyholder files claim with details hash
//! - **Claim Approval**: Uses `base_div` to calculate approved amount privately
//!
//! ## Integration
//!
//! - **Money::Burn** for premium payments
//! - **Money::TokenMint** for claim payouts
//! - **DAO-Escrow** for premium pool management

use darkfi_sdk::define_contract_function;

/// Functions available in the insurance contract
define_contract_function!(InsuranceFunction {
    // Create a new insurance policy
    CreatePolicyV1 = 0x00,
    // Activate a policy after premium payment
    ActivatePolicyV1 = 0x01,
    // File a claim against an active policy
    FileClaimV1 = 0x02,
    // Approve a pending claim
    ApproveClaimV1 = 0x03,
    // Reject a pending claim
    RejectClaimV1 = 0x04,
    // Pay out an approved claim
    PayClaimV1 = 0x05,
    // Cancel a policy and get refund
    CancelPolicyV1 = 0x06,
});

/// Internal contract errors
pub mod error;

/// Call parameters definitions and data structures
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Policies tree: stores insurance policies indexed by policy_id
pub const INSURANCE_CONTRACT_POLICIES_TREE: &str = "policies";
/// Claims tree: stores insurance claims indexed by claim_id
pub const INSURANCE_CONTRACT_CLAIMS_TREE: &str = "claims";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const INSURANCE_CONTRACT_DB_VERSION: &[u8] = b"db_version";

// ============================================================================
// zkas CIRCUIT NAMESPACES
// ============================================================================

/// CreatePolicy circuit namespace
pub const INSURANCE_CONTRACT_ZKAS_CREATE_POLICY_NS_V1: &str = "CreatePolicy_V1";
/// ActivatePolicy circuit namespace
pub const INSURANCE_CONTRACT_ZKAS_ACTIVATE_POLICY_NS_V1: &str = "ActivatePolicy_V1";
/// FileClaim circuit namespace
pub const INSURANCE_CONTRACT_ZKAS_FILE_CLAIM_NS_V1: &str = "FileClaim_V1";
/// ApproveClaim circuit namespace
pub const INSURANCE_CONTRACT_ZKAS_APPROVE_CLAIM_NS_V1: &str = "ApproveClaim_V1";
/// RejectClaim circuit namespace
pub const INSURANCE_CONTRACT_ZKAS_REJECT_CLAIM_NS_V1: &str = "RejectClaim_V1";
/// PayClaim circuit namespace
pub const INSURANCE_CONTRACT_ZKAS_PAY_CLAIM_NS_V1: &str = "PayClaim_V1";