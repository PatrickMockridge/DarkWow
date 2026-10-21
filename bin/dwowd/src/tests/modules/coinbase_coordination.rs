//! Coinbase parameter coordination for native_token FeeV3/BurnV1.
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
    NATIVE_TOKEN_CONTRACT_LATEST_COMMITMENT_ROOT,
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
    /// The per-block derived mining secret sk_H (the coin's secret). The FeeV3
    /// circuit derives the input coin from this secret, so the harness MUST use
    /// the same sk_H that minted the coinbase coin — otherwise the input coin
    /// doesn't match the merkle tree leaf and the Fee_V3 proof fails.
    pub secret: dwow_sdk::crypto::SecretKey,
    /// Leaf position of the current coinbase coin in the on-chain coin merkle
    /// tree AFTER the coinbase tx (tx0) of this block has been applied. The
    /// tree accumulates every minted coin across blocks (coinbase + FeeV3 change
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

/// Build the coinbase first, returning parameters needed for FeeV3/BurnV1.
pub async fn prefetch_coinbase_params(
    chain: &HeavyweightPipeline,
) -> Result<PrefetchedCoinbase> {
    let height = chain.height().succ();
    let reward = dwow_sdk::blockchain::expected_reward(height);
    let cb = chain.build_coinbase_for_height(height, reward).await?;
    let mut pf = PrefetchedCoinbase::from(cb);

    // Read the authoritative on-chain coin merkle tree and append the current
    // block's coinbase coin. The tree accumulates every minted coin across
    // blocks (coinbase + FeeV3 change + FeeCollect fee + transfer/spend
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

/// Read the latest commitment (coin) root from the native token's `info` tree.
fn query_latest_coin_root(chain: &HeavyweightPipeline) -> Result<MerkleNode> {
    let bytes = chain
        .query_contract_state(
            *NATIVE_TOKEN_CONTRACT_ID,
            NATIVE_TOKEN_CONTRACT_INFO_TREE,
            NATIVE_TOKEN_CONTRACT_LATEST_COMMITMENT_ROOT,
        )?
        .ok_or_else(|| dwow_core::Error::Custom(
            "TEST-FAIL [coinbase_coordination]: latest coin root not found on-chain".into(),
        ))?;
    let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        dwow_core::Error::Custom(format!(
            "TEST-FAIL [coinbase_coordination]: latest coin root is {} bytes, not 32",
            bytes.len()
        ))
    })?;
    MerkleNode::from_bytes(arr).ok_or_else(|| dwow_core::Error::Custom(
        "TEST-FAIL [coinbase_coordination]: latest coin root is not a canonical field element".into(),
    ))
}

/// Spend a prefetched coinbase **one block after it is minted**.
///
/// The previous shape minted the prefetched coinbase and spent it in the same block. That is
/// not a valid trajectory: the coinbase transaction declares the coin's claim nullifier, the
/// claim nullifier *is* the coin's future spend nullifier, and OBL-C146 made the spending call
/// declare that same value — so the within-block replay gate refused the block as a duplicate
/// spend against the claim that created it. Relaxing that gate would let a miner spend a
/// coinbase in the block that created it, bypassing coinbase maturity
/// (`check_coinbase_maturity` cannot see an in-block claim). The fixture was wrong, not the
/// rule.
///
/// So this submits two blocks:
///   1. H+1: the prefetched coinbase and nothing else. Its apply appends the coin and stores
///      `pf.merkle_root`, which is the state the prefetched path/position were computed against.
///   2. H+2: the endpoint call (FeeV3/BurnV1) spending that coin, plus a fee collect and the
///      block's own fresh coinbase. At H+2 the coin is mature: `COINBASE_MATURITY` is 1 in test
///      builds and `H+2 - (H+1) = 1`.
pub async fn submit_with_coinbase(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<dwow_core::zk::Proof>,
    is_zk: bool,
    pf: &PrefetchedCoinbase,
) -> Result<BlockHeight> {
    if is_zk && proofs.is_empty() {
        return Err(dwow_core::Error::Custom(format!(
            "ZK-gated function on contract '{}' requires proofs (got 0)", harness.name()
        )));
    }

    // Block H+1: mint the coin. No contract calls, so the only leaf this block adds to the
    // coin tree is `pf.commitment` — which is what makes the prefetched root/path valid for
    // the spend at H+2, and what this assertion pins.
    chain.block()?.submit_with_coinbase(pf.coinbase_tx.clone()).await?;

    let latest = query_latest_coin_root(chain)?;
    if latest.to_bytes() != pf.merkle_root.to_bytes() {
        return Err(dwow_core::Error::Custom(format!(
            "TEST-FAIL [coinbase_coordination]: the coinbase-only block did not produce the \
             prefetched coin root (expected {}, got {})",
            hex::encode(pf.merkle_root.to_bytes()),
            hex::encode(latest.to_bytes()),
        )));
    }

    // Block H+2: spend the mature coin. `submit()` builds this block's own fresh coinbase.
    chain
        .block()?
        .with_call(cid, harness, call_data, proofs)?
        .add_fee(FeeAmount::new(1)) // FeeV3 fee_amount=1 (native_token_spec.rs); FeeCollectV1 C1 rejects zero-claim otherwise
        .with_fee_collect()?
        .submit()
        .await
}
