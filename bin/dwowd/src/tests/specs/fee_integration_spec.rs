//! Fee system integration tests — fee-spec.md §14, fee-testing.md.
//!
//! Python reference: `contrib/model/fee_window_model.py` (P-IT-1 through P-IT-6).
//! Infrastructure: `HeavyweightPipeline` in `../blockchain.rs`.

use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, FeeAmount};
use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
use dwow_sdk::pasta::pallas;
use crate::tests::blockchain::HeavyweightPipeline;

/// IT-1: Full fee lifecycle — wallet constructs FeeV3 (plaintext fee) → mempool
/// admits → FeeCollectV1 verifies plaintext `fees_db` → fee pot zeroed.
///
/// Python ref: `test_p_it_1_full_lifecycle`
/// Invariants: FI-GEN-1, FI-ADMIT-1/3, FI-COLLECT-1/2, FI-FLAG-1
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

    // ── FI-GEN-1: Genesis-initialized fee parameters ──
    {
        let cs = &chain.chain_state;
        assert!(cs.fee_window.is_some(),
            "[IT-1 FI-GEN-1] fee_window must be present after genesis");
        let fw = cs.fee_window.as_ref().unwrap();
        assert_eq!(fw.circuit_cf().premium().get(),
            dwow_chain::fee_window::CongestionFactor::SCALE,
            "[IT-1 FI-GEN-1] circuit CF initialized to SCALE at genesis");
    }

    let cid = *NATIVE_TOKEN_CONTRACT_ID;
    let native_harness = NativeTokenHarness::spawn();

    // ── Block 2: Coinbase-only (creates spendable coin) ──
    let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
    chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;
    chain.log(&format!("[IT-1-ST1] height={}", chain.height()));

    // ── Block 3: FeeV3 + FeeCollectV1 ──
    let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
    let fee_height = chain.height().succ();

    // Merkle tree: must match on-chain tree after genesis + height-2.
    // On-chain: init_contract creates [ZERO], genesis coinbase appends at pos 1,
    // height-2 coinbase appends at pos 2. Test must include all 3 leaves.
    // HAZOP §3 NO/NOT: missing genesis commitment caused TransferMerkleRootNotFound.
    let gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
    let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
    let mut tree = MerkleTree::new(1);
    tree.append(MerkleNode::from_base(pallas::Base::zero()));               // pos 0: ZERO
    tree.append(MerkleNode::from_base(gen_cb.commitment.inner()));     // pos 1: genesis
    tree.append(MerkleNode::from_base(cb2.commitment.inner()));        // pos 2: height-2
    let coin_pos = tree.mark().expect("tree.mark");
    let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
    let root = tree.root(0).expect("tree.root");

    let mining_kp = chain.mining_keypair(BlockHeight::new(2));
    let fee_amount: u64 = 1;

    let fee_result = native_harness.fee_v2(
        cb2.coin_value,
        pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
        cb2.commitment_blind,
        u64::from(coin_pos),
        path.clone(),
        root,
        mining_kp.secret.clone(),
        mining_kp.secret.clone(),
        PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
        pallas::Base::zero(), pallas::Base::zero(),
        fee_amount,
    ).map_err(|e| dwow_core::Error::Custom(format!(
        "[IT-1-ST2] FeeV3 harness: {}", e
    )))?;

    // ── FI-ADMIT-1/2/3: Mempool admission gate ──
    // Admit FeeV3 through plaintext fee, verify FCFS selection,
    // verify duplicate nullifier rejection. This is the production path —
    // transactions enter through mempool, not direct block submission.
    let (selected_tx_data, selected_proofs) = {
        use crate::NativeTokenFeeSignallingExtractor;
        let extractor = NativeTokenFeeSignallingExtractor::new();
        let mp = dwow_mempool::create_mempool(
            Box::new(extractor),
            Some(chain.chain_state.clone()),
        );
        mp.update_tier_prices(
            FeeAmount::new(1), FeeAmount::new(1), FeeAmount::new(1),
        );
        let nf = fee_result.params.input.nullifier;
        let chain_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: cid,
                data: fee_result.call_data.clone(),
            }],
            lock_time: 0,
            nullifiers: vec![nf],
            witness: vec![],
        };
        // FI-ADMIT-1: FeeV3 tx admitted to the high queue via plaintext fee.
        let tx_hash = mp.add(chain_tx.clone()).await
            .map_err(|e| dwow_core::Error::Custom(format!(
                "[IT-1 FI-ADMIT-1] mempool add failed: {}", e
            )))?;
        assert_eq!(mp.high_queue_len(), 1,
            "[IT-1 FI-ADMIT-1] high queue must have 1 tx (fee {} >= price_high 1)",
            fee_amount);
        // FI-ADMIT-3a: Verify nullifier IS tracked in mempool after admission.
        assert!(mp.has_nullifier(&nf).await,
            "[IT-1 FI-ADMIT-3] nullifier must be tracked in mempool after admission");
        // FI-ADMIT-2: FCFS selection — high queue drains first.
        let selected = mp.select_for_block(&dwow_mempool::MinerConfig::default()).await;
        assert_eq!(selected.len(), 1,
            "[IT-1 FI-ADMIT-2] select_for_block must return 1 tx");
        assert_eq!(mp.high_queue_len(), 0,
            "[IT-1 FI-ADMIT-2] high queue drained after selection");
        // FI-ADMIT-3b: Duplicate nullifier rejected (NOT hash dedup).
        // Build a different tx with the SAME nullifier — different hash ensures
        // the rejection comes from nullifier dedup, not tx-hash dedup.
        let mut dup_data = fee_result.call_data.clone();
        dup_data.push(0xFF); // different hash, same nullifier
        let dup_tx = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: cid,
                data: dup_data,
            }],
            lock_time: 0,
            nullifiers: vec![nf],
            witness: vec![],
        };
        assert_ne!(dup_tx.hash(), chain_tx.hash(),
            "[IT-1 FI-ADMIT-3] dup tx must have different hash from original");
        let dup_result = mp.add(dup_tx).await;
        assert!(dup_result.is_err(),
            "[IT-1 FI-ADMIT-3] duplicate nullifier must be rejected, got {:?}",
            dup_result.ok());
        let err_msg = format!("{:?}", dup_result.err());
        assert!(err_msg.contains("nullifier"),
            "[IT-1 FI-ADMIT-3] error must cite 'nullifier' (nullifier dedup), \
             not 'already in mempool' (hash dedup). Got: {}", err_msg);
        (selected[0].contract_calls[0].data.clone(), fee_result.proofs.clone())
    };

    // ── Submit: FeeV3 + FeeCollectV1 ──
    let before = chain.height();
    eprintln!("[IT-1] Block 3 coinbase tx hash: {}", cb3.coinbase_tx.hash());
    eprintln!("[IT-1] Block 3 fee_amount={}", fee_amount);
    let new_height = chain.block()?
        .with_call(cid, &native_harness, &selected_tx_data, selected_proofs)?
        .add_fee(FeeAmount::new(fee_amount))
        .with_fee_collect()?
        .submit_with_coinbase(cb3.coinbase_tx.clone()).await?;
    assert!(new_height > before,
        "[IT-1-ST4-C1] height advanced: {} -> {}", before, new_height);

    // ── Fee pot zeroed (FeeV3: plaintext fees_db, no accumulator) ──
    let fees_data = chain.query_contract_state(cid, "fees", &fee_height.to_le_bytes())?
        .expect("[IT-1-ST5-V6] fees_db entry not found");
    let fee_pot = u64::from_le_bytes(
        fees_data[..8].try_into()
            .map_err(|_| dwow_core::Error::Custom(
                "corrupt fees_db entry: wrong length".into()))?
    );
    assert_eq!(fee_pot, 0,
        "[IT-1-ST5-V6] fee pot zeroed (was {})", fee_pot);

    // ── FI-COLLECT-2: Supply neutrality ──
    // Read the ACTUAL on-chain cumulative supply from the committed
    // supply_chain sled tree, not the locally-recomputed value.
    let sc = chain.chain_state.supply_chain.get(BlockHeight::new(3))
        .expect("[IT-1 FI-COLLECT-2] supply_chain at height 3");
    let expected: u64 = (1..=3u64).map(|h| expected_reward(BlockHeight::new(h)).get()).sum();
    assert_eq!(sc.total_supply.get(), expected,
        "[IT-1 FI-COLLECT-2] on-chain cumulative supply ({}) == expected sum of rewards ({}). \
         Fees transfer value; they do not create or destroy it.",
        sc.total_supply.get(), expected);
    // Double-check: the locally-computed supply must match the on-chain value.
    let local_supply = chain.cumulative_supply();
    assert_eq!(local_supply, sc.total_supply.get(),
        "[IT-1 FI-COLLECT-2] local cumulative_supply ({}) must match on-chain ({})",
        local_supply, sc.total_supply.get());

    // ── Nullifier written ──
    let spent_nf = fee_result.params.input.nullifier.to_bytes();
    assert!(chain.query_contract_state(cid, "nullifiers", &spent_nf)?.is_some(),
        "[IT-1-ST5-V5] spent nullifier exists on-chain");

    // ── FI-FLAG-1: fee_window_flags chain-synced ──
    // A wallet reading flags from block N SHALL derive the same CFs that
    // the miner used to set mempool thresholds. The test harness submits
    // blocks directly without the miner PID loop, so flags default to zero
    // (no congestion). The assertion verifies the derive_cfs roundtrip is
    // correct for the default state: FeeWindowFlags(0) → CongestionFactor
    // with premium == standard == SCALE.
    let block = chain.chain_state.get_block(fee_height)
        .map_err(|e| dwow_core::Error::Custom(format!("get_block: {}", e)))?;
    let flags = block.header.fee_window_flags;
    let (derived_cf, derived_wf) = flags.derive_cfs();
    assert_eq!(derived_cf.premium().get(), dwow_chain::fee_window::CongestionFactor::SCALE,
        "[IT-1 FI-FLAG-1] default flags => circuit CF == SCALE, got {}",
        derived_cf.premium().get());
    assert_eq!(derived_wf.premium().get(), dwow_chain::fee_window::CongestionFactor::SCALE,
        "[IT-1 FI-FLAG-1] default flags => wasm CF == SCALE, got {}",
        derived_wf.premium().get());

    chain.log("[IT-1] Full lifecycle test PASSED");
    Ok(())
}

/// IT-2: Mempool admission → FeeCollectV1 → block acceptance.
///
/// Production steps tested: 9-17 (mempool receipt, plaintext fee admission,
/// tier assignment, miner tx selection, FeeCollectV1 build).
///
/// Invariants: FI-ADMIT-1/2/3, FI-COLLECT-1/2
pub async fn run_fee_integration_mempool_lifecycle() -> Result<()> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, PublicKey, SecretKey};
    use dwow_sdk::blockchain::expected_reward;
    use crate::tests::modules::coinbase_coordination;
    use dwow_mempool::FeeSignallingExtractor;
    use crate::NativeTokenFeeSignallingExtractor;

    dwow_native_token_contract::enable_deterministic_zk();

    let mut chain = HeavyweightPipeline::new().await?;
    chain.init_genesis().await?;

    let cid = *NATIVE_TOKEN_CONTRACT_ID;
    let native_harness = NativeTokenHarness::spawn();

    // ── Block 2: Coinbase-only (creates spendable coin) ──
    let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
    chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

    // ── Build FeeV3 with plaintext fee ──
    let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
    let fee_height = chain.height().succ();
    let gen_reward = expected_reward(BlockHeight::new(1));
    let gen_cb = chain.build_coinbase_for_height(BlockHeight::new(1), gen_reward).await?;
    let mut tree = MerkleTree::new(1);
    tree.append(MerkleNode::from_base(pallas::Base::zero()));
    tree.append(MerkleNode::from_base(gen_cb.commitment.inner()));
    tree.append(MerkleNode::from_base(cb2.commitment.inner()));
    let coin_pos = tree.mark().expect("tree.mark");
    let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
    let root = tree.root(0).expect("tree.root");

    let mining_kp = chain.mining_keypair(BlockHeight::new(2));
    let fee_amount: u64 = 150_000_000; // above premium threshold
    let fee_result = native_harness.fee_v2(
        cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
        cb2.commitment_blind, u64::from(coin_pos), path, root,
        mining_kp.secret.clone(), mining_kp.secret.clone(),
        PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
        pallas::Base::zero(), pallas::Base::zero(),
        fee_amount,
    ).map_err(|e| dwow_core::Error::Custom(format!(
        "[IT-2-ST1] FeeV3 harness: {}", e
    )))?;

    // ── Admit to mempool via plaintext fee ──
    let chain_tx = dwow_chain::Transaction {
        version: dwow_sdk::blockchain::BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![dwow_chain::ContractCall {
            contract_id: cid,
            data: fee_result.call_data.clone(),
        }],
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    };

    let extractor = NativeTokenFeeSignallingExtractor::new();
    let mp = dwow_mempool::create_mempool(
        Box::new(extractor),
        Some(chain.chain_state.clone()),
    );
    // Set all tier prices to 1 so the plaintext fee (150M) admits to the high queue.
    mp.update_tier_prices(FeeAmount::new(1), FeeAmount::new(1), FeeAmount::new(1));

    // FI-ADMIT-1: high tier (fee >= price_high of 1)
    let _tx_hash = mp.add(chain_tx.clone()).await
        .map_err(|e| dwow_core::Error::Custom(format!("[IT-2-ST2] mempool add: {}", e)))?;
    assert_eq!(mp.high_queue_len(), 1,
        "[IT-2-ST3] high queue must have 1 tx (fee {} >= price_high 1)",
        fee_amount);

    // ── FI-ADMIT-2: FCFS within tier ──
    let miner_cfg = dwow_mempool::MinerConfig::default();
    let selected = mp.select_for_block(&miner_cfg).await;
    assert_eq!(selected.len(), 1, "[IT-2-ST4] select_for_block must return 1 tx");
    assert_eq!(mp.high_queue_len(), 0, "[IT-2-ST5] high queue empty after drain");

    // ── Submit block: coinbase + selected FeeV3 + FeeCollectV1 ──
    let before = chain.height();
    let new_height = chain.block()?
        .with_call(cid, &native_harness, &selected[0].contract_calls[0].data, fee_result.proofs.clone())?
        .add_fee(FeeAmount::new(fee_amount))
        .with_fee_collect()?
        .submit_with_coinbase(cb3.coinbase_tx.clone()).await?;
    assert!(new_height > before,
        "[IT-2-ST6] height advanced: {} -> {}", before, new_height);

    // ── Supply neutrality ──
    let supply = chain.cumulative_supply();
    let expected: u64 = (1..=3u64).map(|h| expected_reward(BlockHeight::new(h)).get()).sum();
    assert_eq!(supply, expected,
        "[IT-2-ST8] supply unchanged: {} == {}", supply, expected);

    chain.log("[IT-2] Mempool lifecycle test PASSED");
    Ok(())
}
