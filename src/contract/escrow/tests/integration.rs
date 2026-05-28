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

//! Escrow contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{MerkleNode, pasta_prelude::Group, schnorr::Signature},
    pasta::pallas,
};
use dwow_escrow_contract::{
    model::{
        CancelEscrowParamsV1, CancelEscrowUpdateV1, ClaimEscrowParamsV1, ClaimEscrowUpdateV1,
        CreateEscrowParamsV1, CreateEscrowUpdateV1, Escrow, EscrowId, EscrowState,
        FundEscrowParamsV1, FundEscrowUpdateV1, RefundEscrowParamsV1, RefundEscrowUpdateV1,
    },
    EscrowFunction,
    // Constants
    ESCROW_CONTRACT_INFO_TREE, ESCROW_CONTRACT_ESCROWS_TREE,
    ESCROW_CONTRACT_NULLIFIERS_TREE, ESCROW_CONTRACT_SPENT_FLAGS_TREE,
};

/// Helper to create a test PublicKey
fn make_pubkey(seed: u64) -> dwow_sdk::crypto::PublicKey {
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// Helper to create a test MerkleNode
fn make_merkle_node(seed: u64) -> MerkleNode {
    MerkleNode::from(pallas::Base::from(seed))
}

#[test]
fn test_escrow_function_enum_valid() {
    assert!(EscrowFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(EscrowFunction::try_from(0x01).is_ok()); // CreateEscrowV1
    assert!(EscrowFunction::try_from(0x02).is_ok()); // FundV1
    assert!(EscrowFunction::try_from(0x03).is_ok()); // ClaimV1
    assert!(EscrowFunction::try_from(0x04).is_ok()); // RefundV1
    assert!(EscrowFunction::try_from(0x05).is_ok()); // CancelV1
}

#[test]
fn test_escrow_function_enum_invalid() {
    assert!(EscrowFunction::try_from(0xFF).is_err());
    assert!(EscrowFunction::try_from(0x06).is_err());
    assert!(EscrowFunction::try_from(0x10).is_err());
}

#[test]
fn test_escrow_state_values() {
    assert_eq!(EscrowState::Created as u8, 0);
    assert_eq!(EscrowState::Funded as u8, 1);
    assert_eq!(EscrowState::Claimed as u8, 2);
    assert_eq!(EscrowState::Refunded as u8, 3);
    assert_eq!(EscrowState::Cancelled as u8, 4);
}

#[test]
fn test_escrow_state_try_from() {
    assert_eq!(EscrowState::try_from(0).ok(), Some(EscrowState::Created));
    assert_eq!(EscrowState::try_from(1).ok(), Some(EscrowState::Funded));
    assert_eq!(EscrowState::try_from(2).ok(), Some(EscrowState::Claimed));
    assert_eq!(EscrowState::try_from(3).ok(), Some(EscrowState::Refunded));
    assert_eq!(EscrowState::try_from(4).ok(), Some(EscrowState::Cancelled));
    assert!(EscrowState::try_from(5).is_err());
    assert!(EscrowState::try_from(255).is_err());
}

#[test]
fn test_constants() {
    assert_eq!(ESCROW_CONTRACT_INFO_TREE, "info");
    assert_eq!(ESCROW_CONTRACT_ESCROWS_TREE, "escrows");
    assert_eq!(ESCROW_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(ESCROW_CONTRACT_SPENT_FLAGS_TREE, "spent_flags");
}

#[test]
fn test_create_escrow_params_encoding() {
    let params = CreateEscrowParamsV1 {
        buyer_pubkey: make_pubkey(1),
        seller_pubkey: make_pubkey(2),
        value: 1000,
        token_id: pallas::Base::from(1),
        timeout: 100,
        commitment: pallas::Base::from(42),
        merkle_root: make_merkle_node(99),
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: CreateEscrowParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.buyer_pubkey, params.buyer_pubkey);
    assert_eq!(decoded.seller_pubkey, params.seller_pubkey);
    assert_eq!(decoded.value, params.value);
    assert_eq!(decoded.token_id, params.token_id);
    assert_eq!(decoded.timeout, params.timeout);
}

#[test]
fn test_create_escrow_update_encoding() {
    let update = CreateEscrowUpdateV1 {
        escrow_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded: CreateEscrowUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, update.escrow_id);
}

#[test]
fn test_fund_escrow_params_encoding() {
    let params = FundEscrowParamsV1 {
        escrow_id: pallas::Base::from(1),
        value_commit: pallas::Point::identity(),
        merkle_proof: vec![pallas::Base::from(1), pallas::Base::from(2)],
        merkle_root: make_merkle_node(99),
    };

    let encoded = serialize(&params);
    let decoded: FundEscrowParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, params.escrow_id);
    assert_eq!(decoded.merkle_proof.len(), 2);
}

#[test]
fn test_fund_escrow_update_encoding() {
    let update = FundEscrowUpdateV1 {
        escrow_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded: FundEscrowUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, update.escrow_id);
}

#[test]
fn test_claim_escrow_params_encoding() {
    let params = ClaimEscrowParamsV1 {
        escrow_id: pallas::Base::from(1),
        seller_secret: pallas::Base::from(42),
        spent_nullifier: pallas::Base::from(50),
        recipient_pubkey: make_pubkey(3),
    };

    let encoded = serialize(&params);
    let decoded: ClaimEscrowParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, params.escrow_id);
    assert_eq!(decoded.seller_secret, params.seller_secret);
    assert_eq!(decoded.spent_nullifier, params.spent_nullifier);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
}

#[test]
fn test_claim_escrow_update_encoding() {
    let update = ClaimEscrowUpdateV1 {
        escrow_id: pallas::Base::from(1),
        spent_nullifier: pallas::Base::from(50),
    };

    let encoded = serialize(&update);
    let decoded: ClaimEscrowUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, update.escrow_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
}

#[test]
fn test_refund_escrow_params_encoding() {
    let params = RefundEscrowParamsV1 {
        escrow_id: pallas::Base::from(1),
        buyer_secret: pallas::Base::from(42),
        spent_nullifier: pallas::Base::from(50),
        current_block: 150,
        timeout: 100,
        recipient_pubkey: make_pubkey(3),
    };

    let encoded = serialize(&params);
    let decoded: RefundEscrowParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, params.escrow_id);
    assert_eq!(decoded.buyer_secret, params.buyer_secret);
    assert_eq!(decoded.current_block, params.current_block);
    assert_eq!(decoded.recipient_pubkey, params.recipient_pubkey);
}

#[test]
fn test_refund_escrow_update_encoding() {
    let update = RefundEscrowUpdateV1 {
        escrow_id: pallas::Base::from(1),
        spent_nullifier: pallas::Base::from(50),
    };

    let encoded = serialize(&update);
    let decoded: RefundEscrowUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, update.escrow_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
}

#[test]
fn test_cancel_escrow_params_encoding() {
    let params = CancelEscrowParamsV1 {
        escrow_id: pallas::Base::from(1),
        buyer_pubkey: make_pubkey(1),
        buyer_secret: pallas::Base::from(42),
        signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: CancelEscrowParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, params.escrow_id);
    assert_eq!(decoded.buyer_secret, params.buyer_secret);
}

#[test]
fn test_cancel_escrow_update_encoding() {
    let update = CancelEscrowUpdateV1 {
        escrow_id: pallas::Base::from(1),
    };

    let encoded = serialize(&update);
    let decoded: CancelEscrowUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.escrow_id, update.escrow_id);
}

#[test]
fn test_escrow_encoding() {
    let escrow = Escrow {
        id: pallas::Base::from(1),
        buyer_pubkey: make_pubkey(2),
        seller_pubkey: make_pubkey(3),
        value: 1000,
        token_id: pallas::Base::from(1),
        timeout: 100,
        state: EscrowState::Funded,
        value_commit: pallas::Point::identity(),
        value_blind: pallas::Scalar::from(99),
        spent_nullifier: pallas::Base::from(50),
        created_at: 50,
        funded_at: Some(55),
        instance_seed: [1u8; 32],
    };

    let encoded = serialize(&escrow);
    let decoded: Escrow = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, escrow.id);
    assert_eq!(decoded.buyer_pubkey, escrow.buyer_pubkey);
    assert_eq!(decoded.seller_pubkey, escrow.seller_pubkey);
    assert_eq!(decoded.value, escrow.value);
    assert_eq!(decoded.token_id, escrow.token_id);
    assert_eq!(decoded.timeout, escrow.timeout);
    assert_eq!(decoded.state, escrow.state);
    assert_eq!(decoded.created_at, escrow.created_at);
    assert_eq!(decoded.funded_at, escrow.funded_at);
}

#[test]
fn test_escrow_derive_id() {
    let buyer_pubkey = make_pubkey(1);
    let seller_pubkey = make_pubkey(2);
    let value = 1000u64;
    let token_id = pallas::Base::from(1);
    let timeout = 100u64;
    let buyer_secret = pallas::Base::from(42);
    let seller_secret = pallas::Base::from(99);

    let escrow_id: EscrowId =
        Escrow::derive_id(&buyer_pubkey, &seller_pubkey, value, token_id, timeout, buyer_secret, seller_secret);

    // Escrow ID should be non-zero
    assert!(escrow_id != pallas::Base::zero());
}

#[test]
fn test_escrow_compute_nullifier() {
    let escrow = Escrow {
        id: pallas::Base::from(1),
        buyer_pubkey: make_pubkey(2),
        seller_pubkey: make_pubkey(3),
        value: 1000,
        token_id: pallas::Base::from(1),
        timeout: 100,
        state: EscrowState::Funded,
        value_commit: pallas::Point::identity(),
        value_blind: pallas::Scalar::from(99),
        spent_nullifier: pallas::Base::from(50),
        created_at: 50,
        funded_at: Some(55),
        instance_seed: [0u8; 32],
    };

    let secret = pallas::Base::from(42);
    let nullifier = escrow.compute_nullifier(secret);

    // Nullifier should be non-zero
    assert!(nullifier != pallas::Base::zero());
}