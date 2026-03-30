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

//! Attestation contract integration tests

use darkfi_attestation_contract::{
    model::{
        Attestation, AttestationState, Claim, ClaimId, ClaimState, CreateAttestationParamsV1,
        CreateAttestationUpdateV1, CreateClaimParamsV1, CreateClaimUpdateV1,
        ExpireAttestationParamsV1, ExpireAttestationUpdateV1, Predicate, RevokeAttestationParamsV1,
        RevokeAttestationUpdateV1, ValidateClaimParamsV1, ValidateClaimUpdateV1,
        VerifyClaimParamsV1, VerifyClaimUpdateV1, ConsumeClaimParamsV1, ConsumeClaimUpdateV1,
    },
    AttestationFunction,
    // Constants
    ATTESTATION_CONTRACT_ATTESTATIONS_TREE, ATTESTATION_CONTRACT_CLAIMS_TREE,
    ATTESTATION_CONTRACT_NULLIFIERS_TREE, ATTESTATION_CONTRACT_INDEX_TREE,
};

#[test]
fn test_attestation_function_enum_valid() {
    assert!(AttestationFunction::try_from(0x00).is_ok()); // CreateAttestationV1
    assert!(AttestationFunction::try_from(0x01).is_ok()); // RevokeAttestationV1
    assert!(AttestationFunction::try_from(0x02).is_ok()); // ExpireAttestationV1
    assert!(AttestationFunction::try_from(0x03).is_ok()); // CreateClaimV1
    assert!(AttestationFunction::try_from(0x04).is_ok()); // VerifyClaimV1
    assert!(AttestationFunction::try_from(0x05).is_ok()); // ConsumeClaimV1
    assert!(AttestationFunction::try_from(0x06).is_ok()); // ValidateClaimV1
}

#[test]
fn test_attestation_function_enum_invalid() {
    assert!(AttestationFunction::try_from(0xFF).is_err());
    assert!(AttestationFunction::try_from(0x07).is_err());
    assert!(AttestationFunction::try_from(0x10).is_err());
}

#[test]
fn test_attestation_state_from_u8() {
    assert_eq!(AttestationState::try_from(0), Ok(AttestationState::Active));
    assert_eq!(AttestationState::try_from(1), Ok(AttestationState::Revoked));
    assert_eq!(AttestationState::try_from(2), Ok(AttestationState::Expired));
    assert!(AttestationState::try_from(3).is_err());
    assert!(AttestationState::try_from(255).is_err());
}

#[test]
fn test_claim_state_from_u8() {
    assert_eq!(ClaimState::try_from(0), Ok(ClaimState::Pending));
    assert_eq!(ClaimState::try_from(1), Ok(ClaimState::Verified));
    assert_eq!(ClaimState::try_from(2), Ok(ClaimState::Consumed));
    assert_eq!(ClaimState::try_from(3), Ok(ClaimState::Rejected));
    assert!(ClaimState::try_from(4).is_err());
    assert!(ClaimState::try_from(255).is_err());
}

#[test]
fn test_predicate_from_u8() {
    assert_eq!(Predicate::try_from(0), Ok(Predicate::Matches));
    assert_eq!(Predicate::try_from(1), Ok(Predicate::GreaterOrEqual));
    assert_eq!(Predicate::try_from(2), Ok(Predicate::LessOrEqual));
    assert_eq!(Predicate::try_from(3), Ok(Predicate::Contains));
    assert_eq!(Predicate::try_from(4), Ok(Predicate::Custom));
    assert!(Predicate::try_from(5).is_err());
    assert!(Predicate::try_from(255).is_err());
}

#[test]
fn test_attestation_derive_id() {
    let attestor_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let claim_type = Predicate::Matches;
    let claim_data = vec![darkfi_sdk::pasta::pallas::Base::from(1), darkfi_sdk::pasta::pallas::Base::from(2)];
    let attestor_secret = darkfi_sdk::pasta::pallas::Base::from(42);

    let id = Attestation::derive_id(&attestor_pubkey, claim_type, &claim_data, attestor_secret);

    // Should be deterministic
    let id2 = Attestation::derive_id(&attestor_pubkey, claim_type, &claim_data, attestor_secret);
    assert_eq!(id, id2);

    // Different input should produce different ID
    let id_different = Attestation::derive_id(
        &attestor_pubkey,
        claim_type,
        &vec![darkfi_sdk::pasta::pallas::Base::from(3)],
        attestor_secret,
    );
    assert_ne!(id, id_different);
}

#[test]
fn test_attestation_encoding() {
    let attestation = Attestation {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestor_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        attestor_secret: darkfi_sdk::pasta::pallas::Base::from(2),
        claim_type: Predicate::Matches,
        claim_data: vec![darkfi_sdk::pasta::pallas::Base::from(1), darkfi_sdk::pasta::pallas::Base::from(2)],
        metadata: vec![1, 2, 3],
        state: AttestationState::Active,
        created_at: 50000,
        expires_at: Some(100000),
    };

    let encoded = attestation.encode().unwrap();
    let decoded = Attestation::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, attestation.id);
    assert_eq!(decoded.claim_type, attestation.claim_type);
    assert_eq!(decoded.state, attestation.state);
    assert_eq!(decoded.created_at, attestation.created_at);
    assert_eq!(decoded.expires_at, attestation.expires_at);
}

#[test]
fn test_claim_encoding() {
    let claim = Claim {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(2),
        claimant_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        claimant_secret: darkfi_sdk::pasta::pallas::Base::from(3),
        predicate: Predicate::GreaterOrEqual,
        evidence_commitment: vec![1, 2, 3],
        revealed_result: vec![4, 5, 6],
        proof: vec![7, 8, 9],
        state: ClaimState::Pending,
        created_at: 50000,
        consumed_at: None,
    };

    let encoded = claim.encode().unwrap();
    let decoded = Claim::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, claim.id);
    assert_eq!(decoded.predicate, claim.predicate);
    assert_eq!(decoded.state, claim.state);
    assert_eq!(decoded.created_at, claim.created_at);
    assert_eq!(decoded.consumed_at, claim.consumed_at);
}

#[test]
fn test_create_attestation_params_encoding() {
    let params = CreateAttestationParamsV1 {
        proof: vec![1, 2, 3],
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestor_pub_x: darkfi_sdk::pasta::pallas::Base::from(2),
        attestor_pub_y: darkfi_sdk::pasta::pallas::Base::from(3),
        claim_type: Predicate::Matches,
        claim_data: vec![darkfi_sdk::pasta::pallas::Base::from(4)],
        metadata: vec![5, 6],
        expires_at: Some(100000),
    };

    let encoded = params.encode().unwrap();
    let decoded = CreateAttestationParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_type, params.claim_type);
    assert_eq!(decoded.expires_at, params.expires_at);
}

#[test]
fn test_create_attestation_update_encoding() {
    let update = CreateAttestationUpdateV1 {
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = CreateAttestationUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_revoke_attestation_params_encoding() {
    let params = RevokeAttestationParamsV1 {
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestor_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
    };

    let encoded = params.encode().unwrap();
    let decoded = RevokeAttestationParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_revoke_attestation_update_encoding() {
    let update = RevokeAttestationUpdateV1 {
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = RevokeAttestationUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_expire_attestation_params_encoding() {
    let params = ExpireAttestationParamsV1 {
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = params.encode().unwrap();
    let decoded = ExpireAttestationParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_expire_attestation_update_encoding() {
    let update = ExpireAttestationUpdateV1 {
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = ExpireAttestationUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_create_claim_params_encoding() {
    let params = CreateClaimParamsV1 {
        proof: vec![1, 2, 3],
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(2),
        claimant_pub_x: darkfi_sdk::pasta::pallas::Base::from(3),
        claimant_pub_y: darkfi_sdk::pasta::pallas::Base::from(4),
        predicate: Predicate::LessOrEqual,
        evidence_commitment: vec![5, 6, 7],
        revealed_result: vec![8, 9],
    };

    let encoded = params.encode().unwrap();
    let decoded = CreateClaimParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.predicate, params.predicate);
}

#[test]
fn test_create_claim_update_encoding() {
    let update = CreateClaimUpdateV1 {
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = CreateClaimUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
}

#[test]
fn test_verify_claim_params_encoding() {
    let params = VerifyClaimParamsV1 {
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = params.encode().unwrap();
    let decoded = VerifyClaimParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_verify_claim_update_encoding() {
    let update = VerifyClaimUpdateV1 {
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
        verified: true,
    };

    let encoded = update.encode().unwrap();
    let decoded = VerifyClaimUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
    assert_eq!(decoded.verified, update.verified);
}

#[test]
fn test_consume_claim_params_encoding() {
    let params = ConsumeClaimParamsV1 {
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(2),
        claimant_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        nullifier: darkfi_sdk::pasta::pallas::Base::from(3),
    };

    let encoded = params.encode().unwrap();
    let decoded = ConsumeClaimParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.nullifier, params.nullifier);
}

#[test]
fn test_consume_claim_update_encoding() {
    let update = ConsumeClaimUpdateV1 {
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = ConsumeClaimUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
}

#[test]
fn test_validate_claim_params_encoding() {
    let params = ValidateClaimParamsV1 {
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
        attestation_id: darkfi_sdk::pasta::pallas::Base::from(2),
        evidence: vec![darkfi_sdk::pasta::pallas::Base::from(3), darkfi_sdk::pasta::pallas::Base::from(4)],
    };

    let encoded = params.encode().unwrap();
    let decoded = ValidateClaimParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.evidence.len(), params.evidence.len());
}

#[test]
fn test_validate_claim_update_encoding() {
    let update = ValidateClaimUpdateV1 {
        claim_id: darkfi_sdk::pasta::pallas::Base::from(1),
        valid: true,
    };

    let encoded = update.encode().unwrap();
    let decoded = ValidateClaimUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
    assert_eq!(decoded.valid, update.valid);
}

#[test]
fn test_constants() {
    assert_eq!(ATTESTATION_CONTRACT_ATTESTATIONS_TREE, "attestations");
    assert_eq!(ATTESTATION_CONTRACT_CLAIMS_TREE, "claims");
    assert_eq!(ATTESTATION_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(ATTESTATION_CONTRACT_INDEX_TREE, "attestation_index");
}