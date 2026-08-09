//! Mempool fee policy tests — separate from consensus (fee-spec.md §7).
//!
//! Fee minimums, threshold proofs, and transaction prioritization are mempool
//! concerns, not consensus rules. These tests verify:
//! 1. Premium/general queue FIFO ordering
//! 2. Two-tier admission with REJECT path for below-general
//! 3. FeeV1 transactions through legacy fee_index

use std::sync::Mutex;

use dwow_mempool::{FeeCommitment, FeeSignallingExtractor, Mempool, MempoolConfig, MinerConfig};
use dwow_sdk::blockchain::{BlockCharge, FeeAmount};
use dwow_sdk::pasta::group::{Group, GroupEncoding};

struct TestFeeSignallingExtractor;
impl FeeSignallingExtractor for TestFeeSignallingExtractor {
    fn extract_fee(&self, tx: &dwow_chain::Transaction) -> FeeAmount {
        if let Some(call) = tx.contract_calls.first() {
            if call.data.len() >= 9 {
                if call.data[0] == 0x08 {
                    return FeeAmount::new(
                        u64::from_le_bytes(call.data[1..9].try_into().unwrap_or([0; 8]))
                    );
                }
            }
        }
        FeeAmount::ZERO
    }
    fn declare_charge(&self, tx: &dwow_chain::Transaction) -> BlockCharge {
        BlockCharge::new(tx.contract_calls.len() as u64 * 400_000_000)
    }
    fn extract_fee_commitment(&self, _tx: &dwow_chain::Transaction) -> Option<FeeCommitment> {
        None
    }
    fn verify_threshold_proof(&self, tx: &dwow_chain::Transaction, threshold: FeeAmount) -> bool {
        self.extract_fee(tx) >= threshold
    }
}

/// Make a FeeV2 test transaction (selector 0x08) with a given fee amount.
/// The test extractor simulates threshold proof verification by comparing
/// the fee against the threshold directly.
fn make_fee_v2_tx(fee: u64) -> dwow_chain::Transaction {
    let mut data = vec![0x08u8];
    // Minimal FeeParamsV2 payload — test extractor reads fee from the
    // first 8 bytes after selector (same layout as V1 for testing).
    data.extend_from_slice(&fee.to_le_bytes());
    dwow_chain::Transaction {
        version: dwow_sdk::blockchain::BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![dwow_chain::ContractCall {
            contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
            data,
        }],
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    }
}

#[test]
fn test_mempool_queues_initialized() {
    smol::block_on(async {
        let config = MempoolConfig::default();
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        let selected = mempool.select_for_block(&MinerConfig {
            max_charge: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert!(selected.is_empty(), "empty mempool must return empty selection");
    })
}

#[test]
fn test_mempool_add_single_tx() {
    smol::block_on(async {
        let config = MempoolConfig::default();
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        let tx = make_fee_v2_tx(50_000_000);
        let hash = mempool.add(tx).await.expect("add tx");
        assert!(!hash.as_bytes().iter().all(|b| *b == 0), "tx hash must be non-zero");

        let selected = mempool.select_for_block(&MinerConfig {
            max_charge: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert_eq!(selected.len(), 1, "single tx must be selected");
    })
}

#[test]
fn test_mempool_accepts_zero_fee() {
    // Consensus accepts any fee level. Mempool may accept zero-fee txs
    // when min_fee=0 — rejection is policy, not consensus (fee-spec.md §7).
    smol::block_on(async {
        let config = MempoolConfig { min_fee: FeeAmount::ZERO, ..Default::default() };
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        let tx = make_fee_v2_tx(0);
        let result = mempool.add(tx).await;
        assert!(result.is_ok(), "zero-fee tx must be accepted when min_fee=0, got {:?}", result.err());
    })
}

#[test]
fn test_feev2_premium_admission() {
    // FeeV2 tx with fee >= premium_threshold goes to premium queue.
    smol::block_on(async {
        let config = MempoolConfig {
            premium_threshold: FeeAmount::new(100_000_000),
            general_threshold: FeeAmount::new(10_000_000),
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        let tx = make_fee_v2_tx(150_000_000); // above premium
        let result = mempool.add(tx).await;
        assert!(result.is_ok(), "premium fee tx must be accepted, got {:?}", result.err());
    })
}

#[test]
fn test_feev2_general_admission() {
    // FeeV2 tx with fee between general and premium goes to general queue.
    smol::block_on(async {
        let config = MempoolConfig {
            premium_threshold: FeeAmount::new(200_000_000),
            general_threshold: FeeAmount::new(50_000_000),
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        let tx = make_fee_v2_tx(100_000_000); // above general, below premium
        let result = mempool.add(tx).await;
        assert!(result.is_ok(), "general fee tx must be accepted, got {:?}", result.err());
    })
}

#[test]
fn test_feev2_reject_below_general() {
    // FeeV2 tx with fee below general_threshold is REJECTED.
    smol::block_on(async {
        let config = MempoolConfig {
            premium_threshold: FeeAmount::new(100_000_000),
            general_threshold: FeeAmount::new(50_000_000),
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        let tx = make_fee_v2_tx(10_000_000); // below general
        let result = mempool.add(tx).await;
        assert!(result.is_err(), "below-general tx must be rejected");
    })
}

#[test]
fn test_feev2_premium_before_general() {
    // Premium queue drained before general queue in select_for_block.
    smol::block_on(async {
        let config = MempoolConfig {
            premium_threshold: FeeAmount::new(100_000_000),
            general_threshold: FeeAmount::new(10_000_000),
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        // Add general-tier tx first, then premium-tier
        let tx_general = make_fee_v2_tx(50_000_000);
        let tx_premium = make_fee_v2_tx(200_000_000);
        mempool.add(tx_general).await.expect("add general");
        mempool.add(tx_premium).await.expect("add premium");

        let selected = mempool.select_for_block(&MinerConfig {
            max_charge: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert_eq!(selected.len(), 2, "both txs must be selected");
        // Premium tx must be first regardless of insertion order
        let first_data = &selected[0].contract_calls[0].data;
        let first_fee = u64::from_le_bytes(first_data[1..9].try_into().unwrap());
        assert_eq!(first_fee, 200_000_000, "premium tx must be first (got fee={})", first_fee);
    })
}

// ============================================================================
// Level 1.5: Mempool → accept_block integration.
// Tests the full path: FeeV2 tx admitted to mempool via threshold proof,
// selected for block inclusion, accepted through accept_block, state verified.
// Spec: mempool.md §5, fee-spec.md §5.6.
// ============================================================================

#[test]
fn test_mempool_feev2_through_accept_block() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;

    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ---- Build a real FeeV2 transaction via harness ----
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file("mempool_feev2_15")));

        let native_harness = NativeTokenHarness::spawn();
        let cid = *NATIVE_TOKEN_CONTRACT_ID;

        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_amount: u64 = 150_000_000; // above premium threshold
        let fee_result = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret,
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            fee_amount,
            42_000_000,  // premium threshold
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [mempool_1.5::FeeV2]: {}", e
        )))?;

        // ---- Admit to mempool via threshold proof ----
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

        let config = MempoolConfig {
            premium_threshold: FeeAmount::new(42_000_000),
            general_threshold: FeeAmount::new(1_000_000),
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeSignallingExtractor), None);

        // Admission: the tx carries a FeeThreshold_V1 proof for premium threshold
        let tx_hash = mempool.add(chain_tx.clone()).await
            .expect("TEST-FAIL [mempool_1.5]: FeeV2 tx must be admitted to mempool");

        // Selection: tx must be selected for block inclusion
        let selected = mempool.select_for_block(&MinerConfig {
            max_charge: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert!(!selected.is_empty(),
            "TEST-FAIL [mempool_1.5]: FeeV2 tx must be selected for block");
        // Verify the selected tx matches what was admitted (mempool→selection integrity)
        assert_eq!(selected[0].contract_calls[0].data, fee_result.call_data,
            "TEST-FAIL [mempool_1.5]: selected tx call_data must match admitted tx");

        // ---- Submit through accept_block using mempool-selected transaction ----
        // The selected tx's call_data is identical to harness call_data. We submit
        // with harness proofs because the mempool TestFeeSignallingExtractor uses raw u64
        // comparison (not real ZK proof verification) — the real proof verification
        // happens in accept_block via verify_core_tx_with_tables.
        let before = chain.height();
        let new_height = chain.block()?
            .with_call(cid, &native_harness, &selected[0].contract_calls[0].data, fee_result.proofs)?
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;
        assert!(new_height > before,
            "TEST-FAIL [mempool_1.5]: height must advance (was {}, now {})", before, new_height);

        // ---- State verification ----
        // Accumulator reset to Identity after FeeCollectV1
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("TEST-FAIL [mempool_1.5]: accumulator not found");
        let acc_point: dwow_sdk::pasta::pallas::Point =
            Option::from(dwow_sdk::pasta::pallas::Point::from_bytes(
                &acc_data[..32].try_into().unwrap()
            )).expect("invalid accumulator point");
        assert_eq!(acc_point, dwow_sdk::pasta::pallas::Point::identity(),
            "TEST-FAIL [mempool_1.5]: accumulator not reset after FeeCollectV1");

        // ---- Fee window flags propagation (Scenario 1) ----
        // Verify fee_window_flags in the block header are active and well-formed.
        {
            let stored = chain.chain_state.store.get_block(new_height)?;
            let flags = stored.header.fee_window_flags;
            assert!(flags.is_active(),
                "L1.5-FW-1: fee_window_flags must be active after fee-bearing block, got 0x{:04x}", flags.get());
            let circuit_cm = flags.circuit_byte().congestion_multiplier();
            let wasm_cm = flags.wasm_byte().congestion_multiplier();
            assert!(circuit_cm <= 2,
                "L1.5-FW-1: circuit CM ({}) must be valid (0-2)", circuit_cm);
            assert!(wasm_cm <= 2,
                "L1.5-FW-1: wasm CM ({}) must be valid (0-2)", wasm_cm);
            // derive_cfs: wallet-side CF estimation from flags
            let (circuit_cf, wasm_cf) = flags.derive_cfs();
            assert!(circuit_cf.premium() >= dwow_chain::fee_window::CongestionFactor::SCALE,
                "L1.5-FW-1: derived circuit CF ({}) must be >= SCALE", circuit_cf.premium());
            assert!(wasm_cf.premium() >= dwow_chain::fee_window::CongestionFactor::SCALE,
                "L1.5-FW-1: derived wasm CF ({}) must be >= SCALE", wasm_cf.premium());
        }

        // ---- Rejection: tx below general_threshold must be rejected ----
        let below_tx = make_fee_v2_tx(500_000); // below general_threshold (1_000_000)
        let result = mempool.add(below_tx).await;
        assert!(result.is_err(),
            "TEST-FAIL [mempool_1.5]: below-threshold tx must be rejected, got {:?}", result);

        Ok(())
    })
}

// ============================================================================
// L1.5-FW-2: Real extractor inside real mempool → accept_block.
// Uses the REAL NativeTokenFeeSignallingExtractor (FeeParamsV2 decode + threshold
// check) instead of the TestFeeSignallingExtractor's u64 comparison.
// Verifies: real FeeV2 tx admitted to premium queue, selected for block,
// accept_block advances height, accumulator resets to Identity.
// ============================================================================

#[test]
fn test_real_extractor_mempool_accept_block() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;
    use crate::NativeTokenFeeSignallingExtractor;

    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(std::sync::Mutex::new(crate::tests::test_output::create_log_file("mempool_real_extractor_15")));

        let native_harness = NativeTokenHarness::spawn();
        let cid = *NATIVE_TOKEN_CONTRACT_ID;

        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;

        let cb3 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let fee_amount: u64 = 150_000_000; // above premium threshold
        let fee_result = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret,
            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?),
            pallas::Base::zero(), pallas::Base::zero(),
            fee_amount,
            42_000_000,  // premium threshold
        ).map_err(|e| dwow_core::Error::Custom(format!(
            "[L1.5-FW-2] fee_v2 harness: {}", e
        )))?;

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

        // Real extractor — parses FeeParamsV2, checks threshold, extracts commitment.
        let config = MempoolConfig {
            premium_threshold: FeeAmount::new(42_000_000),
            general_threshold: FeeAmount::new(1_000_000),
            ..Default::default()
        };
        let mempool = Mempool::new(
            config, None,
            Box::new(NativeTokenFeeSignallingExtractor::new()),
            None,
        );

        // Admission via real extractor
        let tx_hash = mempool.add(chain_tx.clone()).await
            .expect("[L1.5-FW-2] real extractor: FeeV2 tx must be admitted to mempool");

        // Selection
        let selected = mempool.select_for_block(&MinerConfig {
            max_charge: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert!(!selected.is_empty(),
            "[L1.5-FW-2] real extractor: FeeV2 tx must be selected for block");

        // Submit through accept_block
        let before = chain.height();
        let new_height = chain.block()?
            .with_call(cid, &native_harness, &selected[0].contract_calls[0].data, fee_result.proofs)?
            .with_fee_collect()?
            .submit_with_coinbase(cb3.coinbase_tx).await?;
        assert!(new_height > before,
            "[L1.5-FW-2] height must advance (was {}, now {})", before, new_height);

        // Accumulator reset to Identity after FeeCollectV1
        let acc_data = chain.query_contract_state(cid, "info", b"fee_commit_acc")?
            .expect("[L1.5-FW-2] accumulator not found");
        let acc_point: pallas::Point =
            Option::from(pallas::Point::from_bytes(
                &acc_data[..32].try_into().unwrap()
            )).expect("invalid accumulator point");
        assert_eq!(acc_point, pallas::Point::identity(),
            "[L1.5-FW-2] accumulator not reset after FeeCollectV1");

        // ---- Fee window flags propagation (Scenario 1) ----
        {
            let stored = chain.chain_state.store.get_block(new_height)?;
            let flags = stored.header.fee_window_flags;
            assert!(flags.is_active(),
                "[L1.5-FW-2] fee_window_flags must be active after fee-bearing block, got 0x{:04x}", flags.get());
            let circuit_cm = flags.circuit_byte().congestion_multiplier();
            let wasm_cm = flags.wasm_byte().congestion_multiplier();
            assert!(circuit_cm <= 2,
                "[L1.5-FW-2] circuit CM ({}) must be valid (0-2)", circuit_cm);
            assert!(wasm_cm <= 2,
                "[L1.5-FW-2] wasm CM ({}) must be valid (0-2)", wasm_cm);
            let (circuit_cf, wasm_cf) = flags.derive_cfs();
            assert!(circuit_cf.premium() >= dwow_chain::fee_window::CongestionFactor::SCALE,
                "[L1.5-FW-2] derived circuit CF ({}) must be >= SCALE", circuit_cf.premium());
            assert!(wasm_cf.premium() >= dwow_chain::fee_window::CongestionFactor::SCALE,
                "[L1.5-FW-2] derived wasm CF ({}) must be >= SCALE", wasm_cf.premium());
            // G1: fee_window_flags must be NON-DEFAULT after a fee-bearing block.
            // If flags == default(), fee signalling never activated — wallet would
            // read identity CF and compute fees at zero congestion regardless of
            // actual network state.
            assert_ne!(flags, FeeWindowFlags::default(),
                "[L1.5-FW-2] G1: fee_window_flags must be non-default after fee tx. \
                 Got default — fee signalling did not activate. Wallet would underpay under congestion.");
        }

        Ok(())
    })
}

// ============================================================================
// NF-1 WYSIWYG: Nullifier replay rejected at mempool admission (L1.5-FW-3).
//
// Two DIFFERENT FeeV2 transactions (different hashes) spending the SAME coin
// produce the SAME nullifier. First tx admitted. Second tx REJECTED with an
// error specifically citing "nullifier" — proving the in-mempool nullifier
// dedup barrier (B2, line 383) is exercised, NOT the duplicate-hash check
// or fee-below-threshold gate.
//
// WYSIWYG: Every assertion has a unique tag. State is logged before each check.
// ============================================================================

#[test]
fn test_nullifier_replay_rejected_at_mempool() -> std::result::Result<(), Box<dyn std::error::Error>> {
    use dwow_contract_test_harness::harness::NativeTokenHarness;
    use dwow_sdk::blockchain::BlockHeight;
    use dwow_sdk::crypto::{MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID, Nullifier, PublicKey, SecretKey};
    use dwow_sdk::pasta::pallas;
    use crate::tests::blockchain::HeavyweightPipeline;
    use crate::tests::modules::coinbase_coordination;
    use crate::NativeTokenFeeSignallingExtractor;

    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        let mut chain = HeavyweightPipeline::new().await?;
        chain.init_genesis().await?;
        chain.log_file = Some(std::sync::Mutex::new(
            crate::tests::test_output::create_log_file("wysiwyg_nf1")
        ));
        let log = |msg: &str| chain.log(msg);

        let native_harness = NativeTokenHarness::spawn();
        let cid = *NATIVE_TOKEN_CONTRACT_ID;

        // ── STEP 0: Prerequisites ──────────────────────────────────────
        log("[NF1-ST0] Verifying prerequisites");
        let h = chain.height();
        assert_eq!(h, BlockHeight::new(1),
            "[NF1-ST0-1] Chain must be at genesis height 1, was {}", h);

        // ── STEP 1: Produce a spendable coin at height 2 ───────────────
        log("[NF1-ST1] Mining coinbase at height 2");
        let cb2 = coinbase_coordination::prefetch_coinbase_params(&chain).await?;
        chain.block()?.submit_with_coinbase(cb2.coinbase_tx).await?;
        log(&format!("[NF1-ST1-1] Height 2 mined, coin_value={}", cb2.coin_value));

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(cb2.coin_commitment.inner()));
        let coin_pos = tree.mark().expect("tree.mark");
        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
        let root = tree.root(0).expect("tree.root");

        // ── STEP 2: Build two txs with same coin → same nullifier ──────
        log("[NF1-ST2] Building two FeeV2 transactions from same coin");
        let mining_kp = chain.mining_keypair(BlockHeight::new(2));
        let nf = Nullifier::new(mining_kp.secret.clone(), cb2.coin_commitment.inner());
        assert!(!nf.is_zero(), "[NF1-ST2-1] Nullifier must be non-zero");
        log(&format!("[NF1-ST2-1] Nullifier computed (non-zero)"));
        let fee_dest = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32])?);
        let threshold: u64 = 42_000_000;
        let general: u64 = 1_000_000;

        // Tx1: fee=150M
        let fr1 = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path.clone(), root,
            mining_kp.secret.clone(), mining_kp.secret.clone(),
            fee_dest, pallas::Base::zero(), pallas::Base::zero(),
            150_000_000, threshold,
        ).map_err(|e| dwow_core::Error::Custom(format!("[NF1-ST2] fee_v2 tx1: {}", e)))?;

        // Tx2: fee=200M, SAME coin → SAME nullifier
        let fr2 = native_harness.fee_v2(
            cb2.coin_value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            cb2.coin_blind, u64::from(coin_pos), path, root,
            mining_kp.secret.clone(), mining_kp.secret,
            fee_dest, pallas::Base::zero(), pallas::Base::zero(),
            200_000_000, threshold,
        ).map_err(|e| dwow_core::Error::Custom(format!("[NF1-ST2] fee_v2 tx2: {}", e)))?;

        let tx1 = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: cid, data: fr1.call_data.clone(),
            }],
            lock_time: 0, nullifiers: vec![nf], witness: vec![],
        };
        let tx2 = dwow_chain::Transaction {
            version: dwow_sdk::blockchain::BlockVersion::CURRENT,
            inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: cid, data: fr2.call_data.clone(),
            }],
            lock_time: 0, nullifiers: vec![nf], witness: vec![],
        };

        // Critical: hashes MUST differ, nullifiers MUST be identical
        assert_ne!(tx1.hash(), tx2.hash(),
            "[NF1-ST2-2] Tx hashes must differ (fee 150M vs 200M). Same hash = false positive.");
        assert_eq!(tx1.nullifiers, tx2.nullifiers,
            "[NF1-ST2-3] Both txs must have identical nullifiers (they spend same coin)");
        log("[NF1-ST2] Two txs built: different hashes, same nullifier");

        // ── STEP 3: Admit first tx ─────────────────────────────────────
        log("[NF1-ST3] Configuring mempool and admitting first tx");
        let mempool = Mempool::new(
            MempoolConfig {
                premium_threshold: FeeAmount::new(threshold),
                general_threshold: FeeAmount::new(general),
                ..Default::default()
            }, None,
            Box::new(NativeTokenFeeSignallingExtractor::new()),
            None,
        );

        mempool.add(tx1).await
            .map_err(|e| {
                log(&format!("[NF1-ST3-1] FAILED to admit first tx: {:?} (fee=150M, premium={}, general={})",
                    e, threshold, general));
                e
            })
            .expect("[NF1-ST3-1] First tx (150M fee) must be admitted to mempool");
        log("[NF1-ST3] First tx admitted");

        // ── STEP 4: Verify mempool state ───────────────────────────────
        log("[NF1-ST4] Verifying mempool state");
        assert_eq!(mempool.premium_queue_len(), 1,
            "[NF1-ST4-1] Premium queue must have 1 tx (fee 150M >= premium 42M)");
        assert_eq!(mempool.standard_queue_len(), 0,
            "[NF1-ST4-2] Standard queue must be empty (tx went to premium, not general)");

        // ── STEP 5: Nullifier replay rejection ─────────────────────────
        log("[NF1-ST5] Attempting nullifier replay with second tx");
        let r2 = mempool.add(tx2).await;
        assert!(r2.is_err(),
            "[NF1-ST5-1] Nullifier replay MUST be rejected. Second tx was admitted \
             despite sharing nullifier with first tx. Barrier B2 (line 383) has failed.");
        let err = format!("{:?}", r2.err());
        log(&format!("[NF1-ST5-2] Rejection: {}", err));
        assert!(err.contains("nullifier"),
            "[NF1-ST5-2] Error must cite 'nullifier'. Got: '{}'. \
             If 'already in mempool': duplicate-hash fired (false positive).", err);
        // The nullifier dedup error is "Double-spend: nullifier already in mempool"
        // which contains "nullifier" (proof of line 384, not line 375 hash dedup).
        // "Transaction already in mempool" (without "nullifier") = line 375 false positive.
        assert!(!err.contains("Transaction already in mempool"),
            "[NF1-ST5-3] Error must be 'Double-spend: nullifier already in mempool' \
             (nullifier dedup, line 384), not 'Transaction already in mempool' \
             (hash dedup, line 375). Got: {}", err);
        assert!(!err.contains("fee below"),
            "[NF1-ST5-4] Error must NOT cite 'fee below' (FeeV2 gate, not nullifier).");

        log("[NF1] PASSED: nullifier replay correctly rejected");
        Ok(())
    })
}
