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

use std::sync::Arc;

use dwow_chain::{
    Block, BlockHeader, CChainState, ContractCall, Miner, PowSource, Transaction, UncleBlock,
    build_uncle_merkle, create_uncle,
};
use dwow_core::Result;
use dwow_sdk::blockchain;

use super::genesis::GenesisHarness;

/// Two independent linear blockchain nodes for sync testing.
pub struct Harness {
    pub alice: GenesisHarness,
    pub bob: GenesisHarness,
}

impl Harness {
    pub fn new() -> Result<Self> {
        Ok(Self { alice: GenesisHarness::new()?, bob: GenesisHarness::new()? })
    }
}

/// Synthetic timestamp for test blocks, spaced 120s per height so the
/// consensus target stays at `u32::MAX` (no difficulty drift when blocks
/// are built in a rapid test loop).
const TEST_BASE_TIMESTAMP: u64 = 1776770000;

fn test_block_timestamp(height: u64) -> u64 {
    TEST_BASE_TIMESTAMP + (height - 1) * 120
}

/// Build a block header with `target: u32::MAX` (instant PoW).
pub fn build_test_header(
    chain_state: &CChainState,
    height: u64,
    merkle_root: blake3::Hash,
    timestamp: u64,
) -> BlockHeader {
    let previous_hash = if height <= 1 {
        blake3::Hash::from_bytes([0u8; 32])
    } else {
        match chain_state.get_latest_block() {
            Ok(block) => {
                let prev_key = block.header.randomx_key;
                let prev_vm = chain_state.get_vm(prev_key);
                { let guard = prev_vm.lock().unwrap(); block.hash_with_vm(&guard) }
            }
            Err(_) => blake3::Hash::from_bytes([0u8; 32]),
        }
    };

    BlockHeader {
        version: 1,
        previous: previous_hash,
        merkle_root,
        timestamp,
        target: u32::MAX,
        nonce: 0,
        height,
        uncle_merkle_root: [0u8; 32],
        total_reward: blockchain::expected_reward(height as u32),
        randomx_key: Miner::derive_key_from_height(height),
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
        outputs: vec![dwow_chain::TxOutput { value: reward, script: vec![] }],
        contract_calls: vec![],
        lock_time: 0,
                nullifiers: vec![],
    }
}

/// Build a transaction with a single contract call.
pub fn build_contract_tx(contract_id: [u8; 32], call_data: Vec<u8>) -> Transaction {
    Transaction {
        version: 1,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![ContractCall { contract_id, data: call_data }],
        lock_time: 0,
                nullifiers: vec![],
    }
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
            layer.push(*layer.last().unwrap());
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
    height: u64,
    txs: Vec<Transaction>,
) -> Block {
    let timestamp = test_block_timestamp(height);
    let merkle_root = compute_merkle_root(&txs);
    let header = build_test_header(chain_state, height, merkle_root, timestamp);
    Block { header, transactions: txs }
}

/// Build an uncle block from a non-canonical block.
pub fn build_test_uncle(block: Block, depth: u8, base_reward: u64) -> UncleBlock {
    create_uncle(block, depth, base_reward)
}

/// Build a canonical block with uncles.
pub fn build_test_block_with_uncles(
    chain_state: &CChainState,
    height: u64,
    txs: Vec<Transaction>,
    uncles: &[UncleBlock],
) -> Block {
    let timestamp = test_block_timestamp(height);
    let merkle_root = compute_merkle_root(&txs);
    let randomx_key = Miner::derive_key_from_height(height);
    let vm = chain_state.get_vm(randomx_key);
    let (uncle_merkle_root, _) = build_uncle_merkle(uncles, &vm.lock().unwrap());
    let previous_hash = if height <= 1 {
        blake3::Hash::from_bytes([0u8; 32])
    } else {
        match chain_state.get_latest_block() {
            Ok(block) => {
                let prev_key = block.header.randomx_key;
                let prev_vm = chain_state.get_vm(prev_key);
                { let guard = prev_vm.lock().unwrap(); block.hash_with_vm(&guard) }
            }
            Err(_) => blake3::Hash::from_bytes([0u8; 32]),
        }
    };

    Block {
        header: BlockHeader {
            version: 1,
            previous: previous_hash,
            merkle_root,
            timestamp,
            target: u32::MAX,
            nonce: 0,
            height,
            uncle_merkle_root,
            total_reward: blockchain::expected_reward(height as u32),
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
