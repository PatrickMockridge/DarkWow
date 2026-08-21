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

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]


//! DarkWow Attestation Contract
//!
//! A generalized attestation and claims system that can be used as a module
//! by other contracts for implementing claim verification patterns.
//!
//! ## Core Concepts
//!
//! - **Attestation**: A party's commitment to a claim or condition
//! - **Claim**: A claimant's assertion based on an attestation
//! - **Predicate**: The type of verification required (Matches, >=, <=, etc.)
//!
//! ## Use Cases
//!
//! - Labor Market: Employer attests to deliverable_hash, worker claims completion
//! - Tender: Requester attests to competency requirements, bidders claim competency
//! - Oracle: Attestor attests to external data (push model)
//!
//! ## Flow
//!
//! 1. Attestor creates Attestation with claim_type and claim_data
//! 2. Claimant creates Claim against the Attestation
//! 3. Claim is verified (ZK + on-chain)
//! 4. Claim can be consumed (prevents replay)

use dwow_sdk::define_contract_function;

/// Deterministic ZK mode — TEST-ONLY. `enable_deterministic_zk()` swaps `OsRng` for a
/// seeded RNG so PI-7 determinism can be checked. SECURITY: seed 0 disables zero-knowledge,
/// so the mode is gated behind the `deterministic-zk` cargo feature (harness-only). The
/// wallet/WASM never enable it; `deterministic_zk_enabled()` returns false there.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

pub fn deterministic_zk_enabled() -> bool {
    #[cfg(feature = "deterministic-zk")]
    {
        DETERMINISTIC_ZK.load(Ordering::SeqCst)
    }
    #[cfg(not(feature = "deterministic-zk"))]
    {
        false
    }
}

define_contract_function!(AttestationFunction {
    CreateAttestationV1 = 0x00,
    RevokeAttestationV1 = 0x01,
    ExpireAttestationV1 = 0x02,
    CreateClaimV1 = 0x03,
    VerifyClaimV1 = 0x04,
    ConsumeClaimV1 = 0x05,
    ValidateClaimV1 = 0x06,
    CheckNotRevokedV1 = 0x07,
    DelegateAttestationV1 = 0x08,
    VerifyChainV1 = 0x09,
    UpdateDelegationV1 = 0x0a,
    AttestSlashV1 = 0x0b,
    CommitFeeScheduleV1 = 0x0c,
});

/// Capability descriptor
pub mod capability;
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
pub const ATTESTATION_CONTRACT_ATTESTATIONS_TREE: &str = "attestations";
pub const ATTESTATION_CONTRACT_CLAIMS_TREE: &str = "claims";
pub const ATTESTATION_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const ATTESTATION_CONTRACT_INDEX_TREE: &str = "attestation_index";
pub const ATTESTATION_CONTRACT_RATE_LIMIT_TREE: &str = "claim_rate_limits";
pub const ATTESTATION_CONTRACT_DELEGATIONS_TREE: &str = "delegations";

// These are keys inside the info tree
pub const ATTESTATION_CONTRACT_DB_VERSION: &[u8] = b"db_version";

/// Max age of a slash event that can be attested (blocks, ~7 days at 12s/block)
pub const MAX_SLASH_ATTESTATION_AGE: u64 = 50400;

// zkas circuit namespaces
pub const ATTESTATION_CONTRACT_ZKAS_CREATE_NS_V1: &str = "CreateAttestation";
pub const ATTESTATION_CONTRACT_ZKAS_CREATE_CLAIM_NS_V1: &str = "CreateClaim";
pub const ATTESTATION_CONTRACT_ZKAS_VERIFY_CLAIM_NS_V1: &str = "VerifyClaim";
pub const ATTESTATION_CONTRACT_ZKAS_CONSUME_CLAIM_NS_V1: &str = "ConsumeClaim";
pub const ATTESTATION_CONTRACT_ZKAS_CHECK_NOT_REVOKED_NS_V1: &str = "CheckNotRevoked";
pub const ATTESTATION_CONTRACT_ZKAS_DELEGATE_NS_V1: &str = "DelegateAttestation";
pub const ATTESTATION_CONTRACT_ZKAS_VERIFY_CHAIN_NS_V1: &str = "VerifyChain";
pub const ATTESTATION_CONTRACT_ZKAS_UPDATE_DELEGATION_NS_V1: &str = "UpdateDelegation";
pub const ATTESTATION_CONTRACT_ZKAS_ATTEST_SLASH_NS_V1: &str = "AttestSlashV1";
pub const ATTESTATION_CONTRACT_ZKAS_COMMIT_FEE_SCHEDULE_NS_V1: &str = "CommitFeeScheduleV1";

// V2 circuit namespaces (HAZOP RC3: domain separation)
pub const ATTESTATION_CONTRACT_ZKAS_CREATE_NS_V2: &str = "CreateAttestationV2";
pub const ATTESTATION_CONTRACT_ZKAS_CREATE_CLAIM_NS_V2: &str = "CreateClaimV2";
pub const ATTESTATION_CONTRACT_ZKAS_VERIFY_CLAIM_NS_V2: &str = "VerifyClaimV2";
pub const ATTESTATION_CONTRACT_ZKAS_CONSUME_CLAIM_NS_V2: &str = "ConsumeClaimV2";
pub const ATTESTATION_CONTRACT_ZKAS_CHECK_NOT_REVOKED_NS_V2: &str = "CheckNotRevokedV2";
pub const ATTESTATION_CONTRACT_ZKAS_DELEGATE_NS_V2: &str = "DelegateAttestationV2";
pub const ATTESTATION_CONTRACT_ZKAS_VERIFY_CHAIN_NS_V2: &str = "VerifyChainV2";
pub const ATTESTATION_CONTRACT_ZKAS_UPDATE_DELEGATION_NS_V2: &str = "UpdateDelegationV2";
pub const ATTESTATION_CONTRACT_ZKAS_ATTEST_SLASH_NS_V2: &str = "AttestSlashV2";
pub const ATTESTATION_CONTRACT_ZKAS_COMMIT_FEE_SCHEDULE_NS_V2: &str = "CommitFeeScheduleV2";