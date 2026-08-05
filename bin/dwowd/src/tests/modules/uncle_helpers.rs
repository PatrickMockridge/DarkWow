//! Uncle block construction and shared helpers for block-execution tests.
//!
//! Used by: Category 4 (all 8 block-exec tests).
//! Spec: heavyweight-spec.md §8 (Block Execution Tests).

use dwow_contract_test_harness::harness::NativeTokenHarness;
use dwow_core::Result;
use dwow_core::zk::Proof;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget};
use dwow_sdk::crypto::{ContractId, Keypair, SecretKey, PublicKey, NATIVE_TOKEN_CONTRACT_ID};
use dwow_sdk::crypto::MerkleNode;
use dwow_sdk::pasta::pallas;

use crate::tests::blockchain::HeavyweightPipeline;

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
pub fn mine_test_nonce(block: &dwow_chain::Block, vm: &randomx::RandomXVM, target: BlockTarget) -> u32 {
    for nonce in 0u32..1_000_000 {
        let mut b = block.clone();
        b.header.nonce = nonce;
        let hash = b.hash_with_vm(vm);
        let hash_u32 = u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap());
        if hash_u32 <= target.get() {
            return nonce;
        }
    }
    panic!("Could not find valid nonce for target {} after 1M iterations", target);
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
    let block = crate::tests::harness::build_test_block(
        &chain.chain_state, height, vec![tx],
    );
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

/// Generate call_data via NativeTokenHarness.
/// Uses harness.fee() — produces ZK call_data with FeeV1 circuit.
/// Returns (call_data, proofs) for use with chain.block() methods.
pub fn native_token_call(
    harness: &NativeTokenHarness,
    keypair: Keypair,
) -> std::result::Result<(Vec<u8>, Vec<Proof>), Box<dyn std::error::Error>> {
    let recipient = PublicKey::from_secret(SecretKey::from_bytes([9u8; 32])?);
    let result = harness.fee(
        1000,
        pallas::Base::from(1u64),
        pallas::Base::from(0u64),
        pallas::Base::from(0u64),
        pallas::Base::from(0u64),
        0,
        vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
        keypair.secret.clone(),
        keypair.secret,
        recipient,
        pallas::Base::from(0u64),
        pallas::Base::from(0u64),
        10,
    )?;
    Ok((result.call_data, result.proofs))
}
