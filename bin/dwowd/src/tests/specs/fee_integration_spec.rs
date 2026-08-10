//! Fee system integration tests — fee-spec.md §14, fee-testing.md.
//!
//! Python reference: `contrib/model/fee_window_model.py` (P-IT-1 through P-IT-6).
//! Infrastructure: `HeavyweightPipeline` in `../blockchain.rs`.

use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, FeeAmount};
use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
use dwow_sdk::pasta::{group::Group, group::GroupEncoding, pallas};
use crate::tests::blockchain::HeavyweightPipeline;

/// IT-1: Full fee lifecycle — wallet constructs FeeV2 → mempool admits →
/// miner decrypts → FeeCollectV1 verifies → accumulator resets.
///
/// Python ref: `test_p_it_1_full_lifecycle`
/// Invariants: FI-GEN-1, FI-ENCRYPT-1/2/3, FI-ADMIT-1/3, FI-COLLECT-1/2, FI-FLAG-1
pub async fn run_fee_integration_full_lifecycle() -> Result<()> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, PublicKey, SecretKey};
    use dwow_sdk::blockchain::expected_reward;
    use crate::tests::modules::coinbase_coordination;

    dwow_native_token_contract::enable_deterministic_zk();

    let mut chain = HeavyweightPipeline::new().await?;
    chain.init_genesis().await?;
    chain.log_file = Some(std::sync::Mutex::new(
        crate::tests::test_output::create_log_file("fee_integration_1")
    ));

    let cid = *NATIVE_TOKEN_CONTRACT_ID;
    let native_harness = NativeTokenHarness::spawn();

    // ── Block 2: Coinbase-only (creates spendable coin) ──
    let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
    chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;
    chain.log(&format!("[IT-1-ST1] height={}", chain.height()));

    // ── Block 3: FeeV2 + FeeCollectV1 ──
    let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
    let fee_height = chain.height().succ();

    // Merkle tree for coin at height 2
    let mut tree = MerkleTree::new(1);
    tree.append(MerkleNode::from_base(pallas::Base::zero()));
    tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
    let coin_pos = tree.mark().expect("tree.mark");
    let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
    let root = tree.root(0).expect("tree.root");

    let mining_kp = chain.mining_keypair(BlockHeight::new(2));
    let fee_amount: u64 = 42_000_000;
    let threshold: u64 = 42_000_000;

    let fee_result = native_harness.fee_v2(
        cb2.coin_value,
        pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
        cb2.coin_blind,
        u64::from(coin_pos),
        path.clone(),
        root,
        mining_kp.secret.clone(),
        mining_kp.secret.clone(),
        PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
        pallas::Base::zero(), pallas::Base::zero(),
        fee_amount,
        threshold,
    ).map_err(|e| dwow_core::Error::Custom(format!(
        "[IT-1-ST2] FeeV2 harness: {}", e
    )))?;

    // ── Pre-FeeV2: accumulator Identity ──
    let acc_pre = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
        .map(|d| Option::from(pallas::Point::from_bytes(&d[..32].try_into().unwrap())))
        .flatten()
        .unwrap_or(pallas::Point::identity());
    assert_eq!(acc_pre, pallas::Point::identity(),
        "[IT-1-ST0] accumulator must be Identity before FeeV2");

    // ── Submit: FeeV2 + FeeCollectV1 ──
    let before = chain.height();
    let new_height = chain.block()?
        .with_call(cid, &native_harness, &fee_result.call_data, fee_result.proofs.clone())?
        .with_fee_collect()?
        .submit_with_coinbase(cb3.coinbase_tx.clone()).await?;
    assert!(new_height > before,
        "[IT-1-ST4-C1] height advanced: {} -> {}", before, new_height);

    // ── Accumulator reset after FeeCollectV1 ──
    let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
        .expect("[IT-1-ST5-V3] fee_commit_accumulator not found");
    let acc_point: pallas::Point = Option::from(pallas::Point::from_bytes(
        &acc_data[..32].try_into().unwrap()
    )).expect("[IT-1-ST5-V3] invalid accumulator point");
    assert_eq!(acc_point, pallas::Point::identity(),
        "[IT-1-ST5-V3] accumulator reset to Identity after FeeCollectV1");

    // ── Fee pot zeroed ──
    let fees_data = chain.query_contract_state(cid, "fees", &fee_height.to_le_bytes())?
        .expect("[IT-1-ST5-V6] fees_db entry not found");
    let fee_pot = u64::from_le_bytes(fees_data[..8].try_into().unwrap());
    assert_eq!(fee_pot, 0,
        "[IT-1-ST5-V6] fee pot zeroed (was {})", fee_pot);

    // ── Supply neutrality ──
    let supply = chain.cumulative_supply();
    let expected: u64 = (1..=3u64).map(|h| expected_reward(BlockHeight::new(h)).get()).sum();
    assert_eq!(supply, expected,
        "[IT-1-ST5-V4] supply unchanged by fees: {} == {}", supply, expected);

    // ── Nullifier written ──
    let spent_nf = fee_result.params.input.nullifier.to_bytes();
    assert!(chain.query_contract_state(cid, "nullifiers", &spent_nf)?.is_some(),
        "[IT-1-ST5-V5] spent nullifier exists on-chain");

    // ── FI-FLAG-1: fee_window_flags active ──
    let block = chain.chain_state.get_block(fee_height)
        .map_err(|e| dwow_core::Error::Custom(format!("get_block: {}", e)))?;
    assert!(block.header.fee_window_flags.is_active(),
        "[IT-1-ST5-V7] fee_window_flags active after fee-bearing block");

    chain.log("[IT-1] Full lifecycle test PASSED");
    Ok(())
}
