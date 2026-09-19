//! Uncle block construction and shared helpers for block-execution tests.
//!
//! Used by: Category 4 (all 8 block-exec tests).
//! Spec: heavyweight-spec.md §8 (Block Execution Tests).

use dwow_contract_test_harness::harness::NativeTokenHarness;
use dwow_core::Result;
use dwow_core::zk::Proof;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, FeeAmount, FeeTier, RiskFactor};
use dwow_sdk::crypto::{ContractId, Keypair, SecretKey, PublicKey, NATIVE_TOKEN_CONTRACT_ID};
use dwow_sdk::pasta::pallas;

use dwow_native_token_contract::model::DRKW_ASSET_ID;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::modules::coinbase_coordination::PrefetchedCoinbase;

/// Build a RandomX VM for accept_block — used by block-exec tests.
pub fn build_accept_vm(
    block: &dwow_chain::Block,
) -> Result<std::sync::Arc<randomx::RandomXVM>> {
    let rx_flags = randomx::RandomXFlags::get_recommended_flags()
        & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &block.header.randomx_key)
        .map_err(|e| dwow_core::Error::Custom(format!("RandomX cache: {}", e)))?;
    Ok(std::sync::Arc::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
            .map_err(|e| dwow_core::Error::Custom(format!("RandomX VM: {}", e)))?,
    ))
}

/// Find a nonce that makes the block hash ≤ target.
pub fn mine_test_nonce(block: &dwow_chain::Block, vm: &randomx::RandomXVM, target: BlockTarget) -> dwow_core::Result<u32> {
    for nonce in 0u32..1_000_000 {
        let mut b = block.clone();
        b.header.nonce = nonce;
        let hash = b.hash_with_vm(vm).map_err(|e| {
            dwow_core::Error::Custom(format!("hashing candidate nonce {nonce}: {e}"))
        })?;
        // `as_bytes()` is `&[u8; 32]`, so these indices are statically in bounds:
        // no panic site, and therefore no embedded source location.
        let bytes = hash.as_bytes();
        let hash_u32 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if hash_u32 <= target.get() {
            return Ok(nonce);
        }
    }
    Err(dwow_core::Error::Custom(format!("Could not find valid nonce for target {} after 1M iterations", target)))
}

/// Build a single uncle block with one contract call.
pub fn build_uncle_with_call(
    chain: &HeavyweightPipeline,
    height: BlockHeight,
    reward: BlockReward,
    call_data: &[u8],
    depth: u8,
) -> Result<dwow_chain::UncleBlock> {
    let tx = crate::tests::harness::build_contract_tx(
        *NATIVE_TOKEN_CONTRACT_ID, call_data.to_vec(),
    );
    let block = crate::tests::harness::build_test_block(&chain.chain_state, height, vec![tx])
        .map_err(super::error_bridge::bridge)?;
    Ok(dwow_chain::create_uncle(block, depth, reward))
}

/// Create a HeavyweightPipeline with NativeTokenHarness and return
/// the chain, harness, ContractId, and a keypair for generating call_data.
/// Used by all 8 block-exec tests.
pub async fn setup_native_token_pipeline(
) -> std::result::Result<
    (HeavyweightPipeline, NativeTokenHarness, ContractId, Keypair),
    Box<dyn std::error::Error>,
> {
    let chain = HeavyweightPipeline::new().await?;
    chain.init_genesis().await?;
    let harness = NativeTokenHarness::spawn();
    let cid = *NATIVE_TOKEN_CONTRACT_ID;

    let secret = SecretKey::from_bytes([2u8; 32])?;
    let public = PublicKey::from_secret(secret.clone());
    let keypair = Keypair { secret, public };

    Ok((chain, harness, cid, keypair))
}

/// Build a genuine FeeV3 (`0x08`) call against the coinbase coin that `pf`
/// describes.
///
/// Every input parameter comes from `PrefetchedCoinbase`, which reads the
/// authoritative on-chain coin tree (`coinbase_coordination::prefetch_coinbase_params`).
/// They are not synthesised: a call built against a zero blind with a zero
/// Merkle path and a zero root proves membership of a tree the chain has never
/// held, and `accept_block` rejects the block at L2 witness verification. This
/// helper used to do exactly that.
///
/// The charged fee comes from `compute_fee_v3`, the production admission-fee
/// function — never a literal — and is returned alongside the call so the
/// caller can `add_fee(fee)` and have FeeCollectV1 collect exactly what the
/// call paid.
///
/// Returns `(call_data, proofs, fee)`.
pub fn native_token_call(
    pf: &PrefetchedCoinbase,
    harness: &NativeTokenHarness,
) -> std::result::Result<(Vec<u8>, Vec<Proof>, FeeAmount), Box<dyn std::error::Error>> {
    // fee = gas × base_price × CF × tier × risk (fee-spec.md §12.4.1). At zero
    // congestion (CF = 1.0), tier LOW (×1) and baseline risk (×1.0), the fee is
    // gas itself.
    let tier = FeeTier::LOW;
    let gas = 1_000u64;
    let fee = dwow_chain::fee_window::compute_fee_v3(
        gas,
        dwow_chain::fee_window::CongestionFactor::zero(),
        tier,
        RiskFactor::BASELINE,
    );

    // The change output goes to a fresh key. The input side is what has to
    // reconcile with the chain; the output side is a new coin.
    let recipient = PublicKey::from_secret(SecretKey::from_bytes([9u8; 32])?);

    let result = harness.fee_v3(
        pf.coin_value,
        DRKW_ASSET_ID.inner(),
        pallas::Base::zero(), // spend_hook
        pallas::Base::zero(), // user_data
        pf.commitment_blind,
        pf.leaf_position,
        pf.merkle_path.clone(),
        pf.merkle_root,
        pf.secret.clone(),
        pf.secret.clone(), // deterministic ephemeral secret in tests
        recipient,
        pallas::Base::zero(), // output spend_hook
        pallas::Base::zero(), // output user_data
        fee.get(),
        tier,
    )?;
    Ok((result.call_data, result.proofs, fee))
}
