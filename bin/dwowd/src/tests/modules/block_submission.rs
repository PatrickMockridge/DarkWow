//! Uniform block submission with ZK gating + FeeCollectV1.
//!
//! Used by: Categories 1-3 (all contract tests).
//! Spec: heavyweight-spec.md §3.5 (FeeCollectV1 in Every Block),
//!       §7.2 PR-4, PR-6 (ZK gating in submit_block, not with_call).

use dwow_core::zk::Proof;
use dwow_core::Result;
use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::ChildCall;

/// Submit a single contract call in its own block with FeeCollectV1.
/// Enforces ZK gating: if `is_zk` is true, `proofs` must be non-empty.
/// `is_zk` comes from EndpointSpec::is_zk — authoritative metadata, never heuristic (RG-21).
///
/// FeeCollectV1 is appended conditionally via `with_fee_collect()`:
/// - When FeeV1 calls exist in the block (native_token tests) → FeeCollectV1 appended
/// - When no FeeV1 calls exist → FeeCollectV1 omitted (zero-fee block, matches miner)
/// Both cases are valid per consensus (validation.rs:376-387).
pub async fn submit_single_call_block(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<Proof>,
    is_zk: bool,
) -> Result<BlockHeight> {
    if is_zk && proofs.is_empty() {
        return Err(dwow_core::Error::Custom(format!(
            "TEST-FAIL [block_submission]: ZK-gated function on '{}' requires proofs (got 0)",
            harness.name()
        )));
    }

    chain.block()?
        .with_call(cid, harness, call_data, proofs)?
        .with_fee_collect()?
        .submit().await
}

/// Submit a parent contract call with bundled child calls in its own block with FeeCollectV1.
/// The children are listed before the parent (DFS post-order) in the transaction.
pub async fn submit_multi_call_block(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<Proof>,
    is_zk: bool,
    children: Vec<ChildCall>,
) -> Result<BlockHeight> {
    if is_zk && proofs.is_empty() {
        return Err(dwow_core::Error::Custom(format!(
            "TEST-FAIL [block_submission]: ZK-gated function on '{}' requires proofs (got 0)",
            harness.name()
        )));
    }

    let child_tuples: Vec<(ContractId, Vec<u8>, Vec<Proof>)> = children
        .into_iter()
        .map(|c| (c.contract_id, c.call_data, c.proofs))
        .collect();

    chain.block()?
        .with_call_tree(cid, call_data, proofs, child_tuples)?
        .with_fee_collect()?
        .submit().await
}
