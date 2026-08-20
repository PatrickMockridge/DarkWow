//! Integration tests for the Purse contract — data model encode/decode round-trips.
//! Updated for L1 type system (Part C §C.3.2): Amount, Balance, SDK Nullifier.

use dwow_purse_contract::model::{Amount, Balance, BalanceParams, DepositParams, MerklePosition, Purse, PurseId, StateNonce, WithdrawParams};
use dwow_sdk::crypto::{pasta_prelude::PrimeField, Keypair, MerkleNode, Nullifier, PublicKey, SecretKey};
use dwow_sdk::pasta::{group::GroupEncoding, pallas};

fn dummy_pubkey() -> PublicKey {
    Keypair::new(SecretKey::from_base(pallas::Base::from(42))).public
}

fn dummy_point() -> pallas::Point {
    let sk = SecretKey::from_base(pallas::Base::from(7));
    let pk = Keypair::new(sk).public;
    pallas::Point::from_bytes(&pk.to_bytes()).into_option()
        .expect("valid point from keypair")
}

fn dummy_merkle_node() -> MerkleNode {
    MerkleNode::from_base(pallas::Base::from(1u64))
}

fn dummy_nullifier() -> Nullifier {
    // Use a valid canonical field element — not [42u8; 32] which is non-canonical
    Nullifier::from_bytes(pallas::Base::from(99u64).to_repr())
        .expect("valid nullifier")
}

fn dummy_merkle_path() -> [MerkleNode; 32] {
    [MerkleNode::from_base(pallas::Base::from(1u64)); 32]
}

#[test]
fn test_amount_rejects_zero() {
    assert!(Amount::new(0).is_err(), "Amount must reject zero");
}

#[test]
fn test_amount_accepts_positive() {
    let a = Amount::new(1000).expect("positive amount");
    assert_eq!(a.inner(), 1000);
}

#[test]
fn test_amount_roundtrip() {
    let a = Amount::new(500).expect("positive amount");
    let bytes = a.to_le_bytes();
    let b = Amount::from_le_bytes(bytes).expect("round-trip");
    assert_eq!(a.inner(), b.inner());
}

#[test]
fn test_balance_accepts_zero() {
    let b = Balance::new(0);
    assert_eq!(b.inner(), 0);
}

#[test]
fn test_balance_roundtrip() {
    let b = Balance::new(100);
    let bytes = b.to_le_bytes();
    let c = Balance::from_le_bytes(bytes);
    assert_eq!(b.inner(), c.inner());
}

#[test]
fn test_purse_encode_decode_roundtrip() {
    let purse = Purse {
        version: 0,
        purse_id: PurseId(pallas::Base::from(99u64)),
        token_commit: pallas::Base::from(1u64),
        balance_commit: dummy_point(),
        owner_commit: pallas::Base::from(2u64),
    };

    let encoded = purse.encode().expect("encode must succeed");
    assert_eq!(encoded.len(), 129, "Purse must encode to exactly 129 bytes");

    let decoded = Purse::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.version, purse.version);
    assert_eq!(decoded.purse_id.inner(), purse.purse_id.inner());
    assert_eq!(decoded.token_commit, purse.token_commit);

    let re_encoded = purse.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_deposit_params_encode_decode_roundtrip() {
    let params = DepositParams {
        purse_id: PurseId(pallas::Base::from(99u64)),
        old_balance: Balance::new(0),
        deposit_amount: Amount::new(1000).expect("positive amount"),
        new_balance: Balance::new(1000),
        state_nonce: StateNonce::new(pallas::Base::from(1u64)),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        old_commit_x: pallas::Base::from(3u64),
        old_commit_y: pallas::Base::from(4u64),
        new_commit_x: pallas::Base::from(5u64),
        new_commit_y: pallas::Base::from(6u64),
        leaf_pos: MerklePosition::new(0),
        merkle_path: dummy_merkle_path(),
        proof: vec![1u8, 2, 3],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
        asset_id: pallas::Base::from(1u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = DepositParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.purse_id.inner(), params.purse_id.inner());
    assert_eq!(decoded.deposit_amount.inner(), params.deposit_amount.inner());
    assert_eq!(decoded.new_balance.inner(), params.new_balance.inner());
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_withdraw_params_encode_decode_roundtrip() {
    let params = WithdrawParams {
        purse_id: PurseId(pallas::Base::from(99u64)),
        old_balance: Balance::new(1000),
        withdraw_amount: Amount::new(500).expect("positive amount"),
        new_balance: Balance::new(500),
        state_nonce: StateNonce::new(pallas::Base::from(1u64)),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        old_commit_x: pallas::Base::from(3u64),
        old_commit_y: pallas::Base::from(4u64),
        new_commit_x: pallas::Base::from(5u64),
        new_commit_y: pallas::Base::from(6u64),
        leaf_pos: MerklePosition::new(0),
        merkle_path: dummy_merkle_path(),
        proof: vec![4u8, 5, 6],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
        asset_id: pallas::Base::from(1u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = WithdrawParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.purse_id.inner(), params.purse_id.inner());
    assert_eq!(decoded.withdraw_amount.inner(), params.withdraw_amount.inner());
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_balance_params_encode_decode_roundtrip() {
    let params = BalanceParams {
        purse_id: PurseId(pallas::Base::from(99u64)),
        asset_id: pallas::Base::from(1u64),
        balance: Balance::new(100),
        state_nonce: StateNonce::new(pallas::Base::from(1u64)),
        derived_purse_id: pallas::Base::from(2u64),
        expected_root: dummy_merkle_node(),
        token_commit: pallas::Base::from(3u64),
        balance_commit_x: pallas::Base::from(4u64),
        balance_commit_y: pallas::Base::from(5u64),
        leaf_pos: MerklePosition::new(0),
        merkle_path: dummy_merkle_path(),
        proof: vec![7u8, 8, 9],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = BalanceParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.purse_id.inner(), params.purse_id.inner());
    assert_eq!(decoded.balance.inner(), params.balance.inner());
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_decode_rejects_empty() {
    assert!(Purse::decode(&[]).is_err());
    assert!(DepositParams::decode(&[]).is_err());
    assert!(WithdrawParams::decode(&[]).is_err());
    assert!(BalanceParams::decode(&[]).is_err());
}
