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

//! Transaction structures for linear blockchain

use blake3::Hash;
use serde::{Deserialize, Serialize};

// ============================================================================
// Cryptographic newtypes — compile-time enforcement of mathematical spec.
// ============================================================================
// These types prevent the compiler from accepting semantically invalid code.
// CoinCommitment and Nullifier are both 32 bytes but MUST NOT be swappable.
// TokenCommitment is also 32 bytes — distinct from both.
// ZkPublicInputs enforces exactly 7 elements at compile time.
// PedersenCoordinate wraps a 32-byte value commitment coordinate.
// ============================================================================

/// Coin commitment: C = poseidon_hash([pk.x, pk.y, value, token_id, ...]).
/// MUST NOT be swapped with Nullifier or raw [u8; 32].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoinCommitment(pub [u8; 32]);

impl CoinCommitment {
    pub fn to_bytes(&self) -> [u8; 32] { self.0 }
}

/// Nullifier: nf = poseidon_hash([sk_H, C]).
/// MUST NOT be swapped with CoinCommitment or raw [u8; 32].
/// Zero-nullifier is invalid per spec — use from_bytes() to construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nullifier(pub [u8; 32]);

impl Nullifier {
    /// Construct a Nullifier, rejecting the all-zeros sentinel.
    /// Zero nullifier = unclaimed reward = invalid block per Phase 0 validation.
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        if bytes == [0u8; 32] { None } else { Some(Self(bytes)) }
    }

    pub fn to_bytes(&self) -> [u8; 32] { self.0 }

    pub fn is_zero(&self) -> bool { self.0 == [0u8; 32] }
}

/// Token commitment: poseidon_hash(token_id, token_blind).
/// MUST NOT be swapped with CoinCommitment or Nullifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCommitment(pub [u8; 32]);

impl TokenCommitment {
    pub fn to_bytes(&self) -> [u8; 32] { self.0 }
}

/// ZK public inputs: exactly 7 field elements exposed to the verifier.
/// Array size [7] is enforced at compile time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZkPublicInputs(pub [[u8; 32]; 7]);

impl ZkPublicInputs {
    pub fn as_array(&self) -> &[[u8; 32]; 7] { &self.0 }
}

/// Pedersen commitment coordinate — wraps a 32-byte value.
/// Distinct from CoinCommitment, Nullifier, and TokenCommitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedersenCoordinate(pub [u8; 32]);

impl PedersenCoordinate {
    pub fn to_bytes(&self) -> [u8; 32] { self.0 }
}

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
/// Contains ZK proof data, coin commitment, nullifier, and encrypted note.
/// Newtypes enforce the mathematical spec at compile time:
///   - CoinCommitment ≠ Nullifier ≠ TokenCommitment (compiler rejects swaps)
///   - ZkPublicInputs enforces exactly 7 elements
///   - Nullifier::from_bytes rejects zero sentinel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseTransaction {
    /// ZK proof bytes (Mint_V1 circuit)
    pub proof: Vec<u8>,
    /// ZK public inputs: [coin, vc.x, vc.y, tc, nf, S_H.x, S_H.y] — exactly 7
    pub public_inputs: ZkPublicInputs,
    /// Poseidon hash of coin attributes — C = poseidon_hash([pk.x, pk.y, value, ...])
    pub coin: CoinCommitment,
    /// Pedersen value commitment x-coordinate
    pub value_commit_x: PedersenCoordinate,
    /// Pedersen value commitment y-coordinate
    pub value_commit_y: PedersenCoordinate,
    /// Poseidon token commitment
    pub token_commit: TokenCommitment,
    /// Nullifier: nf = poseidon_hash(sk_H.inner(), C) — capability claim.
    /// The miner exercises the coinbase capability by publishing this nullifier.
    /// Validators verify it against the nullifier SMT and ZK proof.
    /// Constructed via Nullifier::from_bytes() — rejects [0u8; 32].
    pub nullifier: Nullifier,
    /// Cumulative supply commitment x-coordinate (S_H.x)
    pub new_cumulative_x: PedersenCoordinate,
    /// Cumulative supply commitment y-coordinate (S_H.y)
    pub new_cumulative_y: PedersenCoordinate,
    /// AEAD encrypted note (AeadEncryptedNote serialized)
    pub encrypted_note: Vec<u8>,
}

/// Transaction - a transfer of value in the blockchain
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Pre-computed nullifiers for mempool double-spend detection
    #[serde(default)]
    pub nullifiers: Vec<Vec<u8>>,
}

impl Transaction {
    /// Calculate the hash of this transaction
    pub fn hash(&self) -> Hash {
        blake3::hash(&serde_json::to_vec(self).unwrap())
    }
}