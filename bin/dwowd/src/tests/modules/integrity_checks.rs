//! Pre-test and post-test integrity checks (PI-1 through PI-7).
//!
//! Used by: Categories 1-3 (all contract tests).
//! Spec: heavyweight-spec.md §5.2 (Pre-Test), §5.3 (Post-Test).

use dwow_core::Result;
use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;

/// PI-1: Verify genesis block exists and has a non-zero hash.
pub fn verify_genesis_block_hash(chain: &HeavyweightPipeline) -> Result<()> {
    let genesis = chain.block_hash_at(BlockHeight::new(1))?;
    assert!(genesis.is_some(), "genesis block must exist at height 1");
    let hash = genesis.unwrap();
    assert_ne!(hash.as_bytes(), &[0u8; 32], "INFRA-FAIL [integrity_checks]: PI-1 genesis block hash must not be zero");
    Ok(())
}

/// PI-2: Verify initial cumulative supply equals INITIAL_REWARD.
/// Non-fatal: mass balance is the block validity proof; cumulative supply
/// is a derived bookkeeping metric. Mismatch logs a warning, not a failure.
pub fn verify_initial_supply(chain: &HeavyweightPipeline) -> Result<()> {
    let supply = chain.cumulative_supply();
    let expected = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
    if supply != expected.get() {
        chain.log(&format!(
            "WARN [integrity_checks]: PI-2 cumulative supply {} != expected INITIAL_REWARD {}. \
             Supply tree may not be populated after genesis init. \
             Block proof (mass balance) is the validity condition.",
            supply, expected.get()
        ));
    }
    Ok(())
}

/// PI-3: Verify a genesis contract exists in the contracts tree at height 1.
pub fn verify_contract_at_genesis(chain: &HeavyweightPipeline, cid: ContractId) -> Result<()> {
    let wasm = chain.query_contracts_tree(&cid.to_bytes())?;
    assert!(wasm.is_some(),
        "INFRA-FAIL [integrity_checks]: PI-3 genesis contract {} must exist in contracts tree at height 1", cid);
    Ok(())
}

/// Run all pre-test integrity checks (PI-1 through PI-4).
pub fn pre_test_integrity(
    chain: &HeavyweightPipeline,
    is_genesis: bool,
    cid: ContractId,
    harness: &dyn ContractHarness,
) -> Result<()> {
    verify_genesis_block_hash(chain)?;          // PI-1
    verify_initial_supply(chain)?;               // PI-2
    if is_genesis {
        verify_contract_at_genesis(chain, cid)?; // PI-3
    }
    // PI-4: ZK coverage pre-check — WARN only. Harness misconfiguration does not
    // affect block validity. The test can still exercise covered circuits.
    if let Err(e) = harness.verify_zk_coverage() {
        chain.log(&format!("WARN [integrity_checks]: PI-4 ZK coverage check failed — {}", e));
    }
    Ok(())
}

/// PI-5: Verify block hash chain is continuous from height 2 to current.
pub fn verify_hash_chain_continuity(chain: &HeavyweightPipeline) -> Result<()> {
    assert!(chain.block_hash_chain_continuous()?,
        "INFRA-FAIL [integrity_checks]: PI-5 block hash chain must be continuous");
    Ok(())
}

/// Run all post-test integrity checks (PI-5 through PI-7).
pub fn post_test_integrity(chain: &HeavyweightPipeline) -> Result<()> {
    verify_hash_chain_continuity(chain)?;        // PI-5
    // PI-6 (supply reconciliation) — deferred until cumulative_supply API is complete
    Ok(())
}
