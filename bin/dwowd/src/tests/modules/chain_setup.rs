//! Chain setup — create and initialize a HeavyweightPipeline.
//!
//! Used by: All 4 test categories (43 tests).
//! Spec: heavyweight-spec.md §9 (Per-Contract Test Template, Step 1).

use dwow_core::Result;
use crate::tests::blockchain::HeavyweightPipeline;

/// Create and initialize a fresh HeavyweightPipeline for testing.
/// Returns chain ready for deploy/block operations at height 1.
pub async fn init_test_chain() -> Result<HeavyweightPipeline> {
    let chain = HeavyweightPipeline::new().await?;
    chain.init_genesis().await?;
    Ok(chain)
}
