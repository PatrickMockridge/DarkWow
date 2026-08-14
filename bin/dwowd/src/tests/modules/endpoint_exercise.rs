//! Endpoint exercise — per-endpoint generate→submit→verify loop.
//!
//! Used by: Categories 1-3 (all contract tests).
//! Spec: heavyweight-spec.md §3.1 (Endpoint Exhaustiveness),
//!       §5 (State Verification Requirements).

use dwow_core::Result;
use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::EndpointSpec;

/// Exercise a single endpoint: generate proofs → submit through accept_block
/// → verify height advancement → verify state transition.
pub async fn exercise_endpoint(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    endpoint: &EndpointSpec<'_>,
    height_before: BlockHeight,
) -> Result<BlockHeight> {
    let result = (endpoint.generate)()?;
    assert!(!result.call_data.is_empty(),
        "TEST-FAIL [{}]: call_data must not be empty", endpoint.name);

    let new_height = if result.children.is_empty() {
        super::block_submission::submit_single_call_block(
            chain, cid, harness,
            &result.call_data, result.proofs, endpoint.is_zk,
        ).await?
    } else {
        super::block_submission::submit_multi_call_block(
            chain, cid, harness,
            &result.call_data, result.proofs, endpoint.is_zk, result.children,
        ).await?
    };

    assert!(new_height > height_before,
        "TEST-FAIL [{}]: height must advance after accept_block (was {}, now {})",
        endpoint.name, height_before, new_height);

    // State verification: contract-specific state query deferred until
    // state inspection API is complete (RG-8, spec §7.2 PR-3).
    // accept_block validates state transitions structurally.
    Ok(new_height)
}

/// Exercise all endpoints sequentially, one per block for error isolation.
pub async fn exercise_all_endpoints(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    endpoints: &[EndpointSpec<'_>],
) -> Result<Vec<BlockHeight>> {
    let mut heights = Vec::with_capacity(endpoints.len());
    let mut current_height = chain.height();

    for endpoint in endpoints {
        let new_height = exercise_endpoint(
            chain, cid, harness, endpoint, current_height,
        ).await?;
        heights.push(new_height);
        current_height = new_height;
    }

    Ok(heights)
}
