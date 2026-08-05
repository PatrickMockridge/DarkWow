//! Nullifier replay rejection verification.
//!
//! Used by: Categories 1-3 (all contracts with ZK-gated functions).
//! Spec: heavyweight-spec.md §3.6 (Nullifier Replay Rejection).

use dwow_core::zk::Proof;
use dwow_core::Result;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;

/// Verify that resubmitting the same ZK-gated call is rejected.
/// First submission must have already succeeded (caller's responsibility).
/// Second submission with identical call_data+proofs MUST be rejected.
pub async fn verify_nullifier_replay(
    chain: &HeavyweightPipeline,
    cid: ContractId,
    harness: &dyn ContractHarness,
    call_data: &[u8],
    proofs: Vec<Proof>,
    is_zk: bool,
) -> Result<()> {
    // Second submission with same call_data MUST be rejected
    let replay_result = super::block_submission::submit_single_call_block(
        chain, cid, harness, call_data, proofs, is_zk,
    ).await;

    assert!(replay_result.is_err(),
        "nullifier replay MUST be rejected — second submission with identical call_data succeeded when it should have failed");

    Ok(())
}
