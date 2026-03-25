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

//! Client-side transaction builders for Identity contract
//!
//! ## How to Use: Minimal Credential Proofs
//!
//! ```ignore
//! // ISSUER: Issue a credential
//! let issue = IssueCredentialBuilder::new()
//!     .issuer_pub(issuer_key)
//!     .holder_pub(holder_key)
//!     .schema_hash(schema.hash())
//!     .encrypted_attributes(encrypt_attributes(attrs, holder_key)?)
//!     .build()?;
//!
//! // HOLDER: Create a claim (off-chain)
//! let claim = CreateClaimBuilder::new()
//!     .nullifier(credential_nullifier)
//!     .claim_type(b"age_over_18")
//!     .predicate(b">= 18")
//!     .revealed_attributes(vec![b"age"])
//!     .build()?;
//!
//! // VERIFIER: Verify the claim
//! let verify = VerifyClaimBuilder::new()
//!     .claim(claim)
//!     .verifier_pub(verifier_key)
//!     .build()?;
//! ```
//!
//! ## Minimal Viable Information (MVI) Principle
//!
//! The key insight: **you often don't need to know WHO someone is,
//! only that they satisfy certain conditions**.
//!
//! Example applications:
//! - "Prove you're over 18" → DOB remains hidden
//! - "Prove you're a DAO member" → wallet remains hidden
//! - "Prove you hold ≥100 tokens" → exact balance hidden
//! - "Prove you're accredited" → income hidden

use darkfi_sdk::error::ClientError;

/// Identity client errors
#[derive(Debug, thiserror::Error)]
pub enum IdentityClientError {
    #[error("Invalid credential: {0}")]
    InvalidCredential(String),

    #[error("Invalid claim: {0}")]
    InvalidClaim(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid ZK proof: {0}")]
    InvalidProof(String),
}

// ============================================================================
// ISSUE CREDENTIAL BUILDER
// ============================================================================

/// Builder for issuing a credential
pub struct IssueCredentialBuilder {
    issuer_pub: Option<[u8; 32]>,
    holder_pub: Option<[u8; 32]>,
    schema_hash: Option<[u8; 32]>,
    encrypted_attributes: Option<Vec<u8>>,
    issued_at: Option<u64>,
    expires_at: Option<u64>,
}

impl IssueCredentialBuilder {
    pub fn new() -> Self {
        Self {
            issuer_pub: None,
            holder_pub: None,
            schema_hash: None,
            encrypted_attributes: None,
            issued_at: None,
            expires_at: None,
        }
    }

    /// Set the issuer's public key
    pub fn issuer_pub(&mut self, pubkey: [u8; 32]) -> &mut Self {
        self.issuer_pub = Some(pubkey);
        self
    }

    /// Set the holder's public key (committed)
    pub fn holder_pub(&mut self, pubkey: [u8; 32]) -> &mut Self {
        self.holder_pub = Some(pubkey);
        self
    }

    /// Set the schema hash
    pub fn schema_hash(&mut self, hash: [u8; 32]) -> &mut Self {
        self.schema_hash = Some(hash);
        self
    }

    /// Set encrypted attributes (encrypted to holder)
    pub fn encrypted_attributes(&mut self, attrs: Vec<u8>) -> &mut Self {
        self.encrypted_attributes = Some(attrs);
        self
    }

    /// Set issuance timestamp
    pub fn issued_at(&mut self, timestamp: u64) -> &mut Self {
        self.issued_at = Some(timestamp);
        self
    }

    /// Set expiration timestamp (0 = never expires)
    pub fn expires_at(&mut self, timestamp: u64) -> &mut Self {
        self.expires_at = Some(timestamp);
        self
    }

    /// Build the issue credential transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let issuer_pub =
            self.issuer_pub.ok_or_else(|| ClientError::InvalidInput("issuer_pub required".into()))?;
        let holder_pub =
            self.holder_pub.ok_or_else(|| ClientError::InvalidInput("holder_pub required".into()))?;
        let schema_hash =
            self.schema_hash.ok_or_else(|| ClientError::InvalidInput("schema_hash required".into()))?;
        let encrypted_attributes = self
            .encrypted_attributes
            .ok_or_else(|| ClientError::InvalidInput("encrypted_attributes required".into()))?;

        // Compute commitment
        let commitment = compute_credential_commitment(
            issuer_pub,
            holder_pub,
            schema_hash,
            &encrypted_attributes,
        );

        // Compute nullifier
        let nullifier = compute_credential_nullifier(issuer_pub, holder_pub);

        // TODO: Generate ZK proof
        let proof = vec![0u8; 64];

        let issued_at = self.issued_at.unwrap_or(0);
        let expires_at = self.expires_at.unwrap_or(0);

        let mut call_data = Vec::new();
        call_data.push(0x01); // IssueCredentialV1
        call_data.extend_from_slice(&issuer_pub);
        call_data.extend_from_slice(&holder_pub);
        call_data.extend_from_slice(&schema_hash);
        call_data.extend_from_slice(&(encrypted_attributes.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&encrypted_attributes);
        call_data.extend_from_slice(&commitment);
        call_data.extend_from_slice(&nullifier);
        call_data.extend_from_slice(&issued_at.to_le_bytes());
        call_data.extend_from_slice(&expires_at.to_le_bytes());
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

/// Compute credential commitment
fn compute_credential_commitment(
    issuer_pub: [u8; 32],
    holder_pub: [u8; 32],
    schema_hash: [u8; 32],
    encrypted_attrs: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"credential_commitment");
    hasher.update(&issuer_pub);
    hasher.update(&holder_pub);
    hasher.update(&schema_hash);
    hasher.update(encrypted_attrs);
    *hasher.finalize().as_bytes()
}

/// Compute credential nullifier
fn compute_credential_nullifier(issuer_pub: [u8; 32], holder_pub: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"credential_nullifier");
    hasher.update(&issuer_pub);
    hasher.update(&holder_pub);
    *hasher.finalize().as_bytes()
}

// ============================================================================
// CREATE CLAIM BUILDER
// ============================================================================

/// Builder for creating a claim from a credential
pub struct CreateClaimBuilder {
    nullifier: Option<[u8; 32]>,
    claim_type: Option<Vec<u8>>,
    predicate: Option<Vec<u8>>,
    revealed_attributes: Option<Vec<Vec<u8>>>,
}

impl CreateClaimBuilder {
    pub fn new() -> Self {
        Self {
            nullifier: None,
            claim_type: None,
            predicate: None,
            revealed_attributes: None,
        }
    }

    /// Set the credential nullifier
    pub fn nullifier(&mut self, nullifier: [u8; 32]) -> &mut Self {
        self.nullifier = Some(nullifier);
        self
    }

    /// Set the claim type (e.g., "age_over_18", "dao_member")
    pub fn claim_type(&mut self, claim_type: Vec<u8>) -> &mut Self {
        self.claim_type = Some(claim_type);
        self
    }

    /// Set the predicate (e.g., ">= 18", "== 1")
    pub fn predicate(&mut self, predicate: Vec<u8>) -> &mut Self {
        self.predicate = Some(predicate);
        self
    }

    /// Set which attributes to reveal
    pub fn revealed_attributes(&mut self, attrs: Vec<Vec<u8>>) -> &mut Self {
        self.revealed_attributes = Some(attrs);
        self
    }

    /// Build the create claim transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let nullifier =
            self.nullifier.ok_or_else(|| ClientError::InvalidInput("nullifier required".into()))?;
        let claim_type =
            self.claim_type.ok_or_else(|| ClientError::InvalidInput("claim_type required".into()))?;
        let predicate =
            self.predicate.ok_or_else(|| ClientError::InvalidInput("predicate required".into()))?;
        let revealed_attributes = self
            .revealed_attributes
            .ok_or_else(|| ClientError::InvalidInput("revealed_attributes required".into()))?;

        // TODO: Generate ZK proof
        let proof = vec![0u8; 64];

        let mut call_data = Vec::new();
        call_data.push(0x03); // CreateClaimV1
        call_data.extend_from_slice(&nullifier);
        call_data.extend_from_slice(&(claim_type.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&claim_type);
        call_data.extend_from_slice(&(predicate.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&predicate);
        call_data.extend_from_slice(&(revealed_attributes.len() as u32).to_le_bytes());
        for attr in &revealed_attributes {
            call_data.extend_from_slice(&(attr.len() as u32).to_le_bytes());
            call_data.extend_from_slice(attr);
        }
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

// ============================================================================
// VERIFY CLAIM BUILDER
// ============================================================================

/// Builder for verifying a claim
pub struct VerifyClaimBuilder {
    claim: Option<Vec<u8>>,
    verifier_pub: Option<[u8; 32]>,
}

impl VerifyClaimBuilder {
    pub fn new() -> Self {
        Self { claim: None, verifier_pub: None }
    }

    /// Set the claim to verify
    pub fn claim(&mut self, claim: Vec<u8>) -> &mut Self {
        self.claim = Some(claim);
        self
    }

    /// Set the verifier's public key
    pub fn verifier_pub(&mut self, pubkey: [u8; 32]) -> &mut Self {
        self.verifier_pub = Some(pubkey);
        self
    }

    /// Build the verify claim transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let claim =
            self.claim.ok_or_else(|| ClientError::InvalidInput("claim required".into()))?;
        let verifier_pub =
            self.verifier_pub.ok_or_else(|| ClientError::InvalidInput("verifier_pub required".into()))?;

        let mut call_data = Vec::new();
        call_data.push(0x04); // VerifyClaimV1
        call_data.extend_from_slice(&claim);
        call_data.extend_from_slice(&verifier_pub);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

// ============================================================================
// REVOKE CREDENTIAL BUILDER
// ============================================================================

/// Builder for revoking a credential
pub struct RevokeCredentialBuilder {
    nullifier: Option<[u8; 32]>,
    reason: Option<Vec<u8>>,
}

impl RevokeCredentialBuilder {
    pub fn new() -> Self {
        Self { nullifier: None, reason: None }
    }

    /// Set the nullifier of the credential to revoke
    pub fn nullifier(&mut self, nullifier: [u8; 32]) -> &mut Self {
        self.nullifier = Some(nullifier);
        self
    }

    /// Set the reason for revocation
    pub fn reason(&mut self, reason: Vec<u8>) -> &mut Self {
        self.reason = Some(reason);
        self
    }

    /// Build the revoke credential transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let nullifier =
            self.nullifier.ok_or_else(|| ClientError::InvalidInput("nullifier required".into()))?;
        let reason =
            self.reason.ok_or_else(|| ClientError::InvalidInput("reason required".into()))?;

        // TODO: Generate issuer signature
        let issuer_sig = vec![0u8; 64];

        let mut call_data = Vec::new();
        call_data.push(0x02); // RevokeCredentialV1
        call_data.extend_from_slice(&(issuer_sig.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&issuer_sig);
        call_data.extend_from_slice(&nullifier);
        call_data.extend_from_slice(&(reason.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&reason);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

// ============================================================================
// MINIMAL VIABLE INFORMATION EXAMPLES
// ============================================================================
//
// These examples show how to use the identity contract for MVI claims
//
// EXAMPLE 1: Age Verification
//
// // Holder proves they're over 18 without revealing DOB
// let claim = CreateClaimBuilder::new()
//     .nullifier(my_credential_nullifier)
//     .claim_type(b"age_over_18".to_vec())
//     .predicate(b">= 18".to_vec())
//     .revealed_attributes(vec![b"age".to_vec()])
//     .build()?;
//
// Result: Verifier learns "age >= 18" — nothing else
//
// EXAMPLE 2: DAO Membership
//
// // Holder proves DAO membership without revealing wallet
// let claim = CreateClaimBuilder::new()
//     .nullifier(my_credential_nullifier)
//     .claim_type(b"dao_member".to_vec())
//     .predicate(b"== 1".to_vec())
//     .revealed_attributes(vec![b"is_member".to_vec()])
//     .build()?;
//
// Result: Verifier learns "holds >= 1 token" — balance hidden
//
// EXAMPLE 3: Token Balance
//
// // Holder proves balance >= 100 without revealing exact amount
// let claim = CreateClaimBuilder::new()
//     .nullifier(my_credential_nullifier)
//     .claim_type(b"token_balance".to_vec())
//     .predicate(b">= 100".to_vec())
//     .revealed_attributes(vec![])
//     .build()?;
//
// Result: Verifier learns "balance >= 100" — exact balance hidden
//
// ============================================================================