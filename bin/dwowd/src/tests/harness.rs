/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! 2-Node Base Harness for linear blockchain sync tests + block construction helpers.
//!
//! Creates two GenesisHarness instances (alice and bob) for testing
//! block sync and contract propagation between independent nodes.
//!
//! Also provides block/uncle construction helpers for in-process
//! execution testing. Uses `target: u32::MAX` (any nonce passes PoW)
//! so blocks are instant — the WASM execution path is identical to production.
//!
//! Adapted for CChainState (commit 597691582 refactor).


use dwow_chain::fee_window::FeeWindowFlags;
use dwow_chain::{
    Block, BlockHeader, CChainState, ContractCall, Miner, PowSource, Transaction, UncleBlock,
    build_uncle_merkle, create_uncle,
};
use dwow_sdk::blockchain::{self, BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion, MoneroBlockHeight};
use dwow_sdk::test_support::{TestError, TestResult};

/// This module's name in an INFRA-FAIL attribution.
const MODULE: &str = "tests::harness";

/// INFRA-FAIL: a block-construction helper in this shared module failed.
///
/// The bound is `Into<Box<dyn Error>>` rather than `Error`, matching `TestError::infra`
/// itself, so a `String` cause from a poisoned lock or a formatted diagnostic is a
/// first-class cause here too — without it, every such site had to spell out the
/// `TestError::Infra { .. }` struct literal by hand.
fn infra(stage: &'static str, cause: impl Into<Box<dyn std::error::Error>>) -> TestError {
    TestError::infra(MODULE, stage, cause)
}

/// The key the coinbase note commits to. `accept_block` requires `header.miner` to equal it for
/// every non-genesis block, and derives its check from the same `pow_reward_params` extraction
/// used here — so builder and rule cannot drift apart again the way they did in `55a04076c9`.
///
/// `[0u8; 32]` when there is no `PoWRewardV1` call: genesis's value, and inert for blocks that
/// are built but never accepted. A test that needs a deliberately *mismatched* miner sets
/// `block.header.miner` after construction, which is what `uncle_minting` does for its uncle.
pub(crate) fn miner_for(txs: &[Transaction]) -> [u8; 32] {
    crate::block_acceptor::pow_reward_params(txs)
        .map(|p| p.commitment_attrs.public_key.to_bytes())
        .unwrap_or([0u8; 32])
}

/// Synthetic timestamp for test blocks, spaced 120s per height so the
/// consensus target stays at `u32::MAX` (no difficulty drift when blocks
/// are built in a rapid test loop).
const TEST_BASE_TIMESTAMP: u64 = 1776770000;

fn test_block_timestamp(height: BlockHeight) -> u64 {
    TEST_BASE_TIMESTAMP + (height.get() - 1) * 120
}

/// Build a block header with `target: u32::MAX` (instant PoW).
///
/// Fails as INFRA-FAIL: it reads the previous block's VM and hash through
/// `chain_state`, and either step can fail for reasons unrelated to the test.
pub fn build_test_header(
    chain_state: &CChainState,
    height: BlockHeight,
    merkle_root: blake3::Hash,
    timestamp: u64,
    miner: [u8; 32],
) -> TestResult<BlockHeader> {
    let previous_hash = if height <= BlockHeight::GENESIS {
        blake3::Hash::from_bytes([0u8; 32])
    } else {
        match chain_state.get_latest_block() {
            Ok(block) => {
                let prev_key = block.header.randomx_key;
                let prev_vm = chain_state
                    .get_vm(prev_key)
                    .map_err(|e| infra("creating the previous block's VM", e))?;
                let guard = prev_vm
                    .lock()
                    .map_err(|e| infra("locking the previous block's VM", format!("mutex poisoned: {e}")))?;
                block
                    .hash_with_vm(&guard)
                    .map_err(|e| infra("hashing the previous block", e))?
            }
            Err(_) => blake3::Hash::from_bytes([0u8; 32]),
        }
    };

    Ok(BlockHeader {
        fee_window_flags: FeeWindowFlags::default(),
        version: BlockVersion::CURRENT,
        previous: previous_hash,
        merkle_root,
        timestamp: BlockTimestamp::new(timestamp),
        target: BlockTarget::MAX,
        nonce: 0,
        height,
        uncle_merkle_root: [0u8; 32],
        total_reward: blockchain::expected_reward(height),
        randomx_key: Miner::derive_key_from_height(height),
        miner,
        commitment_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        pow_source: PowSource::Native,
        anchor_owner: [0u8; 32],
        caribina_anchor: None,
    })
}

/// Build a transaction with a single contract call.
pub fn build_contract_tx(contract_id: dwow_sdk::crypto::ContractId, call_data: Vec<u8>) -> Transaction {
    Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![ContractCall { contract_id, data: call_data }],
        lock_time: 0,
                nullifiers: vec![],
        witness: vec![],
    }
}

/// Build a transaction with multiple contract calls (children before parent, DFS post-order).
/// The `calls` vector must list `(contract_id, data)` in the same order as the witness's
/// `calls` (see `build_witness_tree`), so `decode_and_reconcile` reconciles positionally.
pub fn build_contract_tx_tree(calls: Vec<(dwow_sdk::crypto::ContractId, Vec<u8>)>) -> Transaction {
    Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: calls
            .into_iter()
            .map(|(contract_id, data)| ContractCall { contract_id, data })
            .collect(),
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    }
}

/// Build a transaction carrying one genuine FeeV3 (selector `0x08`) call with a
/// plaintext fee, encoded through the contract's own `FeeParamsV3::encode()` —
/// never assembled by hand.
///
/// The distinction is load-bearing, and there is a scar behind it. A fixture that
/// wrote `[0x08] ++ fee.to_le_bytes() ++ zeros` satisfies the selector and length
/// gates in `as_mass_balance_fee_v3()`, but the real `FeeParamsV3::decode()` parses
/// an `Input` out of the first 224 bytes, so it fails — and every consumer that does
/// the real decode (`sum_block_fee_v3`, `NativeTokenFeeSignallingExtractor`) then
/// skips the call as malformed and sees a fee of *zero*. A test built on such a
/// fixture tests the fabrication, not the summing. `registry/model.rs` records the
/// same lesson for its own copy; this helper is the one definition of a real fee tx,
/// so a wire-format move breaks one place instead of being absorbed by three byte
/// strings, one of which will rot silently.
pub fn build_fee_v3_tx(fee: u64) -> TestResult<Transaction> {
    use dwow_native_token_contract::model::{
        fee::{FeeParamsV3, FeeV3TxBinding},
        Commitment, Input, Nullifier, Output, DRKW_ASSET_ID,
    };
    use dwow_sdk::blockchain::{FeeAmount, FeeTier};
    use dwow_sdk::crypto::keypair::SecretKey;
    use dwow_sdk::crypto::note::AeadEncryptedNote;
    use dwow_sdk::crypto::{BaseBlind, FuncId, MerkleNode, PublicKey};
    use dwow_sdk::pasta::pallas;

    let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(1u64)));
    let nf = Nullifier::from_bytes([1u8; 32])
        .map_err(|e| infra("deriving the fee fixture's nullifier", e))?;

    let params = FeeParamsV3 {
        input: Input {
            value_commit: pallas::Point::default(),
            token_commit: pallas::Base::zero(),
            nullifier: nf,
            merkle_root: MerkleNode::new(pallas::Base::zero()),
            user_data_enc: pallas::Base::zero(),
            spend_hook: FuncId::none(),
            signature_public: pk,
        },
        output: Output {
            value_commit: pallas::Point::default(),
            token_commit: pallas::Base::zero(),
            commitment: Commitment::from_attributes(
                &pk,
                1000,
                DRKW_ASSET_ID,
                FuncId::none(),
                pallas::Base::zero(),
                BaseBlind::ZERO,
            ),
            nullifier: Some(nf),
            note: AeadEncryptedNote { ciphertext: vec![0u8; 32], ephem_public: pk },
        },
        fee: FeeAmount::new(fee),
        tier: FeeTier::LOW,
        fee_value_commit: pallas::Point::default(),
        fee_v3_tx_binding: FeeV3TxBinding::compute(pallas::Base::zero(), pallas::Base::zero()),
        tx_nonce: pallas::Base::zero(),
    };

    // The real encoder, not a hand-assembled byte string.
    // `encode` returns a `Result` since the length-prefix refactor; this call site had not been
    // updated with the rest of that change, which left the whole `dwowd` test build broken.
    let data = dwow_sdk::mass_balance_call_data::MassBalanceFeeV3CallData::new(
        params.encode().map_err(|e| infra("encoding the fee fixture's params", e))?,
    )
    .encode();

    Ok(Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![ContractCall {
            contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
            data,
        }],
        lock_time: 0,
        nullifiers: vec![],
        witness: vec![],
    })
}

/// Compute Merkle root from transactions (same as Block::verify_merkle_root).
pub fn compute_merkle_root(txs: &[Transaction]) -> blake3::Hash {
    let tx_hashes: Vec<blake3::Hash> = txs.iter().map(|tx| tx.hash()).collect();
    if tx_hashes.is_empty() {
        return blake3::hash(&[]);
    }
    let mut layer = tx_hashes;
    while layer.len() > 1 {
        if layer.len() % 2 != 0 {
            // `layer.len() > 1` here, so the last element exists. Written as a
            // binding rather than an unwrap: the length check is what makes it
            // safe, and the type system can see that without a panic site.
            if let Some(last) = layer.last() {
                layer.push(*last);
            }
        }
        let mut next: Vec<blake3::Hash> = Vec::with_capacity(layer.len() / 2);
        for pair in layer.chunks(2) {
            let mut combined = pair[0].as_bytes().to_vec();
            combined.extend_from_slice(pair[1].as_bytes());
            next.push(blake3::hash(&combined));
        }
        layer = next;
    }
    layer[0]
}

/// Build a canonical block from a set of transactions (instant PoW, no mining).
pub fn build_test_block(
    chain_state: &CChainState,
    height: BlockHeight,
    txs: Vec<Transaction>,
) -> TestResult<Block> {
    let timestamp = test_block_timestamp(height);
    let merkle_root = compute_merkle_root(&txs);
    let miner = miner_for(&txs);
    let header = build_test_header(chain_state, height, merkle_root, timestamp, miner)?;
    Ok(Block { header, transactions: txs })
}

/// Build an uncle block from a non-canonical block.
pub fn build_test_uncle(block: Block, depth: u8, base_reward: BlockReward) -> UncleBlock {
    create_uncle(block, depth, base_reward)
}

/// Build a canonical block with uncles.
pub fn build_test_block_with_uncles(
    chain_state: &CChainState,
    height: BlockHeight,
    txs: Vec<Transaction>,
    uncles: &[UncleBlock],
) -> TestResult<Block> {
    let timestamp = test_block_timestamp(height);
    let merkle_root = compute_merkle_root(&txs);
    let randomx_key = Miner::derive_key_from_height(height);
    let (uncle_merkle_root, _) = build_uncle_merkle(uncles);
    let previous_hash = if height <= BlockHeight::GENESIS {
        blake3::Hash::from_bytes([0u8; 32])
    } else {
        match chain_state.get_latest_block() {
            Ok(block) => {
                let prev_key = block.header.randomx_key;
                let prev_vm = chain_state
                    .get_vm(prev_key)
                    .map_err(|e| infra("creating the previous block's VM", e))?;
                let guard = prev_vm
                    .lock()
                    .map_err(|e| infra("locking the previous block's VM", format!("mutex poisoned: {e}")))?;
                block
                    .hash_with_vm(&guard)
                    .map_err(|e| infra("hashing the previous block", e))?
            }
            Err(_) => blake3::Hash::from_bytes([0u8; 32]),
        }
    };

    let base_reward = blockchain::expected_reward(height);
    let (total_reward, _) = dwow_chain::compute_reward(base_reward, uncles)
        .map_err(|e| infra("computing the reward split", e))?;
    let miner = miner_for(&txs);
    Ok(Block {
        header: BlockHeader {
            version: BlockVersion::CURRENT,
            previous: previous_hash,
            merkle_root,
            timestamp: BlockTimestamp::new(timestamp),
            target: BlockTarget::MAX,
            nonce: 0,
            height,
            uncle_merkle_root,
            total_reward,
            randomx_key,
            miner,
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        },
        transactions: txs,
    })
}
