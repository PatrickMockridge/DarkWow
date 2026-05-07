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

//! Identity contract integration tests
//!
//! These tests verify the identity contract's:
//! - Function enum parsing
//! - Data structure encoding/decoding
//! - Model type invariants
//!
//! Full ZK integration tests require the contract to be integrated with
//! the test harness infrastructure (see test-harness/src/vks.rs).

use darkfi_identity_contract::{
    model::{Attribute, AttributeType, Claim, CreateClaimParams, CreateClaimParamsL1, Credential, CredentialSchema, InitializeParams, IssueCredentialParams, Issuer},
    IdentityFunction,
};
use dwow_serial::{deserialize, serialize};

/// Helper to create IntentNullifier from bytes
fn make_nullifier(bytes: [u8; 32]) -> dwow_sdk::crypto::IntentNullifier {
    dwow_sdk::crypto::IntentNullifier::from_bytes(bytes).unwrap()
}

/// Helper to create IntentCommitment from bytes
fn make_commitment(bytes: [u8; 32]) -> dwow_sdk::crypto::IntentCommitment {
    dwow_sdk::crypto::IntentCommitment::from_bytes(bytes).unwrap()
}

#[test]
fn test_identity_function_enum_v0() {
    // Test that Level 0 function IDs are valid
    assert!(IdentityFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(IdentityFunction::try_from(0x01).is_ok()); // IssueCredentialV1
    assert!(IdentityFunction::try_from(0x02).is_ok()); // RevokeCredentialV1
    assert!(IdentityFunction::try_from(0x03).is_ok()); // CreateClaimV1
    assert!(IdentityFunction::try_from(0x04).is_ok()); // VerifyClaimV1
}

#[test]
fn test_identity_function_enum_v1_l1() {
    // Test that Level 1 function ID is valid
    assert!(IdentityFunction::try_from(0x05).is_ok()); // CreateClaimV1L1
}

#[test]
fn test_identity_function_enum_invalid() {
    // Test that invalid function IDs return errors
    assert!(IdentityFunction::try_from(0xFF).is_err());
    assert!(IdentityFunction::try_from(0x10).is_err());
    assert!(IdentityFunction::try_from(0x20).is_err());
}

#[test]
fn test_initialize_params_encoding() {
    let params = InitializeParams { version: 1 };

    let encoded = serialize(&params);
    let decoded: InitializeParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.version, params.version);
}

#[test]
fn test_credential_schema_encoding() {
    let schema = CredentialSchema {
        name: b"age_verification".to_vec(),
        version: 1,
        required_attributes: vec![
            Attribute {
                attribute_type: AttributeType::Numeric,
                name: b"age".to_vec(),
                value: 18u64.to_le_bytes().to_vec(),
            },
        ],
        optional_attributes: vec![],
    };

    let encoded = serialize(&schema);
    let decoded: CredentialSchema = deserialize(&encoded).unwrap();

    assert_eq!(decoded.name, schema.name);
    assert_eq!(decoded.version, schema.version);
    assert_eq!(decoded.required_attributes.len(), 1);
}

#[test]
fn test_attribute_types() {
    let bool_attr = Attribute {
        attribute_type: AttributeType::Boolean,
        name: b"is_citizen".to_vec(),
        value: vec![1],
    };

    let num_attr = Attribute {
        attribute_type: AttributeType::Numeric,
        name: b"age".to_vec(),
        value: 25u64.to_le_bytes().to_vec(),
    };

    let str_attr = Attribute {
        attribute_type: AttributeType::String,
        name: b"country".to_vec(),
        value: b"US".to_vec(),
    };

    assert_eq!(bool_attr.attribute_type, AttributeType::Boolean);
    assert_eq!(num_attr.attribute_type, AttributeType::Numeric);
    assert_eq!(str_attr.attribute_type, AttributeType::String);
}

#[test]
fn test_issue_credential_params_encoding() {
    let params = IssueCredentialParams {
        issuer_pub: [1u8; 32],
        holder_pub: [2u8; 32],
        schema_hash: [3u8; 32],
        encrypted_attributes: vec![4u8; 64],
        commitment: make_commitment([5u8; 32]),
        nullifier: make_nullifier([6u8; 32]),
        issued_at: 1000,
        expires_at: 2000,
        proof: vec![7u8; 128],
        fee: 100,
    };

    let encoded = serialize(&params);
    let decoded: IssueCredentialParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.issuer_pub, params.issuer_pub);
    assert_eq!(decoded.holder_pub, params.holder_pub);
    assert_eq!(decoded.issued_at, params.issued_at);
    assert_eq!(decoded.expires_at, params.expires_at);
}

#[test]
fn test_create_claim_params_encoding() {
    let params = CreateClaimParams {
        nullifier: make_nullifier([1u8; 32]),
        claim_type: b"age_over_18".to_vec(),
        predicate: b">= 18".to_vec(),
        revealed_attributes: vec![b"age".to_vec()],
        proof: vec![2u8; 128],
        fee: 50,
    };

    let encoded = serialize(&params);
    let decoded: CreateClaimParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.claim_type, params.claim_type);
    assert_eq!(decoded.predicate, params.predicate);
    assert_eq!(decoded.fee, params.fee);
}

#[test]
fn test_create_claim_params_l1_encoding() {
    let params = CreateClaimParamsL1 {
        nullifier: make_nullifier([1u8; 32]),
        claim_type: b"age_over_18".to_vec(),
        predicate: b">= 18".to_vec(),
        revealed_attributes: vec![b"age".to_vec()],
        proof: vec![2u8; 128],
        predicate_result: 1, // Level 1: predicate satisfied
        fee: 50,
    };

    let encoded = serialize(&params);
    let decoded: CreateClaimParamsL1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.claim_type, params.claim_type);
    assert_eq!(decoded.predicate_result, 1); // Level 1 reveals predicate result
    assert_eq!(decoded.fee, params.fee);
}

#[test]
fn test_credential_encoding() {
    let credential = Credential {
        nullifier: make_nullifier([1u8; 32]),
        issuer_pub: [2u8; 32],
        holder_pub: [3u8; 32],
        schema_hash: [4u8; 32],
        commitment: make_commitment([5u8; 32]),
        revoked: false,
        issued_at: 1000,
        expires_at: 2000,
    };

    let encoded = serialize(&credential);
    let decoded: Credential = deserialize(&encoded).unwrap();

    assert_eq!(decoded.nullifier, credential.nullifier);
    assert_eq!(decoded.revoked, false);
    assert_eq!(decoded.issued_at, 1000);
}

#[test]
fn test_issuer_encoding() {
    let issuer = Issuer {
        pub_key: [1u8; 32],
        name: b"Government DMV".to_vec(),
        authorized_schemas: vec![[2u8; 32], [3u8; 32]],
        trusted: true,
    };

    let encoded = serialize(&issuer);
    let decoded: Issuer = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pub_key, issuer.pub_key);
    assert_eq!(decoded.name, issuer.name);
    assert_eq!(decoded.trusted, true);
    assert_eq!(decoded.authorized_schemas.len(), 2);
}

#[test]
fn test_claim_encoding() {
    let claim = Claim {
        nullifier: make_nullifier([1u8; 32]),
        issuer_pub: [2u8; 32],
        claim_type: [3u8; 32],
        predicate_result: vec![1], // "true"
        revealed_attributes: vec![b"age".to_vec()],
        proof: vec![4u8; 128],
        created_at: 1000,
        expires_at: 2000,
    };

    let encoded = serialize(&claim);
    let decoded: Claim = deserialize(&encoded).unwrap();

    assert_eq!(decoded.nullifier, claim.nullifier);
    assert_eq!(decoded.predicate_result, vec![1]);
    assert_eq!(decoded.created_at, 1000);
}

#[test]
fn test_credential_not_revoked() {
    let credential = Credential {
        nullifier: make_nullifier([1u8; 32]),
        issuer_pub: [2u8; 32],
        holder_pub: [3u8; 32],
        schema_hash: [4u8; 32],
        commitment: make_commitment([5u8; 32]),
        revoked: false,
        issued_at: 1000,
        expires_at: 0, // Never expires
    };

    assert!(!credential.revoked);
    assert_eq!(credential.expires_at, 0);
}

#[test]
fn test_credential_expired() {
    let current_time: u64 = 3000;
    let credential = Credential {
        nullifier: make_nullifier([1u8; 32]),
        issuer_pub: [2u8; 32],
        holder_pub: [3u8; 32],
        schema_hash: [4u8; 32],
        commitment: make_commitment([5u8; 32]),
        revoked: false,
        issued_at: 1000,
        expires_at: 2000, // Expired at 2000
    };

    // Credential should be considered expired if current_time > expires_at
    let is_expired = credential.expires_at > 0 && current_time > credential.expires_at;
    assert!(is_expired);
}

#[test]
fn test_credential_revoked() {
    let credential = Credential {
        nullifier: make_nullifier([1u8; 32]),
        issuer_pub: [2u8; 32],
        holder_pub: [3u8; 32],
        schema_hash: [4u8; 32],
        commitment: make_commitment([5u8; 32]),
        revoked: true,
        issued_at: 1000,
        expires_at: 0,
    };

    assert!(credential.revoked);
}