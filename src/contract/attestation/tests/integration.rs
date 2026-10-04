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

use dwow_serial::{deserialize, serialize};
use dwow_sdk::pasta::pallas;
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_attestation_contract::{
    model::{
        Attestation, AttestationId, AttestationState, Claim, ClaimId, ClaimState, CreateAttestationParamsV1,
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
    assert!(AttestationFunction::try_from(0x0d).is_ok()); // CheckAttestationV1
}

#[test]
fn test_attestation_function_enum_invalid() {
    assert!(AttestationFunction::try_from(0xFF).is_err());
    // The boundary moved from 0x0d to 0x0e on 2026-09-24, when `CheckAttestationV1` took 0x0d
    // (`OBL-Z16`, for `labor_market`'s `attestation_id`). This control pins the *first code past the
    // enum*, so it follows the enum rather than a particular number — and it is what a typecheck
    // cannot see: `cargo check` and every gate passed over this test while it was wrong.
    assert!(AttestationFunction::try_from(0x0e).is_err());
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
    let attestor_pub = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(1)));
    let claim_type = Predicate::Matches;
    let claim_data = vec![pallas::Base::from(1), pallas::Base::from(2)];
    let attestor_secret = pallas::Base::from(42);

    // derive_id returns a typed error rather than panicking on an identity key, so the call is
    // `expect`-ed here: `attestor_pub` comes from `from_secret`, which is never the identity.
    let id = Attestation::derive_id(attestor_pub, claim_type, &claim_data, attestor_secret)
        .expect("non-identity attestor");

    // Should be deterministic (same input = same output)
    let id2 = Attestation::derive_id(attestor_pub, claim_type, &claim_data, attestor_secret)
        .expect("non-identity attestor");
    assert_eq!(id, id2);

    // Note: Since derive_id is a placeholder returning Base::zero(),
    // we only verify determinism here, not uniqueness
}

#[test]
fn test_attestation_encoding() {
    let attestation = Attestation {

        version: 0,        id: AttestationId(pallas::Base::from(1)),
        attestor_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2))),
        attestor_secret: pallas::Base::from(4),
        claim_type: Predicate::Matches,
        claim_data: vec![pallas::Base::from(1), pallas::Base::from(2)],
        metadata: vec![1, 2, 3],
        state: AttestationState::Active,
        created_at: 50000,
        expires_at: Some(100000),
    };

    let encoded = attestation.encode().unwrap();
    let decoded = Attestation::decode(&encoded).unwrap();

    assert_eq!(decoded.id, attestation.id);
    assert_eq!(decoded.claim_type, attestation.claim_type);
    assert_eq!(decoded.state, attestation.state);
    assert_eq!(decoded.created_at, attestation.created_at);
    assert_eq!(decoded.expires_at, attestation.expires_at);
}

#[test]
fn test_claim_encoding() {
    let claim = Claim {

        version: 0,        id: ClaimId(pallas::Base::from(1)),
        attestation_id: AttestationId(pallas::Base::from(2)),
        claimant_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(3))),
        claimant_secret: pallas::Base::from(5),
        predicate: Predicate::GreaterOrEqual,
        evidence_commitment: vec![1, 2, 3],
        revealed_result: vec![4, 5, 6],
        proof: vec![7, 8, 9],
        state: ClaimState::Pending,
        created_at: 50000,
        consumed_at: None,
    };

    let encoded = claim.encode().unwrap();
    let decoded = Claim::decode(&encoded).unwrap();

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
        attestation_id: AttestationId(pallas::Base::from(1)),
        attestor_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2))),
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
        attestation_id: AttestationId(pallas::Base::from(1)),
        attestation: Attestation {
            version: 0,
            id: AttestationId(pallas::Base::from(1)),
            attestor_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2))),
            attestor_secret: pallas::Base::from(4),
            claim_type: Predicate::Matches,
            claim_data: vec![pallas::Base::from(1), pallas::Base::from(2)],
            metadata: vec![1, 2, 3],
            state: AttestationState::Active,
            created_at: 50000,
            expires_at: Some(100000),
        },
        index_key: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<CreateAttestationUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_revoke_attestation_params_encoding() {
    let params = RevokeAttestationParamsV1 {
        attestation_id: AttestationId(pallas::Base::from(1)),
        attestor_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2))),
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<RevokeAttestationParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_revoke_attestation_update_encoding() {
    let update = RevokeAttestationUpdateV1 {
        attestation_id: AttestationId(pallas::Base::from(1)),
        attestation: Attestation {
            version: 0,
            id: AttestationId(pallas::Base::from(1)),
            attestor_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2))),
            attestor_secret: pallas::Base::from(4),
            claim_type: Predicate::Matches,
            claim_data: vec![pallas::Base::from(1)],
            metadata: vec![1, 2, 3],
            state: AttestationState::Revoked,
            created_at: 50000,
            expires_at: None,
        },
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<RevokeAttestationUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_expire_attestation_params_encoding() {
    let params = ExpireAttestationParamsV1 {
        attestation_id: AttestationId(pallas::Base::from(1)),
        attestor_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2))),
    };

    let encoded = serialize(&params);
    let decoded = deserialize::<ExpireAttestationParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, params.attestation_id);
}

#[test]
fn test_expire_attestation_update_encoding() {
    let update = ExpireAttestationUpdateV1 {
        attestation_id: AttestationId(pallas::Base::from(1)),
        attestation: Attestation {
            version: 0,
            id: AttestationId(pallas::Base::from(1)),
            attestor_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(2))),
            attestor_secret: pallas::Base::from(4),
            claim_type: Predicate::Matches,
            claim_data: vec![pallas::Base::from(1)],
            metadata: vec![1, 2, 3],
            state: AttestationState::Expired,
            created_at: 50000,
            expires_at: None,
        },
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<ExpireAttestationUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.attestation_id, update.attestation_id);
}

#[test]
fn test_create_claim_params_encoding() {
    let params = CreateClaimParamsV1 {
        proof: vec![1, 2, 3],
        claim_id: ClaimId(pallas::Base::from(1)),
        attestation_id: AttestationId(pallas::Base::from(2)),
        claimant_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(3))),
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
        claim_id: ClaimId(pallas::Base::from(1)),
        claim: Claim {
            version: 0,
            id: ClaimId(pallas::Base::from(1)),
            attestation_id: AttestationId(pallas::Base::from(2)),
            claimant_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(3))),
            claimant_secret: pallas::Base::from(5),
            predicate: Predicate::GreaterOrEqual,
            evidence_commitment: vec![1, 2, 3],
            revealed_result: vec![4, 5, 6],
            proof: vec![7, 8, 9],
            state: ClaimState::Pending,
            created_at: 50000,
            consumed_at: None,
        },
        rate_limit_key: pallas::Base::from(1),
        current_block: 50000,
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<CreateClaimUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
}

#[test]
fn test_verify_claim_params_encoding() {
    // Issue #3: the struct carried `revealed_result` and `attestation_data` too. The first was
    // the verdict the host trusted and no circuit witnessed; the second was a caller-supplied
    // copy of a value the host reads from the attestation record. Both are removed, and the
    // payload is 96 bytes — three fields — where it was 160.
    let params = VerifyClaimParamsV1 {
        claim_id: ClaimId(pallas::Base::from(1)),
        attestation_id: AttestationId(pallas::Base::from(2)),
        evidence_commitment: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    assert_eq!(encoded.len(), VerifyClaimParamsV1::ENCODED_SIZE);
    assert_eq!(encoded.len(), 96);

    let decoded = deserialize::<VerifyClaimParamsV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.attestation_id, params.attestation_id);
    assert_eq!(decoded.evidence_commitment, params.evidence_commitment);
}

/// Issue #3: the control for the predicate, which is now one derivation with one home
/// (`model::predicate_holds`) called by both `verify_claim_v1` and `validate_claim_v1`. One
/// assertion per variant, and the ordinal arms are pinned on both sides of their boundary.
mod predicate_holds {
    use dwow_attestation_contract::model::{predicate_holds, Predicate};
    use dwow_sdk::pasta::pallas;

    fn b(n: u64) -> pallas::Base {
        pallas::Base::from(n)
    }

    #[test]
    fn matches_is_equality_including_length() {
        assert!(predicate_holds(Predicate::Matches, &[b(5)], &[b(5)]));
        assert!(!predicate_holds(Predicate::Matches, &[b(5)], &[b(6)]));
        // An attestation of two values is not matched by evidence naming one of them: the
        // register's `OBL-C154` shape, where a requirement's operand can be satisfied more
        // cheaply than the requirement means.
        assert!(!predicate_holds(Predicate::Matches, &[b(5)], &[b(5), b(5)]));
    }

    #[test]
    fn greater_or_equal_is_inclusive_and_ordered() {
        assert!(predicate_holds(Predicate::GreaterOrEqual, &[b(51)], &[b(50)]));
        assert!(predicate_holds(Predicate::GreaterOrEqual, &[b(50)], &[b(50)]));
        assert!(!predicate_holds(Predicate::GreaterOrEqual, &[b(49)], &[b(50)]));
        // Empty evidence or empty claim data is not a satisfied predicate. The stub this
        // replaced answered `false` here too, but by an `if len() >= 1` guard rather than by
        // the definition.
        assert!(!predicate_holds(Predicate::GreaterOrEqual, &[], &[b(50)]));
        assert!(!predicate_holds(Predicate::GreaterOrEqual, &[b(50)], &[]));
    }

    #[test]
    fn less_or_equal_is_inclusive_and_ordered() {
        assert!(predicate_holds(Predicate::LessOrEqual, &[b(49)], &[b(50)]));
        assert!(predicate_holds(Predicate::LessOrEqual, &[b(50)], &[b(50)]));
        assert!(!predicate_holds(Predicate::LessOrEqual, &[b(51)], &[b(50)]));
    }

    #[test]
    fn contains_is_a_contiguous_run() {
        // The stub this replaced compared first elements under the comment "Simplified: just
        // check first element", which answers `true` for `[1,2]` in `[1,9]`.
        assert!(!predicate_holds(Predicate::Contains, &[b(1), b(2)], &[b(1), b(9)]));
        assert!(predicate_holds(Predicate::Contains, &[b(1), b(2)], &[b(0), b(1), b(2), b(3)]));
        assert!(!predicate_holds(Predicate::Contains, &[b(2), b(1)], &[b(1), b(2)]));
        // A pattern longer than the data is not contained in it, and an empty pattern is
        // contained in everything — written out rather than left to `windows(0)`, which
        // panics.
        assert!(!predicate_holds(Predicate::Contains, &[b(1), b(2)], &[b(1)]));
        assert!(predicate_holds(Predicate::Contains, &[], &[b(1)]));
    }

    #[test]
    fn custom_is_not_decided_by_this_contract() {
        // There is no host rule for a custom predicate and no external verifier in this tree
        // to consult, so the answer is `false` rather than a guess: a claim the contract
        // cannot evaluate must not be recorded as verified.
        assert!(!predicate_holds(Predicate::Custom, &[b(1)], &[b(1)]));
    }
}

#[test]
fn test_verify_claim_update_encoding() {
    let update = VerifyClaimUpdateV1 {
        claim_id: ClaimId(pallas::Base::from(1)),
        claim: Claim {
            version: 0,
            id: ClaimId(pallas::Base::from(1)),
            attestation_id: AttestationId(pallas::Base::from(2)),
            claimant_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(3))),
            claimant_secret: pallas::Base::from(5),
            predicate: Predicate::GreaterOrEqual,
            evidence_commitment: vec![1, 2, 3],
            revealed_result: vec![4, 5, 6],
            proof: vec![7, 8, 9],
            state: ClaimState::Verified,
            created_at: 50000,
            consumed_at: None,
        },
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<VerifyClaimUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
}

#[test]
fn test_consume_claim_params_encoding() {
    let params = ConsumeClaimParamsV1 {
        claim_id: ClaimId(pallas::Base::from(1)),
        attestation_id: AttestationId(pallas::Base::from(2)),
        claimant_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(3))),
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
        claim_id: ClaimId(pallas::Base::from(1)),
        claim: Claim {
            version: 0,
            id: ClaimId(pallas::Base::from(1)),
            attestation_id: AttestationId(pallas::Base::from(2)),
            claimant_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(3))),
            claimant_secret: pallas::Base::from(5),
            predicate: Predicate::GreaterOrEqual,
            evidence_commitment: vec![1, 2, 3],
            revealed_result: vec![4, 5, 6],
            proof: vec![7, 8, 9],
            state: ClaimState::Consumed,
            created_at: 50000,
            consumed_at: None,
        },
        nullifier: pallas::Base::from(5),
    };

    let encoded = serialize(&update);
    let decoded = deserialize::<ConsumeClaimUpdateV1>(&encoded).unwrap();

    assert_eq!(decoded.claim_id, update.claim_id);
}

#[test]
fn test_validate_claim_params_encoding() {
    let params = ValidateClaimParamsV1 {
        claim_id: ClaimId(pallas::Base::from(1)),
        attestation_id: AttestationId(pallas::Base::from(2)),
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
        claim_id: ClaimId(pallas::Base::from(1)),
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