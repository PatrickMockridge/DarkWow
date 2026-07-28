//! Integration tests for the Purse contract — data model encode/decode round-trips.

use dwow_purse_contract::model::{Purse, PurseId, DepositParamsV1, WithdrawParamsV1};
use dwow_sdk::crypto::{Keypair, Nullifier, PublicKey, SecretKey};
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

#[test]
fn test_purse_encode_decode_roundtrip() {
    let purse = Purse {
        version: 0,
        purse_id: PurseId(pallas::Base::from(99u64)),
        token_commit: pallas::Base::from(1u64),
        balance_commit: dummy_point(),
        owner_commit: pallas::Base::from(2u64),
    };

    let encoded = purse.encode();
    assert_eq!(encoded.len(), 129, "Purse must encode to exactly 129 bytes");

    let decoded = Purse::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.version, purse.version);
    assert_eq!(decoded.purse_id.inner(), purse.purse_id.inner());
    assert_eq!(decoded.token_commit, purse.token_commit);

    assert_eq!(purse.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_deposit_params_encode_decode_roundtrip() {
    let params = DepositParamsV1 {
        purse_id: PurseId(pallas::Base::from(99u64)),
        deposit_amount: 1000u64,
        old_balance_commit: dummy_point(),
        new_balance_commit: dummy_point(),
        owner: dummy_pubkey(),
        proof: vec![1u8, 2, 3],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };

    let encoded = params.encode();
    assert!(!encoded.is_empty());

    let decoded = DepositParamsV1::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.purse_id.inner(), params.purse_id.inner());
    assert_eq!(decoded.deposit_amount, params.deposit_amount);
    assert_eq!(decoded.proof, params.proof);

    assert_eq!(params.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_withdraw_params_encode_decode_roundtrip() {
    let params = WithdrawParamsV1 {
        purse_id: PurseId(pallas::Base::from(99u64)),
        withdraw_amount: 500u64,
        old_balance_commit: dummy_point(),
        new_balance_commit: dummy_point(),
        nullifier: Nullifier::from_bytes([42u8; 32]).unwrap(),
        owner: dummy_pubkey(),
        proof: vec![4u8, 5, 6],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };

    let encoded = params.encode();
    assert!(!encoded.is_empty());

    let decoded = WithdrawParamsV1::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.purse_id.inner(), params.purse_id.inner());
    assert_eq!(decoded.withdraw_amount, params.withdraw_amount);
    assert_eq!(decoded.proof, params.proof);

    assert_eq!(params.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_decode_rejects_empty() {
    assert!(Purse::decode(&[]).is_err());
    assert!(DepositParamsV1::decode(&[]).is_err());
    assert!(WithdrawParamsV1::decode(&[]).is_err());
}
