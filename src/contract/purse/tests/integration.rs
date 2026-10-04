//! Integration tests for the Purse contract — data model encode/decode round-trips.

use dwow_purse_contract::model::{BalanceParams, DepositParams, MerklePosition, WithdrawParams};
use dwow_sdk::crypto::{pasta_prelude::PrimeField, MerkleNode, Nullifier};
use dwow_sdk::pasta::pallas;

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

// `Amount`'s and `Balance`'s five tests stood here — the newtypes are removed, because nothing
// outside their own definitions used them once the balances left the call data. **The rule they
// pinned did not go with them.** `test_amount_rejects_zero` asserted `Amount::new(0).is_err()`, i.e.
// that a zero deposit is invalid; that check lived in the contract's *decoder*, which cannot see the
// amount any more, and `deposit.zk` now carries `less_than_strict(ZERO, deposit_amount)` beside
// `withdraw.zk:61`'s. **The test for it belongs at the circuit, and it is not here yet** — building a
// zero-amount deposit and requiring the proof to fail is a heavyweight-harness test, and unit 6's
// verification runs `test_heavyweight_purse` but does not add one. Recorded rather than replaced by
// nothing.

// `test_purse_encode_decode_roundtrip` — the `Purse` record's 129-byte round-trip — stood here. The
// record is removed: it was a "future schema" that no entrypoint read, and the host-level owner check it
// would have made possible has no operand, because a deposit carries no owner and the one the circuit
// binds is not recoverable from the leaf. The note in `src/model/mod.rs` states the measurement.

#[test]
fn test_deposit_params_encode_decode_roundtrip() {
    // The three balances are not fields of this struct any more (2026-09-28): two are witness slots
    // the wallet's record and the circuit supply, and the amount is `off_wire` in the manifest.
    let params = DepositParams {
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
        // `tx_binding` removed from the params and the wire (`OBL-C198`): the arm derives it from
        // the host-exposed commitment.
        tx_nonce: pallas::Base::from(300u64),
        // `asset_id` was here and left the wire in `e6a4df553c` (unit 3). It is witness slot 22,
        // sourced `note:asset_id`, and what replaced it as the final wire field is the purse
        // identity — a one-way function of the id, the only form `privacy.md` §5.5 permits public.
        derived_purse_id: pallas::Base::from(1u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = DepositParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.expected_root.to_bytes(), params.expected_root.to_bytes());
    assert_eq!(decoded.derived_purse_id, params.derived_purse_id);
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_withdraw_params_encode_decode_roundtrip() {
    let params = WithdrawParams {
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
        // `tx_binding` removed from the params and the wire (`OBL-C198`): the arm derives it from
        // the host-exposed commitment.
        tx_nonce: pallas::Base::from(300u64),
        // `asset_id` was here and left the wire in `e6a4df553c` (unit 3); see the note above.
        derived_purse_id: pallas::Base::from(1u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = WithdrawParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.expected_root.to_bytes(), params.expected_root.to_bytes());
    assert_eq!(decoded.derived_purse_id, params.derived_purse_id);
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_balance_params_encode_decode_roundtrip() {
    let params = BalanceParams {
        derived_purse_id: pallas::Base::from(2u64),
        expected_root: dummy_merkle_node(),
        token_commit: pallas::Base::from(3u64),
        balance_commit_x: pallas::Base::from(4u64),
        balance_commit_y: pallas::Base::from(5u64),
        leaf_pos: MerklePosition::new(0),
        merkle_path: dummy_merkle_path(),
        proof: vec![7u8, 8, 9],
        // `tx_binding` removed from the params and the wire (`OBL-C198`): the arm derives it from
        // the host-exposed commitment.
        tx_nonce: pallas::Base::from(300u64),
    };

    let encoded = params.encode().expect("encode must succeed");
    assert!(!encoded.is_empty());

    let decoded = BalanceParams::decode(&encoded).expect("round-trip must succeed");
    assert_eq!(decoded.token_commit, params.token_commit);
    assert_eq!(decoded.proof, params.proof);

    let re_encoded = params.encode().expect("re-encode must succeed");
    assert_eq!(re_encoded, encoded, "encode must be deterministic");
}

#[test]
fn test_decode_rejects_empty() {
    assert!(DepositParams::decode(&[]).is_err());
    assert!(WithdrawParams::decode(&[]).is_err());
    assert!(BalanceParams::decode(&[]).is_err());
}
