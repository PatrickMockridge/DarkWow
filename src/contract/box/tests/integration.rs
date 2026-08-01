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

//! Integration tests for the Box contract — data model encode/decode round-trips.
//! Updated for L1 type system: BoxId, MerklePosition, StateNonce, PutParams, TakeParams.
//!
//! Box is the ZK-native o-cap delegation primitive. Put creates a capability,
//! Take consumes via nullifier. Four-component architecture:
//!   info tree | nullifiers tree | box Merkle tree | box roots tree

use dwow_box_contract::model::{
    BoxId, MerklePosition, PutParams, PutUpdate, StateNonce, TakeParams, TakeUpdate,
};
use dwow_sdk::crypto::{pasta_prelude::PrimeField, MerkleNode, Nullifier};
use dwow_sdk::pasta::pallas;

fn dummy_merkle_node() -> MerkleNode {
    MerkleNode::from_base(pallas::Base::from(1u64))
}

fn dummy_nullifier() -> Nullifier {
    Nullifier::from_bytes(pallas::Base::from(99u64).to_repr())
        .expect("valid nullifier")
}

fn dummy_merkle_path() -> [MerkleNode; 32] {
    [MerkleNode::from_base(pallas::Base::from(1u64)); 32]
}

// ── BoxId ────────────────────────────────────────────────────────────────────

#[test]
fn test_box_id_decode_rejects_wrong_length() {
    assert!(BoxId::decode(&[]).is_err(), "BoxId must reject empty input");
    assert!(
        BoxId::decode(&[0u8; 31]).is_err(),
        "BoxId must reject 31-byte input (needs 32)"
    );
}

#[test]
fn test_box_id_encode_decode_roundtrip() {
    let id = BoxId(pallas::Base::from(42u64));
    let encoded = id.encode();
    assert_eq!(encoded.len(), 32, "BoxId must encode to exactly 32 bytes");
    let decoded = BoxId::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.inner(), id.inner());
}

#[test]
fn test_box_id_deterministic() {
    let id = BoxId(pallas::Base::from(42u64));
    let a = id.encode();
    let b = id.encode();
    assert_eq!(a, b, "BoxId encode must be deterministic");
}

// ── MerklePosition ───────────────────────────────────────────────────────────

#[test]
fn test_merkle_position_roundtrip() {
    let pos = MerklePosition::new(42);
    let bytes = pos.to_le_bytes();
    assert_eq!(bytes.len(), 4);
    let back = MerklePosition::from_le_bytes(bytes);
    assert_eq!(back.inner(), 42);
}

// ── StateNonce ───────────────────────────────────────────────────────────────

#[test]
fn test_state_nonce_roundtrip() {
    let sn = StateNonce::new(pallas::Base::from(7u64));
    let bytes = sn.to_repr();
    assert_eq!(bytes.len(), 32);
    let back = StateNonce::from_repr(bytes).expect("round-trip");
    assert_eq!(back.inner(), sn.inner());
}

// ── PutParams ────────────────────────────────────────────────────────────────

#[test]
fn test_put_params_encode_decode_roundtrip() {
    let params = PutParams {
        box_id: BoxId(pallas::Base::from(99u64)),
        old_state_nonce: StateNonce::new(pallas::Base::from(1u64)),
        new_state_nonce: StateNonce::new(pallas::Base::from(2u64)),
        old_contents_commit: pallas::Base::from(3u64),
        new_contents_commit: pallas::Base::from(4u64),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        leaf_pos: MerklePosition::new(0),
        merkle_path: dummy_merkle_path(),
        proof: vec![1u8, 2, 3],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = PutParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.box_id.inner(), params.box_id.inner());
    assert_eq!(decoded.old_state_nonce.inner(), params.old_state_nonce.inner());
    assert_eq!(
        decoded.old_contents_commit, params.old_contents_commit,
        "old_contents_commit must survive round-trip"
    );
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_put_params_rejects_truncated() {
    // PutParams header is 260 bytes + 1024 bytes merkle_path + 1 byte proof_len
    // min: 260 + 1024 + 1 + 0 + 64 = 1349 bytes (min proof is 0 bytes)
    let short = vec![0u8; 500]; // well below minimum
    assert!(
        PutParams::decode(&short).is_err(),
        "PutParams must reject truncated input"
    );
}

// ── PutUpdate ─────────────────────────────────────────────────────────────────

#[test]
fn test_put_update_encode_decode_roundtrip() {
    let update = PutUpdate {
        nullifier: dummy_nullifier(),
        new_leaf: dummy_merkle_node(),
    };

    let encoded = update.encode().expect("encode must succeed");
    assert_eq!(encoded.len(), 64, "PutUpdate must encode to exactly 64 bytes");

    let decoded = PutUpdate::decode(&encoded).expect("round-trip must succeed");
    // Nullifier round-trip: re-derive bytes from decoded
    assert_eq!(decoded.nullifier.to_bytes(), update.nullifier.to_bytes());
    assert_eq!(decoded.new_leaf.to_bytes(), update.new_leaf.to_bytes());
}

#[test]
fn test_put_update_rejects_wrong_length() {
    assert!(PutUpdate::decode(&[]).is_err());
    assert!(PutUpdate::decode(&[0u8; 63]).is_err());
    assert!(PutUpdate::decode(&[0u8; 65]).is_err());
}

// ── TakeParams ───────────────────────────────────────────────────────────────

#[test]
fn test_take_params_encode_decode_roundtrip() {
    let params = TakeParams {
        box_id: BoxId(pallas::Base::from(99u64)),
        contents_commit: pallas::Base::from(3u64),
        state_nonce: StateNonce::new(pallas::Base::from(1u64)),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        leaf_pos: MerklePosition::new(0),
        merkle_path: dummy_merkle_path(),
        proof: vec![4u8, 5, 6],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = TakeParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.box_id.inner(), params.box_id.inner());
    assert_eq!(decoded.contents_commit, params.contents_commit);
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_take_params_rejects_truncated() {
    // TakeParams header is 164 bytes + 1024 merkle_path + 1 proof_len
    let short = vec![0u8; 500];
    assert!(
        TakeParams::decode(&short).is_err(),
        "TakeParams must reject truncated input"
    );
}

// ── TakeUpdate ────────────────────────────────────────────────────────────────

#[test]
fn test_take_update_encode_decode_roundtrip() {
    let update = TakeUpdate {
        nullifier: dummy_nullifier(),
    };

    let encoded = update.encode().expect("encode must succeed");
    assert_eq!(encoded.len(), 32, "TakeUpdate must encode to exactly 32 bytes");

    let decoded = TakeUpdate::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.nullifier.to_bytes(), update.nullifier.to_bytes());
}

#[test]
fn test_take_update_rejects_wrong_length() {
    assert!(TakeUpdate::decode(&[]).is_err());
    assert!(TakeUpdate::decode(&[0u8; 31]).is_err());
    assert!(TakeUpdate::decode(&[0u8; 33]).is_err());
}

// ── Decode Rejects Empty ─────────────────────────────────────────────────────

#[test]
fn test_decode_rejects_empty() {
    assert!(PutParams::decode(&[]).is_err());
    assert!(TakeParams::decode(&[]).is_err());
    assert!(PutUpdate::decode(&[]).is_err());
    assert!(TakeUpdate::decode(&[]).is_err());
}
