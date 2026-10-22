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
        box_tag: pallas::Base::from(3u64),
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
            encoded.len(), 164 + proof_len,
            "the layout minimum is 164, not 196 — an encoder whose capacity hint disagrees with its \
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
            series_asset_id: pallas::Base::from(1u64),
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
    // …and the case the fixture below could not fail on: an **unclaimed** commitment, whose
    // nullifier is `Nullifier::ZERO`. `encode` has always written that sentinel; `decode` refused
    // it, because it read the field with the SDK's `Nullifier::from_bytes`, which rejects zero by
    // design. So this contract could write a record it could not read, and every `IssueStakeV1`
    // was refused at `metadata-decode-zkp`. A fixture with only a real nullifier pins the defect
    // instead of the fix (`AGENTS.md` R1) — which is exactly what the assertion below did, for as
    // long as it was the only one.
    let mut unclaimed = issue.clone();
    unclaimed.commitment.nullifier = BbNullifier::ZERO;
    assert_roundtrip!(IssueStakeParamsV1, unclaimed);

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
            commitment: pallas::Base::from(9u64),
        }],
    };
    assert_roundtrip!(BurnStakeParamsV1, burn);
}

/// `SerializedLen` controls for bearer_bond: every length prefix is a 4-byte `u32`, so a
/// 256-element vector round-trips instead of truncating to `0` through an `as u8`.
#[test]
fn test_bearer_bond_serialized_len_prefixes() {
    use dwow_bearer_bond_contract::model::{
        BondInput, BurnStakeParamsV1, Nullifier as BbNullifier, ProveCoverageParamsV1,
    };
    use dwow_sdk::crypto::SecretKey;

    let bb_nf = BbNullifier::new(
        SecretKey::from_base(pallas::Base::from(99u64)),
        pallas::Base::from(42u64),
    );
    let point = dummy_point();

    // ProveCoverageParamsV1's decoder took the whole tail as the proof and never consumed
    // the length prefix, so this round-trip failed at **any** size before the migration.
    let coverage = ProveCoverageParamsV1 {
        series_asset_id: pallas::Base::from(1u64),
        total_outstanding: 10,
        total_interest_obligation: 1,
        reserve_amount: 20,
        coverage_ratio_bps: 18_181,
        report_block: 7,
        proof: vec![0u8; 256],
    };
    assert_roundtrip!(ProveCoverageParamsV1, coverage);

    // 256 inputs: with a `u8` prefix this wrote `0`, so decode expected only the prefix and
    // the remaining 65_536 bytes were unread.
    let burn = BurnStakeParamsV1 {
        inputs: (0u64..256)
            .map(|i| BondInput {
                value_commit: point,
                token_commit: pallas::Base::from(i + 1),
                nullifier: bb_nf,
                merkle_root: dummy_merkle_node(),
                user_data_enc: pallas::Base::zero(),
                spend_hook: pallas::Base::zero(),
                signature_public: pallas::Base::from(42u64),
                commitment: pallas::Base::from(i + 2),
            })
            .collect(),
    };
    assert_roundtrip!(BurnStakeParamsV1, burn.clone());

    // Negative control: claim one more element than the buffer carries. The old decoder
    // sliced past the end and panicked; the migrated one returns `Err`.
    let mut overlong = burn.encode().expect("encode must succeed");
    let count = u32::from_le_bytes(overlong[0..4].try_into().unwrap());
    overlong[0..4].copy_from_slice(&(count + 1).to_le_bytes());
    assert!(
        BurnStakeParamsV1::decode(&overlong).is_err(),
        "a 4-byte prefix claiming {} elements in a {}-byte buffer must be refused",
        count + 1,
        overlong.len(),
    );
    assert!(
        BurnStakeParamsV1::decode(&overlong[..2]).is_err(),
        "a buffer too short to hold the 4-byte prefix must be refused, not panic",
    );
}

/// `SerializedLen` controls for stablecoin: >255-length fields round-trip, and an overlong
/// prefix is a `ContractError` rather than a panic.
#[test]
fn test_stablecoin_serialized_len_prefixes() {
    use dwow_sdk::crypto::{ContractId, IntentCommitment, IntentNullifier};
    use dwow_stablecoin_contract::model::{
        AccrueInterestParams, CollateralParams, CollateralType, DeadManAction,
        DeadManSwitchConfig, DepositCollateralParams, GovernanceReportParams, InitializeParams,
        LiquidateParams, MintStableParams, RedeemStableParamsV1, RepayStableParams,
        SpendHookCallbackUpdateV1, StablecoinModel, WithdrawCollateralParams,
    };

    let zero = pallas::Base::zero();
    let commitment = IntentCommitment::from_base(pallas::Base::from(1u64));
    let intent_nf = IntentNullifier::from_base(pallas::Base::from(2u64));
    let proof = || vec![0u8; 256];
    let inputs = || vec![zero; 256];

    let deposit = DepositCollateralParams {
        deposit_commitment: commitment,
        collateral_amount: 10,
        collateral_type: CollateralType::Xmr,
        proof: proof(),
        fee: 1,
        zk_public_inputs: inputs(),
    };
    assert_roundtrip!(DepositCollateralParams, deposit);

    let withdraw = WithdrawCollateralParams {
        withdrawal_nullifier: intent_nf,
        new_commitment: commitment,
        withdraw_amount: 3,
        proof: proof(),
        fee: 0,
        zk_public_inputs: inputs(),
    };
    assert_roundtrip!(WithdrawCollateralParams, withdraw);

    let mint = MintStableParams {
        mint_commitment: commitment,
        mint_amount: 5,
        proof: proof(),
        fee: 0,
        zk_public_inputs: inputs(),
    };
    assert_roundtrip!(MintStableParams, mint.clone());

    // Negative control at the proof prefix (offset 40): one more byte than present.
    let mut overlong = mint.encode().expect("encode must succeed");
    let proof_len = u32::from_le_bytes(overlong[40..44].try_into().unwrap());
    overlong[40..44].copy_from_slice(&(proof_len + 1).to_le_bytes());
    assert!(
        MintStableParams::decode(&overlong).is_err(),
        "a proof prefix claiming {} bytes in a {}-byte buffer must be refused",
        proof_len + 1,
        overlong.len(),
    );
    assert!(
        MintStableParams::decode(&[0u8; 2]).is_err(),
        "a buffer too short to hold the fixed prefix fields must be refused, not panic",
    );

    let repay = RepayStableParams {
        repay_commitment: commitment,
        repay_amount: 4,
        proof: proof(),
        fee: 0,
        zk_public_inputs: inputs(),
    };
    assert_roundtrip!(RepayStableParams, repay);

    let liquidate = LiquidateParams {
        liquidation_commitment: commitment,
        total_debt: 10,
        total_collateral: 20,
        current_price: 2,
        debt_to_cover: 5,
        proof: proof(),
        liquidation_reward: 1,
        fee: 0,
        zk_public_inputs: inputs(),
    };
    assert_roundtrip!(LiquidateParams, liquidate);

    let redeem = RedeemStableParamsV1 {
        recipient_pub: dummy_pubkey(),
        redeem_amount: 1_000,
        asset_id: zero,
        receipt_spend_hook: zero,
        total_debt: 5_000,
        total_collateral: 10_000,
        redeem_value_blind: pallas::Base::from(7u64),
        proof: proof(),
        fee: 7,
        zk_public_inputs: inputs(),
    };
    assert_roundtrip!(RedeemStableParamsV1, redeem);

    let governance = GovernanceReportParams {
        asset_id: zero,
        total_collateral: 10,
        total_debt: 10,
        total_redeemed: 0,
        outstanding: 10,
        collateral_ratio_bps: 10_000,
        interest_accrued: 0,
        reporter_pub: dummy_pubkey(),
        proof: proof(),
        fee: 0,
    };
    assert_roundtrip!(GovernanceReportParams, governance);

    let accrue = AccrueInterestParams {
        old_total_debt: 1,
        new_total_debt: 2,
        interest_amount: 1,
        rate_per_second: 1,
        time_elapsed: 1,
        accumulator_pub: dummy_pubkey(),
        proof: proof(),
        fee: 0,
    };
    assert_roundtrip!(AccrueInterestParams, accrue);

    let spend_hook = SpendHookCallbackUpdateV1 {
        nullifiers: vec![dummy_nullifier(); 256],
        value_commits: vec![[0u8; 64]; 256],
        new_total_redeemed: 5,
    };
    assert_roundtrip!(SpendHookCallbackUpdateV1, spend_hook);

    let init = InitializeParams {
        model: StablecoinModel::PooledDebt,
        min_collateralization_ratio: 15_000,
        liquidation_threshold: 11_000,
        liquidation_penalty: 500,
        base_rate: 0,
        pi_kp: 0,
        pi_ki: 0,
        twap_window: 0,
        price_deviation_threshold: 0,
        collateral_params: (0..256)
            .map(|_| CollateralParams {
                collateral_type: CollateralType::Drk,
                haircut: 9_850,
                liquidation_threshold: 11_000,
                max_debt_share: 30_000,
            })
            .collect(),
        dead_man_switch: DeadManSwitchConfig {
            enabled: false,
            timeout_blocks: 0,
            action: DeadManAction::DisableMinting,
            last_action_block: 0,
        },
        token_authority_pub: dummy_pubkey(),
        create_token: false,
        token_symbol: [0u8; 32],
        deployer_auth: zero,
        promissory_note_contract_id: ContractId::from_bytes([1u8; 32]).unwrap(),
        governance_pub_x: zero,
        governance_pub_y: zero,
        tx_nonce: zero,
    };
    assert_roundtrip!(InitializeParams, init);
}

/// `SerializedLen` controls for dex: a 256-byte proof round-trips through every params codec,
/// and an overlong proof prefix is refused.
#[test]
fn test_dex_serialized_len_prefixes() {
    use dwow_dex_contract::model::{
        CancelSwapParams, ExecuteSwapFeeParams, ExecuteSwapParams, ExecuteSwapSlippageParams,
    };
    use dwow_promissory_note_contract::model::CapCommitment;

    let lock = CapCommitment::decode(&pallas::Base::from(1u64).to_repr()).expect("valid cap");
    let nf = dummy_nullifier();
    let nonce = pallas::Base::from(4u64);

    let execute = ExecuteSwapParams {
        swap_id: [1u8; 32],
        alice_secret: [2u8; 32],
        bob_secret: [3u8; 32],
        alice_lock: lock,
        bob_lock: lock,
        alice_nullifier: nf,
        bob_nullifier: nf,
        proof: vec![0u8; 256],
        fee: 0,
        tx_nonce: nonce,
    };
    assert_roundtrip!(ExecuteSwapParams, execute.clone());

    // Negative control at the proof prefix (offset 224).
    let mut overlong = execute.encode().expect("encode must succeed");
    let proof_len = u32::from_le_bytes(overlong[224..228].try_into().unwrap());
    overlong[224..228].copy_from_slice(&(proof_len + 1).to_le_bytes());
    assert!(
        ExecuteSwapParams::decode(&overlong).is_err(),
        "a proof prefix claiming {} bytes in a {}-byte buffer must be refused",
        proof_len + 1,
        overlong.len(),
    );
    assert!(
        ExecuteSwapParams::decode(&overlong[..2]).is_err(),
        "a buffer too short to hold the fixed prefix fields must be refused, not panic",
    );

    let cancel = CancelSwapParams {
        swap_id: [1u8; 32],
        secret: [2u8; 32],
        nullifier: nf,
        proof: vec![0u8; 256],
        fee: 0,
        tx_nonce: nonce,
    };
    assert_roundtrip!(CancelSwapParams, cancel);

    let fee_params = ExecuteSwapFeeParams {
        swap_id: [1u8; 32],
        alice_secret: [2u8; 32],
        bob_secret: [3u8; 32],
        alice_lock: lock,
        bob_lock: lock,
        alice_nullifier: nf,
        bob_nullifier: nf,
        fee_bps: 30,
        proof: vec![0u8; 256],
        fee: 1,
        tx_nonce: nonce,
    };
    assert_roundtrip!(ExecuteSwapFeeParams, fee_params);

    let slippage = ExecuteSwapSlippageParams {
        swap_id: [1u8; 32],
        alice_secret: [2u8; 32],
        bob_secret: [3u8; 32],
        alice_lock: lock,
        bob_lock: lock,
        alice_nullifier: nf,
        bob_nullifier: nf,
        slippage_bps: 30,
        proof: vec![0u8; 256],
        fee: 0,
        tx_nonce: nonce,
    };
    assert_roundtrip!(ExecuteSwapSlippageParams, slippage);
}
