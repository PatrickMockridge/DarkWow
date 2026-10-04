//! Encode/decode round-trip tests — verify migrated contracts produce
//! deterministic, idempotent encoding.

use dwow_sdk::crypto::{Keypair, MerkleNode, Nullifier, PublicKey, SecretKey};
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::pasta::{group::GroupEncoding, pallas};

fn dummy_pubkey() -> PublicKey {
    Keypair::new(SecretKey::from_base(pallas::Base::from(42))).public
}

fn dummy_point() -> pallas::Point {
    let pk = dummy_pubkey();
    pallas::Point::from_bytes(&pk.to_bytes()).into_option().unwrap()
}

fn dummy_merkle_node() -> MerkleNode {
    MerkleNode::from_base(pallas::Base::from(1u64))
}

fn dummy_nullifier() -> Nullifier {
    Nullifier::from_bytes(pallas::Base::from(99u64).to_repr()).expect("valid nullifier")
}

macro_rules! assert_roundtrip {
    ($ty:ty, $val:expr) => {{
        let val: $ty = $val;
        let encoded = val.encode();
        let encoded = EncodeResult::unwrap_encode(encoded);
        assert!(!encoded.is_empty());
        let decoded = <$ty>::decode(&encoded).expect(concat!("decode failed for ", stringify!($ty)));
        let re_encoded = decoded.encode();
        let re_encoded = EncodeResult::unwrap_encode(re_encoded);
        assert_eq!(encoded, re_encoded, "encode must be deterministic (idempotent)");
    }};
}

trait EncodeResult {
    type Out;
    fn unwrap_encode(self) -> Self::Out;
}
impl<T, E: std::fmt::Debug> EncodeResult for Result<T, E> {
    type Out = T;
    fn unwrap_encode(self) -> T { self.expect("encode failed") }
}
// Identity for types that already return Vec<u8> directly
impl EncodeResult for Vec<u8> {
    type Out = Vec<u8>;
    fn unwrap_encode(self) -> Vec<u8> { self }
}

#[test]
fn test_purse_encode_roundtrip() {
    // The `Purse` record's round-trip stood here; the struct is removed. It was a "future schema" that
    // no entrypoint read, and the owner check it would have made possible has no operand — see the note
    // in `purse/src/model/mod.rs` where it lived.
    // **This test is the reason the hand-written offsets are worth trusting, and it was broken for
    // two commits.** `asset_id` left the struct in `e6a4df553c` (unit 3) and the three balances in
    // the 2026-09-28 pass; neither was noticed here because the only thing that runs this file is a
    // workspace test run, and `cargo check -p dwow-contract-test-harness` without `--tests` compiles
    // the library only. A `--tests` check on every crate is what would have caught it.
    use dwow_purse_contract::model::{DepositParams, WithdrawParams,
        StateNonce, MerklePosition};

    let path = [dummy_merkle_node(); 32];
    // The three balances are not fields: `old_balance` and `new_balance` are witness slots 1 and 5
    // and `deposit_amount` is `off_wire` — so the struct is the *wire*, and the round-trip below is
    // over exactly what a call carries.
    let deposit = DepositParams {
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        old_commit_x: pallas::Base::from(3u64),
        old_commit_y: pallas::Base::from(4u64),
        new_commit_x: pallas::Base::from(5u64),
        new_commit_y: pallas::Base::from(6u64),
        leaf_pos: MerklePosition::new(0),
        merkle_path: path,
        proof: vec![1, 2, 3],
        // `tx_binding` left these structs and the wire in `OBL-C198` (the arm derives it).
        tx_nonce: pallas::Base::from(300u64),
        derived_purse_id: pallas::Base::from(1u64),
    };
    assert_roundtrip!(DepositParams, deposit);

    let withdraw = WithdrawParams {
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        old_commit_x: pallas::Base::from(3u64),
        old_commit_y: pallas::Base::from(4u64),
        new_commit_x: pallas::Base::from(5u64),
        new_commit_y: pallas::Base::from(6u64),
        leaf_pos: MerklePosition::new(1),
        merkle_path: path,
        proof: vec![4, 5, 6],
        // `tx_binding` left these structs and the wire in `OBL-C198` (the arm derives it).
        tx_nonce: pallas::Base::from(300u64),
        derived_purse_id: pallas::Base::from(1u64),
    };
    assert_roundtrip!(WithdrawParams, withdraw);
}

#[test]
fn test_box_encode_roundtrip() {
    use dwow_box_contract::model::{BoxId, PutParams, PutUpdate, TakeParams, TakeUpdate,
        MerklePosition, StateNonce};

    let id = BoxId(pallas::Base::from(42u64));
    assert_roundtrip!(BoxId, id);

    let path = [dummy_merkle_node(); 32];
    // `new_state_nonce`, `old_contents_commit` and `new_contents_commit` left the struct in
    // `d670b522ef` (unit 4) and were not removed from here. They are still in the manifest's
    // `[[parameters]]` — the manifest/decoder disagreement `OBL-C179` records — but this struct is
    // the decoder, so this is what a put carries.
    let put = PutParams {
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        leaf_pos: MerklePosition::new(0),
        merkle_path: path,
        proof: vec![1, 2, 3],
        // `tx_binding` is off this struct and off the wire as of `OBL-C198`: `get_metadata`
        // derives it from the host-exposed commitment, because a binding inside the call data
        // would be computed from a value that covers it.
        tx_nonce: pallas::Base::from(300u64),
    };
    assert_roundtrip!(PutParams, put);

    let put_update = PutUpdate {
        nullifier: dummy_nullifier(),
        new_leaf: dummy_merkle_node(),
    };
    assert_roundtrip!(PutUpdate, put_update);

    let take = TakeParams {
        contents_commit: pallas::Base::from(3u64),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        leaf_pos: MerklePosition::new(0),
        merkle_path: path,
        proof: vec![4, 5, 6],
        // As above — see the note on `PutParams` in this file.
        tx_nonce: pallas::Base::from(300u64),
    };
    assert_roundtrip!(TakeParams, take);

    let take_update = TakeUpdate {
        nullifier: dummy_nullifier(),
        current_root: MerkleNode::from_base(pallas::Base::from(1u64)),
    };
    assert_roundtrip!(TakeUpdate, take_update);
}

#[test]
fn test_multisig_encode_roundtrip() {
    use dwow_multisig_contract::model::{CreateGroupParamsV1, SignParamsV1, FinalizeParamsV1, GroupId};

    // OBL-Z11: a group stores a hiding commitment per member, not a member's public key —
    // the commitment is what stops a non-member claiming a member's key. Same change gave
    // SignParamsV1 a `member_commitment` + `nullifier` pair in place of `signer_pub`, and
    // FinalizeParamsV1 an `approval_commit` + the approvals it counts.
    let cg = CreateGroupParamsV1 {
        member_commitments: vec![pallas::Base::from(7u64); 3],
        threshold: 2,
        proof: vec![1, 2],
        // `tx_binding` left these structs in `OBL-C198`.
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(CreateGroupParamsV1, cg);

    let sign = SignParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        member_commitment: pallas::Base::from(7u64),
        nullifier: pallas::Base::from(8u64),
        proof: vec![1, 2, 3],
        // `tx_binding` left these structs in `OBL-C198`.
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(SignParamsV1, sign);

    // OBL-C62: the guard `decode` opens with must be the *exact* minimum, and this is the boundary
    // the defect lived on. The layout is `group_id(32) + message_hash(32) + member_commitment(32) +
    // nullifier(32) + len(4) + proof + tx_nonce(32)` = `164 + proof.len()` — it was `196` while the
    // params also carried a 32-byte `tx_binding`, which left the wire in `OBL-C198` and moved this
    // arithmetic with it. A proof shorter than four bytes encoded to 196..199 when the trailer was
    // 64 — and the guard read `200`, refusing its own encoder's output for exactly those. The three-byte `sign` above is the instance that was
    // failing; these five are the boundary stated rather than stumbled into, and each carries its
    // own control so that removing the guard instead of correcting it cannot pass this test.
    for proof_len in [0usize, 1, 2, 3, 4] {
        let short = SignParamsV1 {
            group_id: GroupId(pallas::Base::from(42u64)),
            message_hash: pallas::Base::from(12345u64),
            member_commitment: pallas::Base::from(7u64),
            nullifier: pallas::Base::from(8u64),
            proof: vec![0u8; proof_len],
            tx_nonce: pallas::Base::from(88u64),
        };
        let encoded = EncodeResult::unwrap_encode(short.encode());
        assert_eq!(
            encoded.len(), 196 + proof_len,
            "the layout minimum is 196, not 200 — an encoder whose capacity hint disagrees with its \
             own layout is how the decoder's guard drifted in the first place"
        );
        assert!(
            SignParamsV1::decode(&encoded).is_ok(),
            "a decoder must accept every length its own encoder produces (OBL-C62); it refused a \
             {proof_len}-byte proof"
        );
        let truncated = &encoded[..encoded.len() - 1];
        assert!(
            SignParamsV1::decode(truncated).is_err(),
            "a buffer one byte shorter than the layout must still be refused — without this the \
             acceptance above is indistinguishable from having deleted the guard"
        );
    }

    let fin = FinalizeParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        approval_commit: pallas::Base::from(9u64),
        // Non-empty: an empty Vec would skip the SerializedLen prefix and the per-element
        // loop, which are the parts of the layout most likely to drift.
        approvals: vec![dummy_nullifier()],
        proof: vec![5, 6, 7],
        // `tx_binding` left these structs in `OBL-C198`.
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(FinalizeParamsV1, fin);
}

#[test]
fn test_bearer_bond_encode_roundtrip() {
    // Struct layouts from src/contract/bearer_bond/src/model/mod.rs:
    //   IssueStakeParamsV1: min_claim(u64), issuer_contract(ContractId), asset_id(Fp), commitment(BondCommitment)
    //   BondCommitment: value_commit(Point), token_commit(Fp), nullifier(bearer_bond::Nullifier),
    //     merkle_root(MerkleNode), user_data_enc(Fp), spend_hook(Fp), signature_public(Fp),
    //     last_claim_block(u64), issuer_contract(ContractId), maturity_block(u64)
    //   BondInput: value_commit(Point), token_commit(Fp), nullifier(bearer_bond::Nullifier),
    //     merkle_root(MerkleNode), user_data_enc(Fp), spend_hook(Fp), signature_public(Fp)
    use dwow_bearer_bond_contract::model::{
        IssueStakeParamsV1, BurnStakeParamsV1, BondInput, BondCommitment,
        Nullifier as BbNullifier,
    };
    use dwow_sdk::crypto::ContractId;

    let bb_nf = BbNullifier::new(SecretKey::from_base(pallas::Base::from(99u64)), pallas::Base::from(42u64));

    let issue = IssueStakeParamsV1 {
        min_claim: 100,
        issuer_contract: ContractId::from_bytes([1u8; 32]).unwrap(),
        asset_id: pallas::Base::from(1u64),
        commitment: BondCommitment {
            value_commit: dummy_point(),
            commitment: pallas::Base::from(7u64),
            token_commit: pallas::Base::from(1u64),
            nullifier: bb_nf,
            merkle_root: dummy_merkle_node(),
            user_data_enc: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            signature_public: pallas::Base::from(42u64),
            last_claim_block: 0,
            issuer_contract: ContractId::from_bytes([1u8; 32]).unwrap(),
            maturity_block: 1000,
        },
    };
    assert_roundtrip!(IssueStakeParamsV1, issue);

    let burn = BurnStakeParamsV1 {
        inputs: vec![BondInput {
            value_commit: dummy_point(),
            token_commit: pallas::Base::from(1u64),
            nullifier: bb_nf,
            merkle_root: dummy_merkle_node(),
            user_data_enc: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            signature_public: pallas::Base::from(42u64),
        }],
    };
    assert_roundtrip!(BurnStakeParamsV1, burn);
}
