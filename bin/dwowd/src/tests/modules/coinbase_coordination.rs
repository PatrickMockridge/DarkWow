//! Coinbase parameter coordination for native_token FeeV1/BurnV1.
//!
//! Used by: Category 1 (native_token). Also available for future tests
//! that need coinbase parameters before constructing call_data.
//! Spec: heavyweight-spec.md §5.1 (native_token SPECIAL handling).

use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, FeeAmount};
use dwow_sdk::crypto::{ContractId, MerkleNode, MerkleTree, NATIVE_TOKEN_CONTRACT_ID};
use dwow_contract_test_harness::harness::ContractHarness;
use dwow_native_token_contract::{
    NATIVE_TOKEN_CONTRACT_COMMITMENT_MERKLE_TREE, NATIVE_TOKEN_CONTRACT_INFO_TREE,
};
use dwow_serial::Decodable;
use std::io::Cursor;

use crate::tests::blockchain::{HeavyweightPipeline, CoinbaseResult};

/// Pre-fetched coinbase parameters needed before constructing call_data
/// for functions that reference coinbase coin parameters.
pub struct PrefetchedCoinbase {
    pub commitment: dwow_chain::Commitment,
    pub nullifier: dwow_chain::Nullifier,
    pub commitment_blind: dwow_sdk::pasta::pallas::Base,
    pub coinbase_tx: dwow_chain::Transaction,
    pub coin_value: u64,
    /// The per-block derived mining secret sk_H (the coin's secret). The FeeV2
    /// circuit derives the input coin from this secret, so the harness MUST use
    /// the same sk_H that minted the coinbase coin — otherwise the input coin
    /// doesn't match the merkle tree leaf and the Fee_V2 proof fails.
    pub secret: dwow_sdk::crypto::SecretKey,
    /// Leaf position of the current coinbase coin in the on-chain coin merkle
    /// tree AFTER the coinbase tx (tx0) of this block has been applied. The
    /// tree accumulates every minted coin across blocks (coinbase + FeeV2 change
    /// + FeeCollect fee + transfer/spend outputs), so the position is read from
    /// the authoritative on-chain tree, never reconstructed from coinbase history.
    pub leaf_position: u64,
    /// Merkle path (siblings) for `leaf_position` in the same tree state.
    pub merkle_path: Vec<MerkleNode>,
    /// Merkle root of the same tree state (matches coin_roots_db).
    pub merkle_root: MerkleNode,
}

impl From<CoinbaseResult> for PrefetchedCoinbase {
    fn from(cb: CoinbaseResult) -> Self {
        let secret: dwow_sdk::crypto::SecretKey = cb.recipient.secret().clone().into();
        Self {
            commitment: cb.commitment,
            nullifier: cb.nullifier,
            commitment_blind: cb.commitment_blind,
            coinbase_tx: cb.tx,
            coin_value: cb.coin_value,
            secret,
            leaf_position: 0,
            merkle_path: Vec::new(),
            merkle_root: MerkleNode::from_base(dwow_sdk::pasta::pallas::Base::zero()),
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
    let mut pf = PrefetchedCoinbase::from(cb);

    // Read the authoritative on-chain coin merkle tree and append the current
    // block's coinbase coin. The tree accumulates every minted coin across
    // blocks (coinbase + FeeV2 change + FeeCollect fee + transfer/spend
    // outputs), so rebuilding it from coinbase history alone misses the
    // non-coinbase leaves and derives a wrong root/position (F1 root cause).
    // Reading the tree is WYSIWYG and deterministic — identical on chain A/B.
    //
    // On-chain format (runtime/import/merkle.rs): [u32 set_size][MerkleTree].
    let tree_bytes = chain.query_contract_state(
        *NATIVE_TOKEN_CONTRACT_ID,
        NATIVE_TOKEN_CONTRACT_INFO_TREE,
        NATIVE_TOKEN_CONTRACT_COMMITMENT_MERKLE_TREE,
    )?
    .ok_or_else(|| dwow_core::Error::Custom(
        "TEST-FAIL [coinbase_coordination]: coin_merkle_tree not found on-chain".into(),
    ))?;

    let mut cursor = Cursor::new(&tree_bytes);
    let _set_size: u32 = Decodable::decode(&mut cursor)
        .map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [coinbase_coordination]: decode tree set_size: {}", e
        )))?;
    let mut tree: MerkleTree = Decodable::decode(&mut cursor)
        .map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [coinbase_coordination]: decode coin MerkleTree: {}", e
        )))?;

    // Append the current block's coinbase coin (minted by tx0 PoWRewardV1).
    tree.append(MerkleNode::from_base(pf.commitment.inner()));
    let coin_pos = tree.mark().ok_or_else(|| dwow_core::Error::Custom(
        "TEST-FAIL [coinbase_coordination]: tree.mark failed".into(),
    ))?;
    pf.merkle_path = tree.witness(coin_pos, 0)
        .map_err(|e| dwow_core::Error::Custom(format!(
            "TEST-FAIL [coinbase_coordination]: tree.witness failed: {:?}", e
        )))?;
    pf.merkle_root = tree.root(0).ok_or_else(|| dwow_core::Error::Custom(
        "TEST-FAIL [coinbase_coordination]: tree.root failed".into(),
    ))?;
    pf.leaf_position = u64::from(coin_pos);

    Ok(pf)
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
        .add_fee(FeeAmount::new(1)) // FeeV2 fee_amount=1 (native_token_spec.rs); FeeCollectV1 C1 rejects zero-claim otherwise
        .with_fee_collect()?
        .submit_with_coinbase(coinbase_tx).await
}
