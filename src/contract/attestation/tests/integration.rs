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

//! Attestation contract integration tests

use darkfi_serial::{deserialize, serialize};
use darkfi_sdk::pasta::pallas;
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
    assert!(AttestationFunction::try_from(0x0b).is_err());
    assert!(AttestationFunction::try_from(0x10).is_err());
}

#[test]
fn test_attestation_state_from_u8() {
    assert_eq!(AttestationState::try_from(0).unwrap(), AttestationState::Active);
    assert_eq!(AttestationState::try_from(1).unwrap(), AttestationState::Revoked);
    assert_eq!(AttestationState::try_from(2).unwrap(), AttestationState::Expired);
    assert!(AttestationState::try_from(3).is_err());
    assert!(AttestationState::try_from(255).is_err());
}

#[test]
fn test_claim_state_from_u8() {
    assert_eq!(ClaimState::try_from(0).unwrap(), ClaimState::Pending);
    assert_eq!(ClaimState::try_from(1).unwrap(), ClaimState::Verified);
    assert_eq!(ClaimState::try_from(2).unwrap(), ClaimState::Consumed);
    assert_eq!(ClaimState::try_from(3).unwrap(), ClaimState::Rejected);
    assert!(ClaimState::try_from(4).is_err());
    assert!(ClaimState::try_from(255).is_err());
}

#[test]
fn test_predicate_from_u8() {
    assert_eq!(Predicate::try_from(0).unwrap(), Predicate::Matches);
    assert_eq!(Predicate::try_from(1).unwrap(), Predicate::GreaterOrEqual);
    assert_eq!(Predicate::try_from(2).unwrap(), Predicate::LessOrEqual);
    assert_eq!(Predicate::try_from(3).unwrap(), Predicate::Contains);
    assert_eq!(Predicate::try_from(4).unwrap(), Predicate::Custom);
    assert!(Predicate::try_from(5).is_err());
    assert!(Predicate::try_from(255).is_err());
}

#[test]
fn test_attestation_derive_id() {
    let attestor_pub_x = pallas::Base::from(1);
    let attestor_pub_y = pallas::Base::from(2);
    let claim_type = Predicate::Matches;
    let claim_data = vec![pallas::Base::from(1), pallas::Base::from(2)];
    let attestor_secret = pallas::Base::from(42);

    // derive_id is a placeholder - just verify it doesn't panic and returns consistent results
    let id = Attestation::derive_id(attestor_pub_x, attestor_pub_y, claim_type, &claim_data, attestor_secret);

    // Should be deterministic (same input = same output)
    let id2 = Attestation::derive_id(attestor_pub_x, attestor_pub_y, claim_type, &claim_data, attestor_secret);
    assert_eq!(id, id2);

    // Note: Since derive_id is a placeholder returning Base::zero(),
    // we only verify determinism here, not uniqueness
}

#[test]
fn test_attestation_encoding() {
    let attestation = Attestation {
        id: pallas::Base::from(1),
        attestor_pub_x: pallas::Base::from(2),
        attestor_pub_y: pallas::Base::from(3),
        attestor_secret: pallas::Base::from(4),
        claim_type: Predicate::Matches,
        claim_data: vec![pallas::Base::from(1), pallas::Base::from(2)],
        metadata: vec![1, 2, 3],
        state: AttestationState::Active,
        created_at: 50000,
        expires_at: Some(100000),
    };

    let encoded = serialize(&attestation);
    let decoded = deserialize::<Attestation>(&encoded).unwrap();

    assert_eq!(decoded.id, attestation.id);
    assert_eq!(decoded.claim_type, attestation.claim_type);
    assert_eq!(decoded.state, attestation.state);
    assert_eq!(decoded.created_at, attestation.created_at);
    assert_eq!(decoded.expires_at, attestation.expires_at);
}

#[test]
fn test_claim_encoding() {
    let claim = Claim {
        id: pallas::Base::from(1),
        attestation_id: pallas::Base::from(2),
        claimant_pub_x: pallas::Base::from(3),
        claimant_pub_y: pallas::Base::from(4),
        claimant_secret: pallas::Base::from(5),
        predicate: Predicate::GreaterOrEqual,
        evidence_commitment: vec![1, 2, 3],
        revealed_result: vec![4, 5, 6],
        proof: vec![7, 8, 9],
        state: ClaimState::Pending,
        created_at: 50000,
        consumed_at: None,
    };

    let encoded = serialize(&claim);
    let decoded = deserialize::<Claim>(&encoded).unwrap();

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
        attestation_id: pallas::Base::from(1),
        attestor_pub_x: pallas::Base::from(2),
        attestor_pub_y: pallas::Base::from(3),
        claim_type: Predicate::Matches,
        claim_data: vec![pallas::Base::from(4)],
        metadata: vec![5, 6],
        expires_at: Some(100000),
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<CreateAttestationParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_type, params.claim_type);
    assert_eq!(decoded.expires_at, params.expires_at);
}

#[test]
fn test_create_attestation_update_encoding() {
    let update = CreateAttestationUpdateV1 {
        attestation_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<CreateAttestationUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_revoke_attestation_params_encoding() {
    let params = RevokeAttestationParamsV1 {
        attestation_id: pallas::Base::from(1),
        attestor_pub_x: pallas::Base::from(2),
        attestor_pub_y: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<RevokeAttestationParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_revoke_attestation_update_encoding() {
    let update = RevokeAttestationUpdateV1 {
        attestation_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<RevokeAttestationUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_expire_attestation_params_encoding() {
    let params = ExpireAttestationParamsV1 {
        attestation_id: pallas::Base::from(1),
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<ExpireAttestationParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_expire_attestation_update_encoding() {
    let update = ExpireAttestationUpdateV1 {
        attestation_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<ExpireAttestationUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_create_claim_params_encoding() {
    let params = CreateClaimParamsV1 {
        proof: vec![1, 2, 3],
        claim_id: pallas::Base::from(1),
        attestation_id: pallas::Base::from(2),
        claimant_pub_x: pallas::Base::from(3),
        claimant_pub_y: pallas::Base::from(4),
        predicate: Predicate::LessOrEqual,
        evidence_commitment: vec![5, 6, 7],
        revealed_result: vec![8, 9],
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<CreateClaimParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.predicate, params.predicate);
}

#[test]
fn test_create_claim_update_encoding() {
    let update = CreateClaimUpdateV1 {
        claim_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<CreateClaimUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
}

#[test]
fn test_verify_claim_params_encoding() {
    let params = VerifyClaimParamsV1 {
        claim_id: pallas::Base::from(1),
        attestation_id: pallas::Base::from(2),
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<VerifyClaimParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_verify_claim_update_encoding() {
    let update = VerifyClaimUpdateV1 {
        claim_id: pallas::Base::from(1),
        verified: true,
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<VerifyClaimUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
    assert_eq!(decoded.verified, update.verified);
}

#[test]
fn test_consume_claim_params_encoding() {
    let params = ConsumeClaimParamsV1 {
        claim_id: pallas::Base::from(1),
        attestation_id: pallas::Base::from(2),
        claimant_pub_x: pallas::Base::from(3),
        claimant_pub_y: pallas::Base::from(4),
        nullifier: pallas::Base::from(5),
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<ConsumeClaimParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.nullifier, params.nullifier);
}

#[test]
fn test_consume_claim_update_encoding() {
    let update = ConsumeClaimUpdateV1 {
        claim_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<ConsumeClaimUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
}

#[test]
fn test_validate_claim_params_encoding() {
    let params = ValidateClaimParamsV1 {
        claim_id: pallas::Base::from(1),
        attestation_id: pallas::Base::from(2),
        evidence: vec![pallas::Base::from(3), pallas::Base::from(4)],
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<ValidateClaimParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.evidence.len(), params.evidence.len());
}

#[test]
fn test_validate_claim_update_encoding() {
    let update = ValidateClaimUpdateV1 {
        claim_id: pallas::Base::from(1),
        valid: true,
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<ValidateClaimUpdateV1>(&encoded).unwrap();

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