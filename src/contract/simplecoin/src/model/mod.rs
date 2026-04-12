/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! SimpleCoin data models
//!
//! Minimal UTXO model with no privacy features.
//! Values and coin data are stored in the clear.

use darkfi_sdk::{crypto::MerkleNode, pasta::pallas};
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// COIN STRUCTURES
// ============================================================================

/// A simple coin - no encryption, no commitments
/// This is the baseline UTXO model
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Coin {
    /// Owner public key (x coordinate)
    pub owner_x: pallas::Base,
    /// Owner public key (y coordinate)
    pub owner_y: pallas::Base,
    /// Coin value
    pub value: u64,
    /// Token identifier (raw pallas base)
    pub token_id: pallas::Base,
    /// Uniqueness nonce
    pub nonce: u64,
}

/// Compute coin ID from coin data
/// coin_id = poseidon_hash(owner_x, owner_y, value, token_id, nonce)
impl Coin {
    pub fn coin_id(&self) -> pallas::Base {
        darkfi_sdk::crypto::poseidon_hash([
            self.owner_x,
            self.owner_y,
            pallas::Base::from(self.value),
            self.token_id,
            pallas::Base::from(self.nonce),
        ])
    }
}

/// A nullifier prevents double-spending
/// nullifier = poseidon_hash(coin_id)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Nullifier(pallas::Base);

impl Nullifier {
    pub fn new(coin_id: pallas::Base) -> Self {
        Self(darkfi_sdk::crypto::poseidon_hash([coin_id]))
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

// ============================================================================
// TRANSACTION INPUTS/OUTPUTS
// ============================================================================

/// Input to a transaction - references an existing coin
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    /// The coin being spent
    pub coin: Coin,
    /// Merkle proof the coin exists
    pub merkle_root: MerkleNode,
    /// Merkle path to prove inclusion
    pub merkle_path: Vec<MerkleNode>,
    /// Leaf position in Merkle tree
    pub leaf_position: u64,
}

/// Output of a transaction - creates new coins
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Output {
    /// The new coin created
    pub coin: Coin,
    /// Merkle tree leaf index
    pub leaf_position: u64,
}

// ============================================================================
// FUNCTION PARAMETERS
// ============================================================================

/// Parameters for GenesisV1 - create initial coin supply
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GenesisParamsV1 {
    /// Initial coins to create
    pub coins: Vec<Coin>,
}

/// State update for GenesisV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GenesisUpdateV1 {
    /// Coins to add to state
    pub coins: Vec<Coin>,
}

/// Parameters for TransferV1 - send coins to another party
/// This uses signature-based ownership verification (no ZK required)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferParamsV1 {
    /// Input coins being spent
    pub inputs: Vec<Input>,
    /// Output coins being created
    pub outputs: Vec<Output>,
    /// Signature over the transaction hash (scalar)
    pub signature: pallas::Base,
}

/// State update for TransferV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferUpdateV1 {
    /// Nullifiers for spent coins
    pub nullifiers: Vec<Nullifier>,
    /// New coins created
    pub coins: Vec<Coin>,
}

/// Parameters for SpendV1 - consume coins, create change
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpendParamsV1 {
    /// Input coin being spent
    pub input: Input,
    /// Change output (remaining value after fee)
    pub change_output: Output,
    /// Fee being paid (part of change)
    pub fee: u64,
    /// Signature over the transaction hash (scalar)
    pub signature: pallas::Base,
}

/// State update for SpendV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpendUpdateV1 {
    /// Nullifier for spent coin
    pub nullifier: Nullifier,
    /// New change coin
    pub change_coin: Coin,
}

/// Parameters for MeltV1 - destroy coins (e.g., for fees)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MeltParamsV1 {
    /// Input coins being melted
    pub inputs: Vec<Input>,
    /// Amount being melted
    pub melt_amount: u64,
    /// Signature over the transaction hash (scalar)
    pub signature: pallas::Base,
}

/// State update for MeltV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MeltUpdateV1 {
    /// Nullifiers for melted coins
    pub nullifiers: Vec<Nullifier>,
    /// Total amount melted
    pub melt_amount: u64,
}