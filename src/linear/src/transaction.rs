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

/// Transaction - a transfer of value in the blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction version
    pub version: u8,
    /// Inputs spent in this transaction
    pub inputs: Vec<Input>,
    /// Outputs created by this transaction
    pub outputs: Vec<Output>,
    /// Lock time (can be block height or timestamp)
    pub lock_time: u64,
}

impl Transaction {
    /// Calculate the hash of this transaction
    pub fn hash(&self) -> Hash {
        blake3::hash(&serde_json::to_vec(self).unwrap())
    }
}