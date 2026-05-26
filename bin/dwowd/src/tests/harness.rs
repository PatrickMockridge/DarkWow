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

//! 2-Node Base Harness for linear blockchain sync tests.
//!
//! Creates two GenesisHarness instances (alice and bob) for testing
//! block sync and contract propagation between independent nodes.
//!
//! Also provides block/uncle construction helpers for in-process
//! execution testing (no Docker needed). Uses `target: u32::MAX`
//! (any nonce passes PoW) so blocks are instant — the WASM execution
//! path is identical to production.

use dwow_core::Result;
use dwow_chain::{
    Block, BlockHeader, ContractCall, Miner, Output, PowSource, Transaction, UncleBlock,
    build_uncle_merkle, create_uncle,
};
use blake3::Hash as Blake3Hash;

use super::genesis::GenesisHarness;
use crate::blockchain::LinearBlockchain;

/// Two independent linear blockchain nodes for sync testing.
pub struct Harness {
    pub alice: GenesisHarness,
    pub bob: GenesisHarness,
}

impl Harness {
    /// Create two independent GenesisHarness instances.
    pub fn new() -> Result<Self> {
        Ok(Self { alice: GenesisHarness::new()?, bob: GenesisHarness::new()? })
    }
}

/// Build a block header with `target: u32::MAX` (instant PoW).
/// The merkle root must be computed from the actual transactions.
pub fn build_test_header(
    blockchain: &LinearBlockchain,
    height: u64,
    merkle_root: Blake3Hash,
    timestamp: u64,
) -> BlockHeader {
    let previous_hash = if height <= 1 {
        Blake3Hash::from_bytes([0u8; 32])
    } else {
        match blockchain.get_latest_block() {
            Ok(block) => {
                let prev_key = block.header.randomx_key;
                let prev_vm = blockchain.get_vm(prev_key);
                block.hash(&prev_vm)
            }
            Err(_) => Blake3Hash::from_bytes([0u8; 32]),
        }
    };

    let randomx_key = Miner::derive_key_from_height(height);
    let reward = dwow_sdk::blockchain::expected_reward(height as u32);

    BlockHeader {
        version: 1,
        previous: previous_hash,
        merkle_root,
        timestamp,
        target: u32::MAX, // Any nonce passes PoW
        nonce: 0,
        height,
        uncle_merkle_root: [0u8; 32],
        total_reward: reward,
        randomx_key,
        coin_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: 0,
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,

    pow_source: PowSource::Native,

    }
}

/// Build a coinbase transaction for the given reward value.
pub fn build_coinbase_tx(reward: u64) -> Transaction {
    Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![Output { value: reward, script: vec![] }],
        contract_calls: vec![],
        lock_time: 0,
        coinbase: None,
    }
}

/// Build a transaction with a single contract call.
///
/// The WASM runtime expects `call.data` to be a serialized
/// `Vec<DarkLeaf<ContractCall>>`, not raw per-call data. This function
/// wraps `call_data` in the proper SDK types and serializes it so the
/// WASM entrypoint can deserialize and dispatch to the correct handler.
pub fn build_contract_tx(contract_id: [u8; 32], call_data: Vec<u8>) -> Transaction {
    let sdk_call = dwow_sdk::tx::ContractCall {
        contract_id: dwow_sdk::crypto::ContractId::from_bytes(contract_id)
            .expect("valid contract id"),
        data: call_data,
    };
    let dark_leaf = dwow_sdk::dark_tree::DarkLeaf {
        data: sdk_call,
        parent_index: None,
        children_indexes: vec![],
    };
    let calls_vec = vec![dark_leaf];
    let serialized = dwow_serial::serialize(&calls_vec);

    Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![ContractCall { contract_id, data: serialized }],
        lock_time: 0,
        coinbase: None,
    }
}

/// Compute the merkle root from a slice of transactions.
/// Mirrors `Block::verify_merkle_root()` and the miner's `create_block()`.
pub fn compute_merkle_root(txs: &[Transaction]) -> Blake3Hash {
    let tx_hashes: Vec<Blake3Hash> = txs.iter().map(|tx| tx.hash()).collect();
    if tx_hashes.is_empty() {
        return blake3::hash(&[]);
    }
    let mut layer = tx_hashes.clone();
    while layer.len() > 1 {
        if layer.len() % 2 != 0 {
            layer.push(*layer.last().unwrap());
        }
        layer = layer
            .chunks(2)
            .map(|pair| {
                let mut combined = pair[0].as_bytes().to_vec();
                combined.extend_from_slice(pair[1].as_bytes());
                blake3::hash(&combined)
            })
            .collect();
    }
    layer[0]
}

/// Build a canonical block from a set of transactions.
/// Uses `target: u32::MAX` — any nonce passes PoW (instant, no mining).
pub fn build_test_block(
    blockchain: &LinearBlockchain,
    height: u64,
    txs: Vec<Transaction>,
) -> Block {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let merkle_root = compute_merkle_root(&txs);
    let header = build_test_header(blockchain, height, merkle_root, now);
    Block { header, transactions: txs }
}

/// Build an uncle block from a non-canonical block.
/// Computes the pin reward based on depth.
///
/// Note: Uncle coinbase outputs are stored but only the pin_reward is
/// claimable (not the full coinbase value). The uncle's coinbase is
/// NOT tracked in the canonical coin_set — only the canonical block's
/// coinbase is tracked for double-mint protection. This is by design:
/// uncles produce their own outputs but the pin mechanism controls
/// which portion is actually spendable.
pub fn build_test_uncle(
    block: Block,
    depth: u8,
    base_reward: u64,
) -> UncleBlock {
    create_uncle(block, depth, base_reward)
}

/// Build a canonical block with uncles, computing the uncle merkle root.
/// Updates the block header with the correct `uncle_merkle_root`.
pub fn build_test_block_with_uncles(
    blockchain: &LinearBlockchain,
    height: u64,
    txs: Vec<Transaction>,
    uncles: &[UncleBlock],
) -> Block {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let merkle_root = compute_merkle_root(&txs);

    // Get the RandomX VM for this height
    let randomx_key = Miner::derive_key_from_height(height);
    let vm = blockchain.get_vm(randomx_key);

    // Compute uncle merkle root
    let (uncle_merkle_root, _) = build_uncle_merkle(uncles, &vm);

    let previous_hash = if height <= 1 {
        Blake3Hash::from_bytes([0u8; 32])
    } else {
        match blockchain.get_latest_block() {
            Ok(block) => {
                let prev_key = block.header.randomx_key;
                let prev_vm = blockchain.get_vm(prev_key);
                block.hash(&prev_vm)
            }
            Err(_) => Blake3Hash::from_bytes([0u8; 32]),
        }
    };

    let reward = dwow_sdk::blockchain::expected_reward(height as u32);

    Block {
        header: BlockHeader {
            version: 1,
            previous: previous_hash,
            merkle_root,
            timestamp: now,
            target: u32::MAX,
            nonce: 0,
            height,
            uncle_merkle_root,
            total_reward: reward,
            randomx_key,
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
    
        pow_source: PowSource::Native,
    
        },
        transactions: txs,
    }
}

/// Execute a single WASM contract call through the blockchain runtime.
/// Creates a block, applies it, and returns whether execution succeeded.
pub async fn exec_contract_call(
    blockchain: &LinearBlockchain,
    contract_id: [u8; 32],
    call_data: Vec<u8>,
) -> Result<()> {
    let height = blockchain.get_height();
    let reward = dwow_sdk::blockchain::expected_reward((height + 1) as u32);

    let coinbase = build_coinbase_tx(reward);
    let contract_tx = build_contract_tx(contract_id, call_data);
    let txs = vec![contract_tx, coinbase];
    let block = build_test_block(blockchain, height + 1, txs);

    blockchain.apply_block_with_uncles(&block, &[]).await
}

/// Execute a WASM contract call with uncle blocks for throughput testing.
/// Both canonical and uncle transactions are executed.
pub async fn exec_with_uncles(
    blockchain: &LinearBlockchain,
    contract_id: [u8; 32],
    canonical_data: Vec<u8>,
    uncle_data: Vec<Vec<u8>>,
) -> Result<()> {
    let height = blockchain.get_height();
    let next_height = height + 1;
    let reward = dwow_sdk::blockchain::expected_reward(next_height as u32);

    let coinbase = build_coinbase_tx(reward);
    let canonical_tx = build_contract_tx(contract_id, canonical_data);
    let txs = vec![canonical_tx, coinbase];

    // Build uncle blocks with their transactions
    let mut uncles = Vec::new();
    for data in &uncle_data {
        let uncle_tx = build_contract_tx(contract_id, data.clone());
        let uncle_coinbase = build_coinbase_tx(reward);
        let uncle_block = build_test_block(
            blockchain,
            next_height,
            vec![uncle_tx, uncle_coinbase],
        );
        let uncle = build_test_uncle(uncle_block, 1, reward);
        uncles.push(uncle);
    }

    let block = build_test_block_with_uncles(blockchain, next_height, txs, &uncles);
    blockchain.apply_block_with_uncles(&block, &uncles).await
}
