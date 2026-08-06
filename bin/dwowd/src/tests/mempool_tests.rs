//! Mempool fee policy tests — separate from consensus (fee-spec.md §7).
//!
//! Fee minimums, threshold proofs, and transaction prioritization are mempool
//! concerns, not consensus rules. These tests verify:
//! 1. Premium/general queue FIFO ordering
//! 2. Two-tier admission with REJECT path for below-general
//! 3. FeeV1 transactions through legacy fee_index

use dwow_mempool::{FeeCommitment, FeeExtractor, Mempool, MempoolConfig, MinerConfig};

struct TestFeeExtractor;
impl FeeExtractor for TestFeeExtractor {
    fn extract_fee(&self, tx: &dwow_chain::Transaction) -> u64 {
        if let Some(call) = tx.contract_calls.first() {
            if call.data.len() >= 9 {
                // FeeV1: [0x00][fee:8][...]
                // FeeV2: [0x08][fee:8][...] (test data uses same prefix layout)
                if call.data[0] == 0x00 || call.data[0] == 0x08 {
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

fn make_fee_tx(fee: u64) -> dwow_chain::Transaction {
    let mut data = vec![0x00u8];
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

/// Make a FeeV2-like transaction (selector 0x08) with a given fee amount.
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

        let tx = make_fee_tx(50_000_000);
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

        let tx = make_fee_tx(0);
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
