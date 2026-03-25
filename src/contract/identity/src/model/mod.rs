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

//! Data structures for Identity Contract
//!
//! ## Minimal Viable Information (MVI) Design
//!
//! The key principle: **reveal less than you think you need**.
//!
//! Example: Verifying someone is a DAO member
//! - BAD: Reveal their wallet address, token balance, voting history
//! - GOOD: ZK proof that "this address holds ≥1 token in this DAO"
//!
//! The credential system allows issuers to make claims about holders,
//! and holders to prove those claims without revealing more.

// ============================================================================
// ATTRIBUTE TYPES
// ============================================================================
//
// We use a tagged format for attributes to allow flexible schemas.
// Each attribute is tagged so verifiers know what they're checking.
// ============================================================================

use darkfi_serial::{SerialDecodable, SerialEncodable};
use darkfi_sdk::crypto::{IntentCommitment, IntentNullifier};

/// Namespace for identity intents (used with generic intent primitives)
pub const IDENTITY_NAMESPACE: u64 = 0x0001;

/// Supported attribute types for credentials
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum AttributeType {
    /// Boolean attribute (e.g., "is adult", "is citizen")
    Boolean,
    /// Numeric attribute (e.g., "age", "score")
    Numeric,
    /// String attribute (e.g., "country", "role")
    String,
    /// Timestamp attribute (e.g., "expiration")
    Timestamp,
    /// Hash attribute (e.g., "credential hash")
    Hash,
}

/// A single attribute in a credential
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Attribute {
    /// Attribute type
    pub attribute_type: AttributeType,
    /// Attribute name/label
    pub name: Vec<u8>,
    /// Attribute value (encoded based on type)
    pub value: Vec<u8>,
}

/// Credential schema defining required and optional attributes
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CredentialSchema {
    /// Schema name
    pub name: Vec<u8>,
    /// Schema version
    pub version: u32,
    /// Required attributes
    pub required_attributes: Vec<Attribute>,
    /// Optional attributes
    pub optional_attributes: Vec<Attribute>,
}

// ============================================================================
// CREDENTIAL STRUCTURES
// ============================================================================

/// Initialize contract parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    /// Contract version
    pub version: u32,
}

/// Issue credential parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueCredentialParams {
    /// Issuer's public key
    pub issuer_pub: [u8; 32],

    /// Holder's public key (committed)
    pub holder_pub: [u8; 32],

    /// Credential schema hash
    pub schema_hash: [u8; 32],

    /// Encrypted attributes (encrypted to holder)
    /// Only holder can decrypt and use
    pub encrypted_attributes: Vec<u8>,

    /// Credential commitment (uses generic PrivateIntent commitment)
    /// commitment = poseidon_hash([9001, owner_x, owner_y, namespace, payload_hash, expiry, nonce, blind])
    pub commitment: IntentCommitment,

    /// Credential nullifier (for non-revocation/consumption)
    /// nullifier = poseidon_hash([9002, owner_secret, namespace, nonce, commitment])
    pub nullifier: IntentNullifier,

    /// Issuance timestamp
    pub issued_at: u64,

    /// Expiration timestamp (0 = never expires)
    pub expires_at: u64,

    /// ZK proof that credential is valid
    pub proof: Vec<u8>,

    /// Fee paid for issuance
    pub fee: u64,
}

/// Revoke credential parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevokeCredentialParams {
    /// Issuer's signature on revocation
    pub issuer_sig: Vec<u8>,

    /// The nullifier of the credential being revoked
    pub nullifier: IntentNullifier,

    /// Reason for revocation (encrypted)
    pub reason: Vec<u8>,

    /// Fee paid for revocation
    pub fee: u64,
}

/// Create claim parameters
/// This is typically done off-chain by the holder
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateClaimParams {
    /// The credential nullifier
    pub nullifier: IntentNullifier,

    /// The claim type (e.g., "age_over_18", "dao_member")
    pub claim_type: Vec<u8>,

    /// Predicate for the claim (e.g., ">= 18", "== 1")
    pub predicate: Vec<u8>,

    /// The attributes being revealed in this claim
    /// (not the actual values, just which ones)
    pub revealed_attributes: Vec<Vec<u8>>,

    /// ZK proof for the claim
    pub proof: Vec<u8>,

    /// Fee paid for claim creation (if on-chain)
    pub fee: u64,
}

/// Verify claim parameters
/// This is typically called by a verifier
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyClaimParams {
    /// The claim to verify
    pub claim: Claim,

    /// The verifier's public key
    pub verifier_pub: [u8; 32],

    /// Fee paid for verification
    pub fee: u64,
}

/// A generated claim ready for verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Credential nullifier (proves credential exists)
    pub nullifier: IntentNullifier,

    /// Issuer's public key (who issued this credential)
    pub issuer_pub: [u8; 32],

    /// Claim type identifier
    pub claim_type: [u8; 32],

    /// Predicate result (the minimal info revealed)
    /// e.g., for age: just "true" or "false"
    /// e.g., for DAO member: just "is_member" or "not_member"
    pub predicate_result: Vec<u8>,

    /// Revealed attribute names
    pub revealed_attributes: Vec<Vec<u8>>,

    /// ZK proof
    pub proof: Vec<u8>,

    /// Timestamp when claim was created
    pub created_at: u64,

    /// Claim expiration (if any)
    pub expires_at: u64,
}

/// Stored credential record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Credential {
    /// Credential nullifier
    pub nullifier: IntentNullifier,

    /// Issuer's public key
    pub issuer_pub: [u8; 32],

    /// Holder's public key
    pub holder_pub: [u8; 32],

    /// Schema hash
    pub schema_hash: [u8; 32],

    /// Commitment
    pub commitment: IntentCommitment,

    /// Whether this credential is revoked
    pub revoked: bool,

    /// Issuance timestamp
    pub issued_at: u64,

    /// Expiration timestamp
    pub expires_at: u64,
}

/// Trusted issuer record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Issuer {
    /// Issuer's public key
    pub pub_key: [u8; 32],

    /// Issuer's name (e.g., "DarkFi DAO")
    pub name: Vec<u8>,

    /// Schema hashes this issuer can issue
    pub authorized_schemas: Vec<[u8; 32]>,

    /// Whether this issuer is currently trusted
    pub trusted: bool,
}

// ============================================================================
// MINIMAL VIABLE INFORMATION EXAMPLES
// ============================================================================
//
// These examples show how different claims reveal only the minimum necessary
//
// EXAMPLE 1: Age Verification
// ---------------------------
// Credential contains: { DOB: 1990-01-01, country: "US", id_hash: H(SSN) }
//
// BAD (traditional): Reveal DOB, full name, address
// GOOD (MVI): Prove age >= 18 with ZK
//   Claim: { predicate_result: true }
//   Reveals: Only "user is over 18" - nothing else
//
// EXAMPLE 2: DAO Membership
// -----------------------
// Credential contains: { token_balance: 1000, wallet: 0x..., votes: 15 }
//
// BAD (traditional): Reveal wallet address, exact balance
// GOOD (MVI): Prove membership with balance >= 1
//   Claim: { predicate_result: true, revealed: ["is_member"] }
//   Reveals: Only "user holds >= 1 token" - balance hidden
//
// EXAMPLE 3: Accredited Investor
// ------------------------------
// Credential contains: { income: $250k, net_worth: $1M, accredited: true }
//
// BAD (traditional): Reveal exact income and net worth
// GOOD (MVI): Prove accredited = true only
//   Claim: { predicate_result: true }
//   Reveals: Only "user is accredited" - amounts hidden
//
// ============================================================================