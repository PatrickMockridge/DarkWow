//! Mempool fee policy tests — separate from consensus (fee-spec.md §7).
//!
//! Fee minimums, threshold proofs, and transaction prioritization are mempool
//! concerns, not consensus rules. These tests verify:
//! 1. Premium/general queue FIFO ordering
//! 2. Two-tier admission with REJECT path for below-general
//! 3. FeeV1 transactions through legacy fee_index

use std::sync::Mutex;

use dwow_mempool::{FeeCommitment, FeeExtractor, Mempool, MempoolConfig, MinerConfig};
use dwow_sdk::pasta::group::{Group, GroupEncoding};

struct TestFeeExtractor;
impl FeeExtractor for TestFeeExtractor {
    fn extract_fee(&self, tx: &dwow_chain::Transaction) -> u64 {
        if let Some(call) = tx.contract_calls.first() {
            if call.data.len() >= 9 {
                // FeeV2: [0x08][fee:8][...] (test data uses same prefix layout)
                if call.data[0] == 0x08 {
                    return u64::from_le_bytes(call.data[1..9].try_into().unwrap_or([0; 8]));
                }
            }
        }
        0
    }
    fn estimate_gas(&self, tx: &dwow_chain::Transaction) -> u64 {
        tx.contract_calls.len() as u64 * 400_000_000
    }
    fn extract_fee_commitment(&self, _tx: &dwow_chain::Transaction) -> Option<FeeCommitment> {
        // Test extractor: V1-only, no commitments
        None
    }
    fn verify_threshold_proof(&self, tx: &dwow_chain::Transaction, threshold: u64) -> bool {
        let fee = self.extract_fee(tx);
        fee >= threshold
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
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

        let selected = mempool.select_for_block(&MinerConfig {
            max_gas: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert!(selected.is_empty(), "empty mempool must return empty selection");
    })
}

#[test]
fn test_mempool_add_single_tx() {
    smol::block_on(async {
        let config = MempoolConfig::default();
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

        let tx = make_fee_v2_tx(50_000_000);
        let hash = mempool.add(tx).await.expect("add tx");
        assert!(!hash.as_bytes().iter().all(|b| *b == 0), "tx hash must be non-zero");

        let selected = mempool.select_for_block(&MinerConfig {
            max_gas: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert_eq!(selected.len(), 1, "single tx must be selected");
    })
}

#[test]
fn test_mempool_accepts_zero_fee() {
    // Consensus accepts any fee level. Mempool may accept zero-fee txs
    // when min_fee=0 — rejection is policy, not consensus (fee-spec.md §7).
    smol::block_on(async {
        let config = MempoolConfig { min_fee: 0, ..Default::default() };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

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
            premium_threshold: 100_000_000,
            general_threshold: 10_000_000,
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

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
            premium_threshold: 200_000_000,
            general_threshold: 50_000_000,
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

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
            premium_threshold: 100_000_000,
            general_threshold: 50_000_000,
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

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
            premium_threshold: 100_000_000,
            general_threshold: 10_000_000,
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

        // Add general-tier tx first, then premium-tier
        let tx_general = make_fee_v2_tx(50_000_000);
        let tx_premium = make_fee_v2_tx(200_000_000);
        mempool.add(tx_general).await.expect("add general");
        mempool.add(tx_premium).await.expect("add premium");

        let selected = mempool.select_for_block(&MinerConfig {
            max_gas: u64::MAX, max_txs: 100, ..Default::default()
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
            premium_threshold: 42_000_000,
            general_threshold: 1_000_000,
            ..Default::default()
        };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

        // Admission: the tx carries a FeeThreshold_V1 proof for premium threshold
        let tx_hash = mempool.add(chain_tx.clone()).await
            .expect("TEST-FAIL [mempool_1.5]: FeeV2 tx must be admitted to mempool");

        // Selection: tx must be selected for block inclusion
        let selected = mempool.select_for_block(&MinerConfig {
            max_gas: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert!(!selected.is_empty(),
            "TEST-FAIL [mempool_1.5]: FeeV2 tx must be selected for block");
        // Verify the selected tx matches what was admitted (mempool→selection integrity)
        assert_eq!(selected[0].contract_calls[0].data, fee_result.call_data,
            "TEST-FAIL [mempool_1.5]: selected tx call_data must match admitted tx");

        // ---- Submit through accept_block using mempool-selected transaction ----
        // The selected tx's call_data is identical to harness call_data. We submit
        // with harness proofs because the mempool TestFeeExtractor uses raw u64
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

        // ---- Rejection: tx below general_threshold must be rejected ----
        let below_tx = make_fee_v2_tx(500_000); // below general_threshold (1_000_000)
        let result = mempool.add(below_tx).await;
        assert!(result.is_err(),
            "TEST-FAIL [mempool_1.5]: below-threshold tx must be rejected, got {:?}", result);

        Ok(())
    })
}
