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

//! DAO-Escrow contract integration tests
//!
//! What these test, and why it is mostly round-trips: the class of defect this contract's re-wire has
//! found most often is **an encoder and its decoder disagreeing about the length they describe**
//! (`OBL-C150`, four instances to date — `CapabilityProof`, `EndowmentWithdrawParamsV1`,
//! `RegisterCapabilityRequirementParamsV1`, and `DaoEscrow`'s own record). Every such defect made a call
//! the client could build into a call the contract could not decode, and no type checker can see it: the
//! encoder's arithmetic and the decoder's arithmetic are two separate expressions over the same struct.
//! So each params and update struct is encoded from a populated value and decoded back, and the two are
//! compared **field by field** — a decode that silently drops a trailing field would still return `Ok`.

use dwow_dao_escrow_contract::{
    model::{
        CancelClaimParamsV1, CancelClaimUpdateV1, CapabilityProof, ClaimId, DaoEscrow,
        DaoEscrowBulla, DaoEscrowMode, EndowmentWithdrawParamsV1, EndowmentWithdrawUpdateV1,
        ExecuteClaimParamsV1, ExecuteClaimUpdateV1, InitializeParamsV1, InitializeUpdateV1,
        Membership, MembershipNote, PayPremiumParamsV1, PayPremiumUpdateV1, Proposal, ProposalId,
        ProposalState, ProposeClaimParamsV1, ProposeClaimUpdateV1, TreasurySpendParamsV1,
        TreasurySpendUpdateV1, UpdateParamsV1, UpdateUpdateV1, VoteClaimParamsV1,
        VoteClaimUpdateV1, VoteType, WithdrawParamsV1, WithdrawUpdateV1,
    },
    DaoEscrowFunction, DAO_ESCROW_CONTRACT_BULLAS_TREE, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE,
    DAO_ESCROW_CONTRACT_INFO_TREE, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE,
};
use dwow_sdk::{
    crypto::{pasta_prelude::Group, AssetId, BaseBlind, PublicKey, ScalarBlind, SecretKey},
    pasta::pallas,
};

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from_base(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// Helper to create BaseBlind from a numeric seed
fn make_blind(seed: u64) -> BaseBlind {
    BaseBlind::from_u64(seed)
}

/// A non-identity `CapabilityProof`, so a round-trip cannot pass by comparing zeroes.
fn make_capability_proof() -> CapabilityProof {
    CapabilityProof {
        capability_id: [7u8; 32],
        capability_secret: [9u8; 32],
        nullifier: dwow_sdk::crypto::IntentNullifier::from_base(pallas::Base::from(31u64)),
        issuer_pub: [11u8; 32],
        predicate_result: [13u8; 32],
        proof: vec![1u8, 2, 3, 4, 5],
    }
}

// ============================================================================
// THE FUNCTION SELECTOR SET
// ============================================================================

/// The ten endpoints that survive, each keeping the selector it has always had.
#[test]
fn test_surviving_selectors_resolve() {
    for code in [0x00u8, 0x01, 0x02, 0x03, 0x04, 0x05, 0x07, 0x08, 0x09, 0x0d] {
        assert!(
            DaoEscrowFunction::try_from(code).is_ok(),
            "selector 0x{code:02x} must resolve — it is one of the ten surviving endpoints",
        );
    }
}

/// The seven retired endpoints must be **refusals, not no-ops**.
///
/// `0x06`, `0x0a`–`0x0c`, `0x0e`–`0x10` all belonged to the OCap/Identity model that MultiSig groups
/// replaced; two of them used to return `Ok(())` while doing nothing, which is indistinguishable from
/// the action having happened. They are absent from the enum rather than mapped to an arm, so a caller
/// sending one reaches `InvalidFunction`. This test is the guard on that: re-adding a no-op arm to make
/// an old client stop erroring would turn this red, which is the point.
#[test]
fn test_retired_selectors_are_refused() {
    for code in [0x06u8, 0x0a, 0x0b, 0x0c, 0x0e, 0x0f, 0x10] {
        assert!(
            DaoEscrowFunction::try_from(code).is_err(),
            "selector 0x{code:02x} is retired and must not resolve — a no-op that reports success reads \
             as the action having happened",
        );
    }
    assert!(DaoEscrowFunction::try_from(0xFF).is_err());
    assert!(DaoEscrowFunction::try_from(0x11).is_err());
}

// ============================================================================
// THE ENDOWMENT RECORD
// ============================================================================

#[test]
fn test_dao_escrow_mode_encoding() {
    for mode in [
        DaoEscrowMode::Escrow,
        DaoEscrowMode::Treasury,
        DaoEscrowMode::TreasuryEndowment,
    ] {
        let encoded = mode.encode();
        assert_eq!(encoded.len(), 1);
        assert_eq!(DaoEscrowMode::decode(&encoded).unwrap(), mode);
    }
}

#[test]
fn test_dao_escrow_record_round_trip() {
    let record = DaoEscrow {
        mode: DaoEscrowMode::TreasuryEndowment,
        owner_pubkey: make_pubkey(1),
        multisig_group_id: pallas::Base::from(77u64),
        min_premium: 250,
    };

    let encoded = record.encode();
    assert_eq!(encoded.len(), 73, "the record has no variable-length part");
    assert_eq!(DaoEscrow::decode(&encoded).unwrap().encode(), encoded);
}

/// The record's length check is an equality. A minimum would let a longer buffer decode as a shorter
/// record and silently drop the tail — `OBL-C150`'s class — so one byte too many must be refused.
#[test]
fn test_dao_escrow_record_refuses_a_longer_buffer() {
    let encoded = DaoEscrow {
        mode: DaoEscrowMode::Escrow,
        owner_pubkey: make_pubkey(2),
        multisig_group_id: pallas::Base::zero(),
        min_premium: 0,
    }
    .encode();

    let mut longer = encoded.clone();
    longer.push(0u8);
    assert!(DaoEscrow::decode(&longer).is_err(), "73 + 1 bytes is not the record this contract writes");

    assert!(DaoEscrow::decode(&encoded[..72]).is_err(), "73 - 1 bytes is not either");
}

#[test]
fn test_dao_escrow_derive_bulla() {
    let dao_bulla = DaoEscrowBulla(pallas::Base::from(42u64));
    let owner_pubkey = make_pubkey(1);
    let pool_asset_id = AssetId::from_base(pallas::Base::one());
    let bulla_blind = make_blind(42);

    let bulla = DaoEscrow::derive_bulla(
        dao_bulla,
        &owner_pubkey,
        pool_asset_id,
        bulla_blind.clone(),
    );
    let again = DaoEscrow::derive_bulla(
        dao_bulla,
        &owner_pubkey,
        pool_asset_id,
        bulla_blind.clone(),
    );
    assert_eq!(bulla, again, "the derivation must be deterministic");

    // Each of the four inputs must matter. This is the guard on `OBL-C156`, where the derivation hashed
    // five elements with no domain constant while `init.zk` and the documentation both stated six with
    // one — so the value the circuit attested was never the value the endowment was stored under.
    let different_dao = DaoEscrowBulla(pallas::Base::from(43u64));
    assert_ne!(bulla, DaoEscrow::derive_bulla(different_dao, &owner_pubkey, pool_asset_id, bulla_blind.clone()));
    assert_ne!(bulla, DaoEscrow::derive_bulla(dao_bulla, &make_pubkey(2), pool_asset_id, bulla_blind.clone()));
    assert_ne!(bulla, DaoEscrow::derive_bulla(dao_bulla, &owner_pubkey, AssetId::from_base(pallas::Base::from(2u64)), bulla_blind.clone()));
    assert_ne!(bulla, DaoEscrow::derive_bulla(dao_bulla, &owner_pubkey, pool_asset_id, make_blind(43)));
}

// ============================================================================
// MEMBERSHIP
// ============================================================================

#[test]
fn test_membership_derive_note() {
    let dao_escrow_bulla = DaoEscrowBulla(pallas::Base::from(1));
    let member_pubkey = make_pubkey(1);
    let value: u64 = 1000;
    let asset_id = pallas::Base::one();
    let expiry: u64 = 100000;
    let blind = make_blind(42);

    let note = Membership::derive_note(dao_escrow_bulla, &member_pubkey, value, asset_id, expiry, blind.clone());
    let again = Membership::derive_note(dao_escrow_bulla, &member_pubkey, value, asset_id, expiry, blind.clone());
    assert_eq!(note, again);

    let different = Membership::derive_note(dao_escrow_bulla, &member_pubkey, value + 1, asset_id, expiry, blind.clone());
    assert_ne!(note, different);
}

#[test]
fn test_membership_encoding() {
    let membership = Membership {
        version: 0,
        note: MembershipNote(pallas::Base::from(1)),
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(2)),
        member_pubkey: make_pubkey(1),
        value: 1000,
        asset_id: AssetId::from_base(pallas::Base::one()),
        expiry: 100000,
        created_at: 50000,
    };

    let encoded = membership.encode();
    assert_eq!(encoded.len(), 153);
    let decoded = Membership::decode(&encoded).unwrap();
    assert_eq!(decoded.note, membership.note);
    assert_eq!(decoded.dao_escrow_bulla, membership.dao_escrow_bulla);
    assert_eq!(decoded.member_pubkey, membership.member_pubkey);
    assert_eq!(decoded.value, membership.value);
    assert_eq!(decoded.asset_id, membership.asset_id);
    assert_eq!(decoded.expiry, membership.expiry);
    assert_eq!(decoded.created_at, membership.created_at);
}

// ============================================================================
// PARAMETERS — one round-trip per surviving endpoint
// ============================================================================

#[test]
fn test_initialize_params_round_trip() {
    for mode in [
        DaoEscrowMode::Escrow,
        DaoEscrowMode::Treasury,
        DaoEscrowMode::TreasuryEndowment,
    ] {
        let params = InitializeParamsV1 {
            dao_bulla: DaoEscrowBulla(pallas::Base::from(1)),
            owner_pubkey: make_pubkey(1),
            endowment_asset_id: AssetId::from_base(pallas::Base::one()),
            bulla_blind: make_blind(42),
            mode,
            min_premium: 250,
        };

        let encoded = params.encode();
        assert_eq!(encoded.len(), 137, "32 + 32 + 32 + 32 + 1 + 8");
        let decoded = InitializeParamsV1::decode(&encoded).unwrap();

        assert_eq!(decoded.dao_bulla, params.dao_bulla);
        assert_eq!(decoded.owner_pubkey, params.owner_pubkey);
        assert_eq!(decoded.endowment_asset_id, params.endowment_asset_id);
        assert_eq!(decoded.bulla_blind, params.bulla_blind);
        // The mode is what makes the other two variants producible at all: it used to be written as a
        // constant, so `treasury_spend_v1`'s gate on it could not pass for any call (`OBL-C154`).
        assert_eq!(decoded.mode, mode);
        // And the premium floor is what makes it a reader rather than a field: it was set to zero for
        // every endowment and compared to nothing.
        assert_eq!(decoded.min_premium, 250);
    }
}

#[test]
fn test_initialize_update_round_trip() {
    let update = InitializeUpdateV1 {
        bulla: DaoEscrowBulla(pallas::Base::from(1)),
        owner_pubkey: make_pubkey(1),
        mode: DaoEscrowMode::Treasury,
        min_premium: 250,
    };

    let encoded = update.encode();
    assert_eq!(encoded.len(), 73);
    let decoded = InitializeUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.bulla, update.bulla);
    assert_eq!(decoded.owner_pubkey, update.owner_pubkey);
    assert_eq!(decoded.mode, update.mode);
    assert_eq!(decoded.min_premium, update.min_premium);
}

/// `UpdateV1` registers the group. The `multisig_group_id` is an `Option`, so both shapes must survive:
/// `None` writes nothing and `Some` is the one-shot installation.
#[test]
fn test_update_params_round_trip() {
    for group in [None, Some(pallas::Base::from(99u64))] {
        let params = UpdateParamsV1 {
            bulla: DaoEscrowBulla(pallas::Base::from(1)),
            multisig_group_id: group,
            owner_pubkey: make_pubkey(1),
            owner_nullifier: pallas::Base::from(1234u64),
        };

        let encoded = params.encode();
        assert_eq!(encoded.len(), if group.is_some() { 129 } else { 97 });
        let decoded = UpdateParamsV1::decode(&encoded).unwrap();
        assert_eq!(decoded.bulla, params.bulla);
        assert_eq!(decoded.multisig_group_id, group);
        assert_eq!(decoded.owner_pubkey, params.owner_pubkey);
        assert_eq!(decoded.owner_nullifier, params.owner_nullifier);
    }
}

#[test]
fn test_update_update_round_trip() {
    let update = UpdateUpdateV1 {
        bulla: DaoEscrowBulla(pallas::Base::from(1)),
        owner_nullifier: pallas::Base::from(1234u64),
        endowment_bytes: vec![7u8; 73],
    };

    let encoded = update.encode().unwrap();
    assert_eq!(encoded.len(), 68 + 73);
    let decoded = UpdateUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.bulla, update.bulla);
    assert_eq!(decoded.owner_nullifier, update.owner_nullifier);
    assert_eq!(decoded.endowment_bytes, update.endowment_bytes);
}

#[test]
fn test_pay_premium_params_round_trip() {
    let params = PayPremiumParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        membership_note: MembershipNote(pallas::Base::from(2)),
        value_commit: Group::identity(),
        value: 500,
        asset_id: AssetId::from_base(pallas::Base::one()),
        expiry: 100000,
        membership_blind: make_blind(42),
        value_blind: ScalarBlind::from_u64(43u64),
        member_pubkey: make_pubkey(1),
    };

    let encoded = params.encode();
    assert_eq!(encoded.len(), 240);
    let decoded = PayPremiumParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.membership_note, params.membership_note);
    assert_eq!(decoded.value, params.value);
    assert_eq!(decoded.asset_id, params.asset_id);
    assert_eq!(decoded.expiry, params.expiry);
    assert_eq!(decoded.member_pubkey, params.member_pubkey);
}

#[test]
fn test_pay_premium_update_round_trip() {
    let update = PayPremiumUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        membership_note: MembershipNote(pallas::Base::from(2)),
        amount: 10500,
        member_pubkey: make_pubkey(1),
        asset_id: AssetId::from_base(pallas::Base::one()),
        expiry: 100000,
        created_at: 4242,
        endowment_bytes: vec![7u8; 73],
    };

    let encoded = update.encode().unwrap();
    assert_eq!(encoded.len(), 156 + 73);
    let decoded = PayPremiumUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.membership_note, update.membership_note);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.member_pubkey, update.member_pubkey);
    assert_eq!(decoded.asset_id, update.asset_id);
    assert_eq!(decoded.expiry, update.expiry);
    assert_eq!(decoded.created_at, update.created_at);
    assert_eq!(decoded.endowment_bytes, update.endowment_bytes);
}

/// `WithdrawV1` carries the owner's amount, the payee and the proof's nullifier. The payee is required
/// to be the owner, so the published coordinates the circuit binds are the owner's.
#[test]
fn test_withdraw_params_round_trip() {
    let params = WithdrawParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        value: 500,
        recipient_pubkey: make_pubkey(1),
        owner_nullifier: pallas::Base::from(4242u64),
    };

    let encoded = params.encode();
    assert_eq!(encoded.len(), 104, "32 + 8 + 32 + 32");
    let decoded = WithdrawParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.value, params.value);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
    // The nullifier is the value the metadata arm publishes as the circuit's third instance, so a
    // codec that dropped it would make every withdrawal proof unsatisfiable.
    assert_eq!(decoded.owner_nullifier, params.owner_nullifier);

    let mut longer = encoded.clone();
    longer.push(0u8);
    assert!(WithdrawParamsV1::decode(&longer).is_err());
}

#[test]
fn test_withdraw_update_round_trip() {
    let update = WithdrawUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        value: 500,
        amount: 9500,
        endowment_bytes: vec![7u8; 73],
    };

    let encoded = update.encode().unwrap();
    assert_eq!(encoded.len(), 52 + 73);
    let decoded = WithdrawUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.value, update.value);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.endowment_bytes, update.endowment_bytes);
}

/// `EndowmentWithdrawV1`'s params must decode **exactly**. This is `OBL-C150`'s second instance: the
/// decoder demanded `>= 105` while the encoder wrote `105 + proof + 33`, so every governance-path call
/// was undecodable — and `capability_proof`, the field that caused it, is gone. `claim_id` followed it
/// (`OBL-C166`: it reached only the record it was copied into), leaving three fixed-size fields and a
/// 72-byte frame, so the equality is the right check rather than a minimum.
#[test]
fn test_endowment_withdraw_params_round_trip() {
    let params = EndowmentWithdrawParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        recipient_pubkey: make_pubkey(1),
        value: 500,
    };

    let encoded = params.encode();
    assert_eq!(encoded.len(), 72);
    let decoded = EndowmentWithdrawParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
    assert_eq!(decoded.value, params.value);

    let mut longer = encoded.clone();
    longer.push(0u8);
    assert!(EndowmentWithdrawParamsV1::decode(&longer).is_err());
}

#[test]
fn test_endowment_withdraw_update_round_trip() {
    let update = EndowmentWithdrawUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        value: 500,
        amount: 9500,
        endowment_bytes: vec![7u8; 73],
    };

    let encoded = update.encode().unwrap();
    assert_eq!(encoded.len(), 52 + 73);
    let decoded = EndowmentWithdrawUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.value, update.value);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.endowment_bytes, update.endowment_bytes);
}

#[test]
fn test_treasury_spend_params_round_trip() {
    let params = TreasurySpendParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        recipient_pubkey: make_pubkey(1),
        value: 500,
    };

    let encoded = params.encode();
    assert_eq!(encoded.len(), 72);
    let decoded = TreasurySpendParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
    assert_eq!(decoded.value, params.value);
}

#[test]
fn test_treasury_spend_update_round_trip() {
    let update = TreasurySpendUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        value: 500,
        amount: 9500,
        endowment_bytes: vec![7u8; 73],
    };

    let encoded = update.encode().unwrap();
    assert_eq!(encoded.len(), 52 + 73);
    let decoded = TreasurySpendUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.value, update.value);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.endowment_bytes, update.endowment_bytes);
}

#[test]
fn test_propose_claim_params_round_trip() {
    let params = ProposeClaimParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        claim_id: ClaimId(pallas::Base::from(2)),
        value: 500,
        recipient_pubkey: make_pubkey(1),
        claim_blind: pallas::Base::from(71u64),
    };

    let encoded = params.encode();
    assert_eq!(encoded.len(), 136);
    let decoded = ProposeClaimParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.claim_id, params.claim_id);
    assert_eq!(decoded.value, params.value);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
    // The blind is the field `OBL-C153` is about: the contract hashes it into `claim_commit`, so a
    // builder that dropped it would produce a call whose proof cannot verify. It must survive the codec.
    assert_eq!(decoded.claim_blind, params.claim_blind);
}

#[test]
fn test_propose_claim_update_round_trip() {
    let update = ProposeClaimUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        claim_id: ClaimId(pallas::Base::from(2)),
        value: 500,
        voting_ends_at: 1000,
        execution_deadline: 2000,
        recipient_pubkey: make_pubkey(1),
    };

    let encoded = update.encode();
    assert_eq!(encoded.len(), 120);
    let decoded = ProposeClaimUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.claim_id, update.claim_id);
    assert_eq!(decoded.value, update.value);
    assert_eq!(decoded.voting_ends_at, update.voting_ends_at);
    assert_eq!(decoded.execution_deadline, update.execution_deadline);
    assert_eq!(decoded.recipient_pubkey, update.recipient_pubkey);
}

/// `VoteClaimV1` still takes a direction and a voter, and its update now carries the **state the vote
/// decided** rather than a tally. The tally is gone because nothing read it: the group's own
/// `FinalizeV1` is the quorum, so an approved vote decides the claim outright (`OBL-C159`, `OBL-C160`).
#[test]
fn test_vote_claim_params_round_trip() {
    for vote in [VoteType::Yes, VoteType::No] {
        let params = VoteClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
            claim_id: ClaimId(pallas::Base::from(2)),
            vote,
            voter_pubkey: make_pubkey(1),
            capability_proof: make_capability_proof(),
        };

        let encoded = params.encode().unwrap();
        let decoded = VoteClaimParamsV1::decode(&encoded).unwrap();
        assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
        assert_eq!(decoded.claim_id, params.claim_id);
        assert_eq!(decoded.vote, vote);
        assert_eq!(decoded.voter_pubkey, params.voter_pubkey);
        assert_eq!(decoded.capability_proof.capability_id, params.capability_proof.capability_id);
        assert_eq!(decoded.capability_proof.capability_secret, params.capability_proof.capability_secret);
        assert_eq!(decoded.capability_proof.nullifier, params.capability_proof.nullifier);
        assert_eq!(decoded.capability_proof.issuer_pub, params.capability_proof.issuer_pub);
        assert_eq!(decoded.capability_proof.predicate_result, params.capability_proof.predicate_result);
        assert_eq!(decoded.capability_proof.proof, params.capability_proof.proof);
    }
}

#[test]
fn test_vote_claim_update_round_trip() {
    for state in [ProposalState::Approved, ProposalState::Rejected, ProposalState::Expired] {
        let update = VoteClaimUpdateV1 {
            dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
            claim_id: ClaimId(pallas::Base::from(2)),
            state,
            vote_nullifier: pallas::Base::from(88u64),
            proposal_bytes: vec![7u8; 89],
        };

        let encoded = update.encode().unwrap();
        assert_eq!(encoded.len(), 101 + 89);
        let decoded = VoteClaimUpdateV1::decode(&encoded).unwrap();
        assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
        assert_eq!(decoded.claim_id, update.claim_id);
        assert_eq!(decoded.state, state);
        assert_eq!(decoded.vote_nullifier, update.vote_nullifier);
        assert_eq!(decoded.proposal_bytes, update.proposal_bytes);
    }
}

#[test]
fn test_execute_claim_params_round_trip() {
    let params = ExecuteClaimParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        proposal_id: ProposalId(pallas::Base::from(2)),
        recipient_pubkey: make_pubkey(1),
        value: 500,
    };

    let encoded = params.encode();
    assert_eq!(encoded.len(), 104);
    let decoded = ExecuteClaimParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.proposal_id, params.proposal_id);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
    assert_eq!(decoded.value, params.value);
}

#[test]
fn test_execute_claim_update_round_trip() {
    let update = ExecuteClaimUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        proposal_id: ProposalId(pallas::Base::from(2)),
        value: 500,
        state: ProposalState::Executed,
        proposal_bytes: vec![7u8; 89],
    };

    let encoded = update.encode().unwrap();
    assert_eq!(encoded.len(), 77 + 89);
    let decoded = ExecuteClaimUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.proposal_id, update.proposal_id);
    assert_eq!(decoded.value, update.value);
    assert_eq!(decoded.state, ProposalState::Executed);
    assert_eq!(decoded.proposal_bytes, update.proposal_bytes);
}

/// Cancellation is authorised by the endowment's group. The caller's `proposer_pubkey` used to ride here
/// and be compared to the record's — two public values, so it admitted anyone (`OBL-C152`).
#[test]
fn test_cancel_claim_params_round_trip() {
    let params = CancelClaimParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        claim_id: ClaimId(pallas::Base::from(2)),
    };

    let encoded = params.encode();
    assert_eq!(encoded.len(), 64);
    let decoded = CancelClaimParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.claim_id, params.claim_id);
}

#[test]
fn test_cancel_claim_update_round_trip() {
    let update = CancelClaimUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        claim_id: ClaimId(pallas::Base::from(2)),
        state: ProposalState::Cancelled,
        proposal_bytes: vec![7u8; 89],
    };

    let encoded = update.encode().unwrap();
    assert_eq!(encoded.len(), 69 + 89);
    let decoded = CancelClaimUpdateV1::decode(&encoded).unwrap();
    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.claim_id, update.claim_id);
    assert_eq!(decoded.state, ProposalState::Cancelled);
    assert_eq!(decoded.proposal_bytes, update.proposal_bytes);
}

// ============================================================================
// THE PROPOSAL RECORD
// ============================================================================

/// Every state the lifecycle can be in must survive the codec, because `verify_proposal_approved`
/// refuses on the *value* it decodes. A codec that mapped two states to one byte would turn a rejected
/// claim into an approved one.
#[test]
fn test_proposal_round_trip_for_every_state() {
    for state in [
        ProposalState::Pending,
        ProposalState::Approved,
        ProposalState::Rejected,
        ProposalState::Executed,
        ProposalState::Cancelled,
        ProposalState::Expired,
    ] {
        let proposal = Proposal {
            dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
            value: 500,
            recipient_pubkey: make_pubkey(1),
            state,
            voting_ends_at: 1000,
            execution_deadline: 2000,
        };

        let encoded = proposal.encode();
        assert_eq!(encoded.len(), 89);
        let decoded = Proposal::decode(&encoded).unwrap();
        assert_eq!(decoded.dao_escrow_bulla, proposal.dao_escrow_bulla);
        assert_eq!(decoded.value, proposal.value);
        assert_eq!(decoded.recipient_pubkey, proposal.recipient_pubkey);
        assert_eq!(decoded.state, state);
        assert_eq!(decoded.voting_ends_at, proposal.voting_ends_at);
        assert_eq!(decoded.execution_deadline, proposal.execution_deadline);
    }
}

#[test]
fn test_proposal_refuses_a_wrong_length() {
    let proposal = Proposal {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        value: 500,
        recipient_pubkey: make_pubkey(1),
        state: ProposalState::Pending,
        voting_ends_at: 1000,
        execution_deadline: 2000,
    };
    let encoded = proposal.encode();
    assert!(Proposal::decode(&encoded[..88]).is_err());
    let mut longer = encoded;
    longer.push(0u8);
    assert!(Proposal::decode(&longer).is_err());
}

// ============================================================================
// THE NESTED PROOF FRAME
// ============================================================================

/// `CapabilityProof` is the one nested, variable-length struct left in this contract's params
/// (`VoteClaimParamsV1` carries it last). Its length check is an **equality**: a minimum would accept
/// whatever the caller appended to the frame, and this type is exactly where that minimum was once
/// needed — `OBL-C150`'s third instance — for a trailing field that no longer exists.
#[test]
fn test_capability_proof_refuses_trailing_bytes() {
    let proof = make_capability_proof();
    let encoded = proof.encode().unwrap();
    assert_eq!(encoded.len(), 164 + 5);

    let decoded = CapabilityProof::decode(&encoded).unwrap();
    assert_eq!(decoded.proof, proof.proof);
    assert_eq!(decoded.capability_secret, proof.capability_secret);

    let mut longer = encoded.clone();
    longer.push(0xff);
    assert!(
        CapabilityProof::decode(&longer).is_err(),
        "the frame is the proof and nothing after it — trailing bytes must be refused, not ignored",
    );

    assert!(CapabilityProof::decode(&encoded[..164]).is_err(), "a truncated frame must be refused");
}

// ============================================================================
// STATE NAMES
// ============================================================================

#[test]
fn test_constants() {
    assert_eq!(DAO_ESCROW_CONTRACT_INFO_TREE, "info");
    assert_eq!(DAO_ESCROW_CONTRACT_BULLAS_TREE, "bullas");
    assert_eq!(DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE, "membership");
    assert_eq!(DAO_ESCROW_CONTRACT_ENDOWMENT_TREE, "endowment");
}
