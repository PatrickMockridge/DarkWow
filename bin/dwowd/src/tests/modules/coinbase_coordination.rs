//! Coinbase parameter coordination for native_token FeeV1/BurnV1.
//!
//! Used by: Category 1 (native_token). Also available for future tests
//! that need coinbase parameters before constructing call_data.
//! Spec: heavyweight-spec.md §5.1 (native_token SPECIAL handling).

use dwow_core::Result;
use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::{HeavyweightPipeline, CoinbaseResult};

/// Pre-fetched coinbase parameters needed before constructing call_data
/// for functions that reference coinbase coin parameters.
pub struct PrefetchedCoinbase {
    pub coin_commitment: dwow_chain::CoinCommitment,
    pub nullifier: dwow_chain::Nullifier,
    pub coin_blind: dwow_sdk::pasta::pallas::Base,
    pub coinbase_tx: dwow_chain::Transaction,
    pub coin_value: u64,
}

impl From<CoinbaseResult> for PrefetchedCoinbase {
    fn from(cb: CoinbaseResult) -> Self {
        Self {
            coin_commitment: cb.coin_commitment,
            nullifier: cb.nullifier,
            coin_blind: cb.coin_blind,
            coinbase_tx: cb.tx,
            coin_value: cb.coin_value,
        }
    }
}

/// Build the coinbase first, returning parameters needed for FeeV1/BurnV1.
pub async fn prefetch_coinbase_params(
    chain: &HeavyweightPipeline,
) -> Result<PrefetchedCoinbase> {
    let height = chain.height().succ();
    let reward = dwow_sdk::blockchain::expected_reward(height);
    let cb = chain.build_coinbase_for_height(height, reward).await?;
    Ok(PrefetchedCoinbase::from(cb))
}

/// Submit a block with a pre-built coinbase (for FeeV1/BurnV1 after coordination).
pub async fn submit_with_coinbase(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<dwow_core::zk::Proof>,
    is_zk: bool,
    coinbase_tx: dwow_chain::Transaction,
) -> Result<BlockHeight> {
    if is_zk && proofs.is_empty() {
        return Err(dwow_core::Error::Custom(format!(
            "ZK-gated function on contract '{}' requires proofs (got 0)", harness.name()
        )));
    }

    chain.block()?
        .with_call(cid, harness, call_data, proofs)?
        .with_fee_collect()?
        .submit_with_coinbase(coinbase_tx).await
}
