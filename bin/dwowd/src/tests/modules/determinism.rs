//! Determinism verification — dual-pipeline replay + hash comparison.
//!
//! Used by: Categories 1-3 (all contract tests).
//! Spec: heavyweight-spec.md §3.7 (Deterministic Execution), PI-7.

use std::future::Future;

use dwow_core::Result;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;

/// Run determinism verification: create independent Pipeline B,
/// execute `replay_fn` identically on both pipelines, compare final hashes.
pub async fn verify_determinism<F, Fut>(
    chain_a: &HeavyweightPipeline,
    cid_a: ContractId,
    is_genesis: bool,
    static_cid: ContractId,
    harness: &dyn ContractHarness,
    name: &str,
    wasm_bytes: Option<&[u8]>,
    replay_fn: F,
) -> Result<()>
where
    F: Fn(&HeavyweightPipeline, ContractId) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    // Pipeline B: replay identical scenario on independent chain
    let chain_b = HeavyweightPipeline::new().await?;
    chain_b.init_genesis().await?;
    if let Err(e) = harness.verify_zk_coverage() {
        eprintln!("WARN [integrity_checks]: PI-4 ZK coverage check failed (determinism replay) — {}", e);
    }

    let cid_b = super::deploy_router::resolve_contract_id(
        &chain_b, is_genesis, static_cid, harness, name, wasm_bytes,
    ).await?;

    // Replay all endpoints
    replay_fn(&chain_b, cid_b).await?;

    // Compare final block hashes (PI-7)
    let hash_a = chain_a.block_hash_at(chain_a.height())?;
    let hash_b = chain_b.block_hash_at(chain_b.height())?;
    assert_eq!(hash_a, hash_b,
        "determinism failure: block hashes must match (PI-7)");

    Ok(())
}
