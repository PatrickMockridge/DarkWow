//! ContractId resolution: genesis-static vs WASM-deploy.
//!
//! Used by: Categories 1-3 (all contract tests, in both Pipeline A and B).
//! Spec: heavyweight-spec.md §5.1-5.8 (genesis), §5.9 (WASM).

use dwow_core::Result;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;

/// Resolve a contract's ID: static for genesis, deploy for WASM.
pub async fn resolve_contract_id(
    chain: &HeavyweightPipeline,
    is_genesis: bool,
    static_cid: ContractId,
    harness: &dyn ContractHarness,
    name: &str,
    wasm_bytes: Option<&[u8]>,
) -> Result<ContractId> {
    if is_genesis {
        Ok(static_cid)
    } else {
        chain.deploy(
            harness,
            name,
            wasm_bytes.expect("WASM contract must provide wasm_bytes"),
        ).await
    }
}
