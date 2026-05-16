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

//! Money V3 data models
//!
//! Privacy-first design for DeFi tokens (wrapped, stablecoins, etc.)
//! No EC operations - everything is a field element.
//!
//! ## Token Model
//!
//! - TokenMintV1: Creates a new token type (stablecoin, wrapped, etc.)
//! - AuthTokenMintV1: Authorizes minting for an existing token
//! - MintV1: Mints tokens of an existing token type
//! - BurnV1: Burns tokens
//! - TransferV1: Private token transfer

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, MerkleNode},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

// Re-export for use in client modules
pub use dwow_sdk::crypto::note::AeadEncryptedNote;

// ============================================================================
// TOKEN/SYMBOLIC CONSTANTS
// ============================================================================

/// Maximum value per coin (prevent overflow)
pub const MAX_COIN_VALUE: u64 = 1_000_000_000_000;

// ============================================================================
// NULLIFIER
// ============================================================================

/// Nullifier definition (for double-spend prevention)
/// Money V3 uses poseidon_hash(secret, coin) for nullifier derivation
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Nullifier(pallas::Base);

impl Nullifier {
    /// Create a new nullifier from secret and coin
    pub fn new(secret: pallas::Base, coin: pallas::Base) -> Self {
        Nullifier(poseidon_hash([secret, coin]))
    }

    /// Create a new nullifier from secret and token_id (for auth mint)
    pub fn new_for_auth(secret: pallas::Base, token_id: pallas::Base) -> Self {
        Nullifier(poseidon_hash([secret, token_id]))
    }

    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a Nullifier directly from a base field element (for client use)
    pub fn from_base(base: pallas::Base) -> Self {
        Nullifier(base)
    }

    /// Convert into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

// ============================================================================
// COIN STRUCTURES (PRIVACY-FIRST - NO EC)
// ============================================================================

/// A coin - hash of coin attributes using Poseidon only (no EC)
/// This is the coin commitment that gets stored in the Merkle tree
/// Coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)
/// where pub = poseidon_hash(secret) is a field element, not EC point
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Coin(pallas::Base);

impl Coin {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Convert the Coin type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    /// Create a Coin from coin attributes
    /// Money V3: public key is poseidon_hash(secret) as a field element
    pub fn from_attributes(
        public_key: pallas::Base,
        value: u64,
        token_id: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        blind: pallas::Base,
    ) -> Self {
        let coin = poseidon_hash([
            public_key,
            pallas::Base::from(value),
            token_id,
            spend_hook,
            user_data,
            blind,
        ]);
        Coin(coin)
    }
}

/// Coin attributes (used in ZK circuits but NOT stored directly)
/// Money V3: public_key is a field element, not EC point
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CoinAttributes {
    /// Public key as field element: poseidon_hash(secret)
    pub public_key: pallas::Base,
    pub value: u64,
    pub token_id: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
    pub blind: pallas::Base,
}

impl CoinAttributes {
    pub fn to_coin(&self) -> Coin {
        Coin(poseidon_hash([
            self.public_key,
            pallas::Base::from(self.value),
            self.token_id,
            self.spend_hook,
            self.user_data,
            self.blind,
        ]))
    }
}

// ============================================================================
// INPUT/OUTPUT (TRANSACTION BUILDING BLOCKS - NO EC)
// ============================================================================

/// Input to a transaction - proves right to spend a coin
/// Money V3: value_commit is pallas::Base (Poseidon hash), not pallas::Point (Pedersen)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    /// Value commitment using Poseidon hash (not Pedersen)
    pub value_commit: pallas::Base,
    /// Commitment of the token ID (Poseidon hash)
    pub token_commit: pallas::Base,
    /// Nullifier - proves coin is not double-spent
    pub nullifier: Nullifier,
    /// Merkle root proving coin existed
    pub merkle_root: MerkleNode,
    /// Encrypted user data field
    pub user_data_enc: pallas::Base,
    /// Signature public key (Poseidon hash of secret, as field element)
    pub signature_public: pallas::Base,
    /// Value of the coin being spent
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
    /// Leaf position in Merkle tree (for proof generation)
    pub leaf_position: u64,
    /// Merkle path (for proof generation)
    pub merkle_path: Vec<MerkleNode>,
}

/// Output of a transaction - creates new coins
/// Money V3: value_commit is pallas::Base (Poseidon hash), not pallas::Point
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Output {
    /// Value commitment using Poseidon hash (not Pedersen)
    pub value_commit: pallas::Base,
    /// Commitment for token ID
    pub token_commit: pallas::Base,
    /// The newly created coin
    pub coin: Coin,
    /// AEAD encrypted note - only recipient can decrypt
    pub note: AeadEncryptedNote,
}

// ============================================================================
// FUNCTION PARAMETERS (MoneyV3 for DeFi tokens)
// ============================================================================

/// Parameters for TokenMintV1 - create a new token type
/// This is how stablecoins, wrapped tokens, etc. are created
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TokenMintParamsV1 {
    /// The initial coin minted with this token type
    pub coin: Coin,
    /// Value commitment for the initial mint
    pub value_commit: pallas::Base,
    /// Token ID (derived from auth_parent, user_data, blind)
    pub token_id: pallas::Base,
    /// Token authorization parent (bound in ZK proof)
    pub token_auth_parent: pallas::Base,
    /// Token ID commitment (hides token_id)
    pub token_commit: pallas::Base,
}

/// State update for TokenMintV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TokenMintUpdateV1 {
    pub token_id: pallas::Base,
    pub coin: Coin,
}

/// Parameters for AuthTokenMintV1 - authorize minting for existing token
/// Proves caller has authority to mint tokens of a specific token_id
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AuthTokenMintParamsV1 {
    /// Nullifier to prevent replay attacks
    pub nullifier: Nullifier,
    /// The minting authority public key (poseidon_hash of secret)
    pub mint_public: pallas::Base,
    /// Token ID being authorized
    pub token_id: pallas::Base,
    /// Merkle root proving token_id exists
    pub token_registry_root: MerkleNode,
}

/// State update for AuthTokenMintV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AuthTokenMintUpdateV1 {
    pub nullifier: Nullifier,
}

/// Parameters for MintV1 - mint tokens of existing token type
/// Requires proof of authorization (from AuthTokenMintV1)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintParamsV1 {
    /// Authorization proof from AuthTokenMintV1
    pub auth_proof: AuthProof,
    /// The newly minted coin
    pub coin: Coin,
    /// Value commitment
    pub value_commit: pallas::Base,
    /// The token ID being minted
    pub token_id: pallas::Base,
}

/// Authorization proof from previous AuthTokenMintV1 call
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AuthProof {
    /// Nullifier from auth call (to prevent reuse)
    pub nullifier: Nullifier,
    /// Public key of the authority
    pub mint_public: pallas::Base,
    /// Merkle root of token registry (proves token_id is authorized)
    pub token_registry_root: MerkleNode,
}

/// State update for MintV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintUpdateV1 {
    pub coin: Coin,
}

/// Parameters for BurnV1 - destroy tokens
/// Reveals nullifier to prove spending without revealing coin content
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnParamsV1 {
    /// Anonymous inputs being burned
    pub inputs: Vec<Input>,
}

/// State update for BurnV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
}

/// Parameters for TransferV1 - private token transfer
/// Atomic burn + mint to prevent value leakage
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
}

/// State update for TransferV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}

/// Parameters for OtcSwapV1 - atomic OTC token swap
/// Swaps tokens between two parties: inputs[0] -> outputs[1], inputs[1] -> outputs[0]
/// Uses the same burn + mint proof structure as TransferV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct OtcSwapParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
}

/// State update for OtcSwapV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct OtcSwapUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}