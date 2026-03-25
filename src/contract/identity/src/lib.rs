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

//! DarkFi Identity Contract - Level 0 MVP: Minimal Credential Proofs
//!
//! This contract enables **selective disclosure** of attributes without
//! revealing more than necessary. The core primitive is the "claim" -
//! a ZK proof that certain conditions are met without revealing identity
//! or additional details.
//!
//! ## The Problem: Identity Verification = Surveillance
//!
//! Traditional identity verification requires revealing everything:
//! - Know Your Customer (KYC) reveals your entire identity to the verifier
//!!- OAuth/OIDC reveals your identity to third parties
//!- Proofs of personhood reveal who you are
//!
//! **But sometimes you just need to prove you're over 18, or hold a token,
//! or are a member of a DAO — without revealing WHO you are.**
//!
//! ## Our Solution: Minimal Viable Information (MVI)
//!
//! Release only the **minimum information necessary** for a transaction:
//!
//! ```text
//! Traditional KYC:           DarkFi Identity (MVI):
//! ┌─────────────────┐        ┌─────────────────┐
//! │ Name: Alice      │        │ Age: ✓ (over 18)│
//! │ DOB: 1990-01-01 │   →    │ Residency: ✓    │
//! │ Address: ...    │        │ Not OFAC: ✓     │
//! │ SSN: ...        │        │ Cred: DAO Member│
//! └─────────────────┘        └─────────────────┘
//!     ALL THE DATA                JUST A PROOF
//! ```
//!
//! ## Use Cases
//!
//! - **Age verification**: Prove you're 18+ without revealing birthdate
//! - **DAO membership**: Prove you're a member without revealing who
//! - **Token holding**: Prove you hold ≥X tokens without revealing balance
//! - **Credential verification**: Prove you have a valid credential
//! - **Accredited investor**: Prove income/net-worth without revealing numbers
//! - **Sybil resistance**: Prove you're a unique person without deanonymizing
//!
//! ## How It Works
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                   Identity Contract Flow                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                   │
//! │  ISSUER                                  HOLDER                    │
//! │     │                                        │                    │
//! │     │  1. Issues credential                  │                    │
//! │     │     Credential = H(issuer_key,         │                    │
//! │     │                  attributes,           │                    │
//! │     │                  expiration)           │                    │
//! │     │─────────────────────────────→         │                    │
//! │     │                                        │                    │
//! │     │  2. Holder generates claim             │                    │
//! │     │     Claim = ZKProof{                   │                    │
//! │     │       "I hold valid credential"         │                    │
//! │     │       "age > 18"                        │                    │
//! │     │       credential_nullifier             │                    │
//! │     │     }                                   │                    │
//! │     │←────────────────────────────────────── │                    │
//! │     │                                        │                    │
//! │     │  3. Verifier checks claim             │                    │
//! │     │     ZK proof verifies:                 │                    │
//! │     │       - Credential exists              │                    │
//! │     │       - Conditions met                │                    │
//! │     │       - Nullifier not double-spent    │                    │
//! │     │       - Issuer is trusted             │                    │
//! │     │───────────────────→─────────────────────→ VERIFIER           │
//! │                                                                   │
//! │  RESULT: ✓ or ✗ — NO additional information revealed            │
//! │                                                                   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Privacy Properties
//!
//! | What You Reveal | What Stays Hidden |
//! |------------------|-------------------|
//! | "I meet criteria" | Who you are |
//! | "Credential valid" | Your actual data |
//! | "Not revoked" | When credential expires |
//! | Issuer is trusted | Full credential contents |
//!
//! ## Contract Functions
//!
//! | Function | ID | Description |
//! |----------|-----|-------------|
//! | InitializeV1 | 0x00 | Initialize identity registry |
//! | IssueCredentialV1 | 0x01 | Issuer issues a credential |
//! | RevokeCredentialV1 | 0x02 | Issuer revokes a credential |
//! | CreateClaimV1 | 0x03 | Generate claim from credential |
//! | VerifyClaimV1 | 0x04 | Verify a claim (on-chain) |
//!
//! ## Future Expansion
//!
//! - Level 1: Multiple issuers, credential chaining
//! - Level 2: Anonymous credentials (CL signatures)
//! - Level 3: Self-sovereign identity with revocation

use darkfi_sdk::define_contract_function;

/// Functions available in the Identity contract
define_contract_function!(IdentityFunction {
    InitializeV1 = 0x00,
    IssueCredentialV1 = 0x01,
    RevokeCredentialV1 = 0x02,
    CreateClaimV1 = 0x03,
    VerifyClaimV1 = 0x04,
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

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Tree for issued credentials
pub const IDENTITY_CONTRACT_CREDENTIALS_TREE: &str = "credentials";
/// Tree for credential nullifiers (for revocation/non-reuse)
pub const IDENTITY_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Tree for trusted issuers
pub const IDENTITY_CONTRACT_ISSUERS_TREE: &str = "issuers";
/// Tree for configuration
pub const IDENTITY_CONTRACT_CONFIG_TREE: &str = "config";

// ============================================================================
// KEYS
// ============================================================================

/// Database version key
pub const IDENTITY_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Revocation registry key
pub const IDENTITY_CONTRACT_REVOCATION_LIST: &[u8] = b"revocation_list";
/// Protocol version
pub const IDENTITY_CONTRACT_VERSION: &[u8] = b"protocol_version";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// Credential issuance circuit namespace
pub const IDENTITY_CONTRACT_ZKAS_ISSUE_NS_V1: &str = "IssueCredential_V1";
/// Claim generation circuit namespace
pub const IDENTITY_CONTRACT_ZKAS_CLAIM_NS_V1: &str = "CreateClaim_V1";
/// Claim verification circuit namespace
pub const IDENTITY_CONTRACT_ZKAS_VERIFY_NS_V1: &str = "VerifyClaim_V1";