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
    use dwow_purse_contract::model::{Purse, PurseId, DepositParams, WithdrawParams,
        Balance, Amount, StateNonce, MerklePosition};

    let purse = Purse {
        version: 0,
        purse_id: PurseId(pallas::Base::from(99u64)),
        token_commit: pallas::Base::from(1u64),
        balance_commit: dummy_point(),
        owner_commit: pallas::Base::from(2u64),
    };
    assert_roundtrip!(Purse, purse);

    let path = [dummy_merkle_node(); 32];
    let deposit = DepositParams {
        purse_id: PurseId(pallas::Base::from(99u64)),
        old_balance: Balance::new(0),
        deposit_amount: Amount::new(1000).unwrap(),
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
        merkle_path: path,
        proof: vec![1, 2, 3],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };
    assert_roundtrip!(DepositParams, deposit);

    let withdraw = WithdrawParams {
        purse_id: PurseId(pallas::Base::from(99u64)),
        old_balance: Balance::new(1000),
        withdraw_amount: Amount::new(500).unwrap(),
        new_balance: Balance::new(500),
        state_nonce: StateNonce::new(pallas::Base::from(2u64)),
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
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
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
    let put = PutParams {
        box_id: BoxId(pallas::Base::from(99u64)),
        old_state_nonce: StateNonce::new(pallas::Base::from(1u64)),
        new_state_nonce: StateNonce::new(pallas::Base::from(2u64)),
        old_contents_commit: pallas::Base::from(3u64),
        new_contents_commit: pallas::Base::from(4u64),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        leaf_pos: MerklePosition::new(0),
        merkle_path: path,
        proof: vec![1, 2, 3],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };
    assert_roundtrip!(PutParams, put);

    let put_update = PutUpdate {
        nullifier: dummy_nullifier(),
        new_leaf: dummy_merkle_node(),
    };
    assert_roundtrip!(PutUpdate, put_update);

    let take = TakeParams {
        box_id: BoxId(pallas::Base::from(99u64)),
        contents_commit: pallas::Base::from(3u64),
        state_nonce: StateNonce::new(pallas::Base::from(1u64)),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        leaf_pos: MerklePosition::new(0),
        merkle_path: path,
        proof: vec![4, 5, 6],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };
    assert_roundtrip!(TakeParams, take);

    let take_update = TakeUpdate {
        nullifier: dummy_nullifier(),
    };
    assert_roundtrip!(TakeUpdate, take_update);
}

#[test]
fn test_multisig_encode_roundtrip() {
    use dwow_multisig_contract::model::{CreateGroupParamsV1, SignParamsV1, FinalizeParamsV1, GroupId};

    let cg = CreateGroupParamsV1 {
        pubkeys: vec![dummy_pubkey(); 3],
        threshold: 2,
        proof: vec![1, 2],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(CreateGroupParamsV1, cg);

    let sign = SignParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        signer_pub: dummy_pubkey(),
        proof: vec![1, 2, 3],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(SignParamsV1, sign);

    let fin = FinalizeParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        proof: vec![5, 6, 7],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(FinalizeParamsV1, fin);
}

#[test]
fn test_bearer_bond_encode_roundtrip() {
    // Struct layouts from src/contract/bearer_bond/src/model/mod.rs:
    //   IssueStakeParamsV1: min_claim(u64), issuer_contract(ContractId), token_id(Fp), coin(BondCoin)
    //   BondCoin: value_commit(Point), token_commit(Fp), nullifier(bearer_bond::Nullifier),
    //     merkle_root(MerkleNode), user_data_enc(Fp), spend_hook(Fp), signature_public(Fp),
    //     last_claim_block(u64), issuer_contract(ContractId), maturity_block(u64)
    //   BondInput: value_commit(Point), token_commit(Fp), nullifier(bearer_bond::Nullifier),
    //     merkle_root(MerkleNode), user_data_enc(Fp), spend_hook(Fp), signature_public(Fp)
    use dwow_bearer_bond_contract::model::{
        IssueStakeParamsV1, BurnStakeParamsV1, BondInput, BondCoin,
        Nullifier as BbNullifier,
    };
    use dwow_sdk::crypto::ContractId;

    // bearer_bond::Nullifier uses Nullifier::new(secret, coin) — tuple struct field may not be directly constructable
    let bb_nf = BbNullifier::new(pallas::Base::from(99u64), pallas::Base::from(42u64));

    let issue = IssueStakeParamsV1 {
        min_claim: 100,
        issuer_contract: ContractId::from_bytes([1u8; 32]).unwrap(),
        token_id: pallas::Base::from(1u64),
        coin: BondCoin {
            value_commit: dummy_point(),
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
