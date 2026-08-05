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

/// Submit a single contract call in its own block with FeeCollectV1.
/// Enforces ZK gating: if `is_zk` is true, `proofs` must be non-empty.
/// `is_zk` comes from EndpointSpec::is_zk — authoritative metadata, never heuristic (RG-21).
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
            "ZK-gated function on contract '{}' requires proofs (got 0)",
            harness.name()
        )));
    }

    chain.block()?
        .with_call(cid, harness, call_data, proofs)?
        .with_fee_collect()?   // unconditional — RG-6, spec §3.5
        .submit().await
}
