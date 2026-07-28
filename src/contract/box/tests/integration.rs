//! Integration tests for the Box contract — data model encode/decode round-trips.

use dwow_box_contract::model::{BoxRecord, PutParamsV1, TakeParamsV1, BoxId};
use dwow_sdk::crypto::{Keypair, Nullifier, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

fn dummy_pubkey() -> PublicKey {
    Keypair::new(SecretKey::from_base(pallas::Base::from(42))).public
}

#[test]
fn test_box_record_encode_decode_roundtrip() {
    let record = BoxRecord {
        version: 0,
        box_id: BoxId(pallas::Base::from(99u64)),
        contents_commit: pallas::Base::from(12345u64),
        is_empty: false,
    };

    let encoded = record.encode();
    assert_eq!(encoded.len(), BoxRecord::ENCODED_SIZE);

    let decoded = BoxRecord::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.version, record.version);
    assert_eq!(decoded.box_id.inner(), record.box_id.inner());
    assert_eq!(decoded.contents_commit, record.contents_commit);
    assert_eq!(decoded.is_empty, record.is_empty);

    assert_eq!(record.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_put_params_encode_decode_roundtrip() {
    let params = PutParamsV1 {
        box_id: BoxId(pallas::Base::from(99u64)),
        old_contents_commit: pallas::Base::from(1u64),
        new_contents_commit: pallas::Base::from(2u64),
        owner: dummy_pubkey(),
        proof: vec![1u8, 2, 3, 4],
        tx_binding: pallas::Base::from(100u64),
        tx_nonce: pallas::Base::from(200u64),
    };

    let encoded = params.encode();
    assert!(!encoded.is_empty());

    let decoded = PutParamsV1::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.box_id.inner(), params.box_id.inner());
    assert_eq!(decoded.old_contents_commit, params.old_contents_commit);
    assert_eq!(decoded.new_contents_commit, params.new_contents_commit);
    assert_eq!(decoded.proof, params.proof);

    assert_eq!(params.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_take_params_encode_decode_roundtrip() {
    let params = TakeParamsV1 {
        box_id: BoxId(pallas::Base::from(99u64)),
        contents_commit: pallas::Base::from(5u64),
        nullifier: Nullifier::from_bytes([42u8; 32]).unwrap(),
        owner: dummy_pubkey(),
        proof: vec![5u8, 6, 7, 8],
        tx_binding: pallas::Base::from(100u64),
        tx_nonce: pallas::Base::from(200u64),
    };

    let encoded = params.encode();
    assert!(!encoded.is_empty());

    let decoded = TakeParamsV1::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.box_id.inner(), params.box_id.inner());
    assert_eq!(decoded.contents_commit, params.contents_commit);
    assert_eq!(decoded.nullifier.inner(), params.nullifier.inner());
    assert_eq!(decoded.proof, params.proof);

    assert_eq!(params.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_decode_rejects_empty() {
    assert!(BoxRecord::decode(&[]).is_err());
    assert!(PutParamsV1::decode(&[]).is_err());
    assert!(TakeParamsV1::decode(&[]).is_err());
}

#[test]
fn test_box_id_encode_decode() {
    let id = BoxId(pallas::Base::from(42u64));
    let encoded = id.encode();
    assert_eq!(encoded.len(), 32);
    let decoded = BoxId::decode(&encoded).expect("BoxId round-trip must succeed");
    assert_eq!(decoded.inner(), id.inner());
    assert_eq!(id.encode(), encoded, "BoxId encode must be deterministic");
}
