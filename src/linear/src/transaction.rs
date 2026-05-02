/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Transaction structures for linear blockchain

use blake3::Hash;
use serde::{Deserialize, Serialize};

/// Transaction input - reference to an unspent output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    /// Reference to the previous transaction output
    pub previous_output: Hash,
    /// Signature script / proof
    pub script: Vec<u8>,
    /// Sequence number (for timelock)
    pub sequence: u32,
}

/// Transaction output - new value created in this transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    /// Value being transferred
    pub value: u64,
    /// Public key or script hash
    pub script: Vec<u8>,
}

/// A contract call embedded in a transaction input's script field.
/// Format: [1 byte call_idx][32 bytes contract_id][varbytes payload]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCall {
    /// ID of the contract to invoke (32 bytes)
    pub contract_id: [u8; 32],
    /// Call data passed to the contract (function selector + params)
    pub data: Vec<u8>,
}

/// Privacy-preserving coinbase output.
/// Contains ZK proof data, coin commitment, and encrypted note.
/// All fields are raw bytes — ZK verification is handled at a higher layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseTransaction {
    /// ZK proof bytes (Mint_V1 circuit)
    pub proof: Vec<u8>,
    /// ZK public inputs: [coin, value_commit.x, value_commit.y, token_commit]
    pub public_inputs: [[u8; 32]; 4],
    /// Poseidon hash of coin attributes (Coin::inner())
    pub coin: [u8; 32],
    /// Pedersen value commitment x-coordinate (32 bytes)
    pub value_commit_x: [u8; 32],
    /// Pedersen value commitment y-coordinate (32 bytes)
    pub value_commit_y: [u8; 32],
    /// Poseidon token commitment
    pub token_commit: [u8; 32],
    /// AEAD encrypted note (AeadEncryptedNote serialized)
    pub encrypted_note: Vec<u8>,
}

/// Transaction - a transfer of value in the blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction version
    pub version: u8,
    /// Inputs spent in this transaction
    pub inputs: Vec<Input>,
    /// Outputs created by this transaction
    pub outputs: Vec<Output>,
    /// Contract calls embedded in inputs (optional extension)
    pub contract_calls: Vec<ContractCall>,
    /// Lock time (can be block height or timestamp)
    pub lock_time: u64,
    /// Optional privacy-preserving coinbase (for block reward transactions)
    pub coinbase: Option<CoinbaseTransaction>,
}

impl Transaction {
    /// Calculate the hash of this transaction
    pub fn hash(&self) -> Hash {
        blake3::hash(&serde_json::to_vec(self).unwrap())
    }
}