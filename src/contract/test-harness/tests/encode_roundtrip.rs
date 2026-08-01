//! Encode/decode round-trip tests — verify migrated contracts produce
//! deterministic, idempotent encoding.

use dwow_sdk::crypto::{Keypair, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

fn dummy_pubkey() -> PublicKey {
    Keypair::new(SecretKey::from_base(pallas::Base::from(42))).public
}

fn dummy_point() -> pallas::Point {
    use dwow_sdk::pasta::group::GroupEncoding;
    let pk = dummy_pubkey();
    pallas::Point::from_bytes(&pk.to_bytes()).into_option().unwrap()
}

macro_rules! assert_roundtrip {
    ($val:expr) => {{
        let encoded = $val.encode();
        assert!(!encoded.is_empty());
        let decoded = $val.decode(&encoded).expect(concat!("decode failed for ", stringify!($val)));
        let re_encoded = decoded.encode();
        assert_eq!(encoded, re_encoded, "encode must be deterministic (idempotent)");
    }};
}

#[test]
fn test_purse_encode_roundtrip() {
    use dwow_purse_contract::model::{Purse, PurseId, DepositParams, WithdrawParams};
    let path = [pallas::Base::zero(); 32];

    let purse = Purse {
        version: 0,
        purse_id: PurseId(pallas::Base::from(99u64)),
        token_commit: pallas::Base::from(1u64),
        balance_commit: dummy_point(),
        owner_commit: pallas::Base::from(2u64),
    };
    assert_roundtrip!(purse);

    let deposit = DepositParams {
        purse_id: PurseId(pallas::Base::from(99u64)),
        old_balance: 0,
        deposit_amount: 1000,
        new_balance: 1000,
        state_nonce: pallas::Base::from(1u64),
        leaf_pos: 0,
        merkle_path: path,
        owner: dummy_pubkey(),
        proof: vec![1, 2, 3],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };
    assert_roundtrip!(deposit);

    let withdraw = WithdrawParams {
        purse_id: PurseId(pallas::Base::from(99u64)),
        old_balance: dwow_purse_contract::model::Balance::new(1000),
        withdraw_amount: dwow_purse_contract::model::Amount::new(500).unwrap(),
        new_balance: dwow_purse_contract::model::Balance::new(500),
        state_nonce: dwow_purse_contract::model::StateNonce::new(pallas::Base::from(2u64)),
        nullifier: dummy_nullifier(),
        expected_root: dummy_merkle_node(),
        new_leaf: dummy_merkle_node(),
        old_commit_x: pallas::Base::from(3u64),
        old_commit_y: pallas::Base::from(4u64),
        new_commit_x: pallas::Base::from(5u64),
        new_commit_y: pallas::Base::from(6u64),
        leaf_pos: dwow_purse_contract::model::MerklePosition::new(1),
        merkle_path: path,
        proof: vec![4, 5, 6],
        tx_binding: pallas::Base::from(200u64),
        tx_nonce: pallas::Base::from(300u64),
    };
    assert_roundtrip!(withdraw);
}

#[test]
fn test_box_encode_roundtrip() {
    use dwow_box_contract::model::{BoxRecord, BoxId, PutParams, TakeParams};

    let record = BoxRecord {
        version: 0,
        box_id: BoxId(pallas::Base::from(99u64)),
        contents_commit: pallas::Base::from(12345u64),
        is_empty: false,
    };
    assert_roundtrip!(record);

    let id = BoxId(pallas::Base::from(42u64));
    assert_roundtrip!(id);
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
    assert_roundtrip!(cg);

    let sign = SignParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        signer_pub: dummy_pubkey(),
        proof: vec![1, 2, 3],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(sign);

    let fin = FinalizeParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        proof: vec![5, 6, 7],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };
    assert_roundtrip!(fin);
}

#[test]
fn test_bearer_bond_encode_roundtrip() {
    use dwow_bearer_bond_contract::model::{IssueStakeParamsV1, BurnStakeParamsV1};

    let issue = IssueStakeParamsV1 {
        series_token_id: pallas::Base::from(1u64),
        interest_rate_bps: 500,
        maturity_block: 1000,
        bond_amount: 10000,
        min_coverage_bps: 15000,
        issuer_pub: dummy_pubkey(),
    };
    assert_roundtrip!(issue);

    use dwow_bearer_bond_contract::model::BondInput;
    let burn = BurnStakeParamsV1 {
        inputs: vec![BondInput {
            coin: pallas::Base::from(99u64),
            value_commit: pallas::Base::from(100u64),
            token_commit: pallas::Base::from(1u64),
            merkle_root: pallas::Base::from(2u64),
            user_data_enc: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            signature_public: pallas::Base::from(42u64),
        }],
    };
    assert_roundtrip!(burn);
}
