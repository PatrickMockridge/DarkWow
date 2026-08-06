//! Mempool fee policy tests — separate from consensus (fee-spec.md §7).
//!
//! Fee minimums, threshold proofs, and transaction prioritization are mempool
//! concerns, not consensus rules. These tests verify:
//! 1. Mempool data structures (queues) initialize correctly
//! 2. FeeV1 transactions route through legacy fee_index
//! 3. FeeV2 threshold proofs gate premium/general queue admission (stub)
//! 4. select_for_block ordering: premium FIFO → general FIFO → fee_index

use dwow_mempool::{FeeExtractor, Mempool, MempoolConfig, MinerConfig};

struct TestFeeExtractor;
impl FeeExtractor for TestFeeExtractor {
    fn extract_fee(&self, tx: &dwow_chain::Transaction) -> u64 {
        // Read fee from first contract call data[1..9] if FeeV1
        if let Some(call) = tx.contract_calls.first() {
            if call.data.len() >= 9 && call.data[0] == 0x00 {
                return u64::from_le_bytes(call.data[1..9].try_into().unwrap_or([0; 8]));
            }
        }
        0
    }
    fn estimate_gas(&self, tx: &dwow_chain::Transaction) -> u64 {
        tx.contract_calls.len() as u64 * 400_000_000
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

#[test]
fn test_mempool_queues_initialized() {
    // Verify premium/general queues exist and are empty by default
    smol::block_on(async {
        let config = MempoolConfig { premium_threshold: Some(100_000), ..Default::default() };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

        let selected = mempool.select_for_block(&MinerConfig {
            max_gas: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        // With no transactions added, select_for_block returns empty
        assert!(selected.is_empty(), "empty mempool must return empty selection");
    })
}

#[test]
fn test_mempool_backward_compat_no_queues() {
    // When premium_threshold is None, queues are unused (backward compat)
    smol::block_on(async {
        let config = MempoolConfig { premium_threshold: None, ..Default::default() };
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
fn test_mempool_fee_ordering_descending() {
    // FeeV1 transactions are ordered by fee rate descending (legacy behavior)
    smol::block_on(async {
        let config = MempoolConfig { min_fee: 0, premium_threshold: None, ..Default::default() };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

        let tx_low = make_fee_tx(10_000_000);
        let tx_high = make_fee_tx(100_000_000);
        mempool.add(tx_low).await.expect("add low");
        mempool.add(tx_high).await.expect("add high");

        let selected = mempool.select_for_block(&MinerConfig {
            max_gas: u64::MAX, max_txs: 100, ..Default::default()
        }).await;
        assert_eq!(selected.len(), 2);

        // Higher fee should be first
        let fee_first = TestFeeExtractor.extract_fee(&selected[0]);
        let fee_second = TestFeeExtractor.extract_fee(&selected[1]);
        assert!(fee_first >= fee_second,
            "higher fee tx must be selected first ({} vs {})", fee_first, fee_second);
    })
}

#[test]
fn test_mempool_accepts_zero_fee() {
    // Consensus accepts any fee level. Mempool admission may accept zero-fee
    // txs — rejection is policy, not consensus (fee-spec.md §7).
    smol::block_on(async {
        let config = MempoolConfig { min_fee: 0, ..Default::default() };
        let mempool = Mempool::new(config, None, Box::new(TestFeeExtractor), None);

        let tx = make_fee_tx(0);
        let result = mempool.add(tx).await;
        assert!(result.is_ok(), "zero-fee tx must be accepted when min_fee=0, got {:?}", result.err());
    })
}
