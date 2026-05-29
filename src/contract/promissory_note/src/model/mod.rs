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

//! Promissory Note data models
//!
//! Privacy-first design for DeFi tokens (wrapped, stablecoins, etc.)
//! Uses Pedersen commitments for additively homomorphic value conservation.
//!
//! ## Token Model
//!
//! - TokenMintV1: Creates a new token type (stablecoin, wrapped, etc.)
//! - MintV1: Mints tokens of an existing token type (proves backing capability)
//! - BurnV1: Burns tokens
//! - TransferV1: Private token transfer

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, MerkleNode},
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
/// Promissory Note uses poseidon_hash(secret, coin) for nullifier derivation
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
// COIN STRUCTURES (PRIVACY-FIRST - Pedersen value commitments)
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
    /// Promissory Note: public key is poseidon_hash(secret) as a field element
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
/// Promissory Note: public_key is a field element, not EC point
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
// INPUT/OUTPUT (TRANSACTION BUILDING BLOCKS)
// ============================================================================

/// Input to a transaction - proves right to spend a coin
/// An input (spent coin) in a PromissoryNote transaction.
///
/// This struct contains ONLY on-chain fields that are serialized into the
/// transaction.  Private witness data needed for client-side ZK proof
/// generation lives in [`InputWitness`].
///
/// Promissory Note uses Pedersen commitments for value (additively homomorphic).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    /// Pedersen commitment of the value (additively homomorphic)
    pub value_commit: pallas::Point,
    /// Commitment of the token ID (Poseidon hash)
    pub token_commit: pallas::Base,
    /// Nullifier - proves coin is not double-spent
    pub nullifier: Nullifier,
    /// Merkle root proving coin existed
    pub merkle_root: MerkleNode,
    /// Encrypted user data field
    pub user_data_enc: pallas::Base,
    /// Spend hook (ZK circuit public input — constrains coin commitment)
    pub spend_hook: pallas::Base,
    /// Signature public key (Poseidon hash of secret, as field element)
    pub signature_public: pallas::Base,
}

/// Client-side witness data for ZK proof generation.
///
/// These fields are NEVER serialized on-chain.  They are passed from the
/// client to the ZK prover alongside the [`Input`] struct.  The entrypoint
/// functions never access them.
#[derive(Debug, Clone)]
pub struct InputWitness {
    /// Value of the coin being spent
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
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
/// Promissory Note uses Pedersen commitments for value (additively homomorphic).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Output {
    /// Pedersen commitment for value (additively homomorphic)
    pub value_commit: pallas::Point,
    /// Commitment for token ID (now ZK-constrained via BlindOutputV1)
    pub token_commit: pallas::Base,
    /// The newly created coin
    pub coin: Coin,
    /// AEAD encrypted note - only recipient can decrypt
    pub note: AeadEncryptedNote,
    /// Spend hook — verified by circuit as public input
    pub spend_hook: pallas::Base,
}

// ============================================================================
// FUNCTION PARAMETERS (PromissoryNote for DeFi tokens)
// ============================================================================

/// Parameters for TokenMintV1 - create a new token type
/// This is how stablecoins, wrapped tokens, etc. are created
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TokenMintParamsV1 {
    /// The initial coin minted with this token type
    pub coin: Coin,
    /// Pedersen value commitment for the initial mint
    pub value_commit: pallas::Point,
    /// Token ID (derived from auth_parent, user_data, blind)
    pub token_id: pallas::Base,
    /// Token authorization parent (bound in ZK proof)
    pub token_auth_parent: pallas::Base,
    /// Token ID commitment (hides token_id)
    pub token_commit: pallas::Base,
    /// Spend hook for the initial coin
    pub spend_hook: pallas::Base,
}

/// State update for TokenMintV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TokenMintUpdateV1 {
    pub token_id: pallas::Base,
    pub coin: Coin,
    /// Token authority public key (poseidon_hash of mint_secret)
    /// Stored on-chain as the current capability holder for rotation
    pub token_auth_parent: pallas::Base,
}

/// The token registry maps token_id → token_auth_parent (the current mint authority's
/// public key). This is a capability datum, not metadata — it's what the rotation
/// ZK proof validates against (old_mint_public must match the stored authority).
/// No TokenInfo struct is needed; the registry value is just the serialized
/// pallas::Base authority key.

/// Parameters for MintV1 - mint tokens of existing token type
/// Proves knowledge of the backing secret directly against stored token_auth_parent
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintParamsV1 {
    /// The newly minted coin
    pub coin: Coin,
    /// Pedersen value commitment
    pub value_commit: pallas::Point,
    /// The token ID being minted
    pub token_id: pallas::Base,
    /// Token registry Merkle root (proves token exists)
    pub token_registry_root: MerkleNode,
    /// Backing capability public key (poseidon_hash of backing secret)
    pub mint_public: pallas::Base,
    /// Spend hook for the newly minted coin
    pub spend_hook: pallas::Base,
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

/// Parameters for RedeemV1 - redeem a coin, destroying its monetary value
///
/// RedeemV1 is the lifecycle counterpart to TokenMintV1: where 0x00 opens the
/// lifecycle (a promise is made), 0x01 closes it (the promise is honored).
///
/// The input coin is burned (nullifier published, value destroyed). The output
/// is a zero-value receipt coin — cryptographic proof that redemption occurred.
/// The receipt is non-transferable (spend_hook = issuer contract) and serves as
/// both the redeemer's proof and the issuer's on-chain book-keeping record.
///
/// Value conservation is NOT enforced: redemption IS value destruction in the
/// PromissoryNote system. The issuer fulfills the promise by releasing the
/// underlying asset (collateral, native token on another chain, etc.).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RedeemParamsV1 {
    /// Coin being redeemed (burn proof)
    pub input: Input,
    /// Receipt coin (blind output proof, value = 0)
    pub output: Output,
}

/// State update for RedeemV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RedeemUpdateV1 {
    pub nullifier: Nullifier,
    pub coin: Coin,
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

// ============================================================================
// SPEND HOOK CALLBACK
// ============================================================================

/// Payload delivered to the spend_hook target contract during BurnV1.
/// Contains all public Burn_V1 data so the target can verify the burn.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnSpendHookPayload {
    /// Contract ID of the PromissoryNote instance that performed the burn
    pub caller_contract_id: ContractId,
    /// Nullifiers of burned coins
    pub nullifiers: Vec<pallas::Base>,
    /// Token commitments of burned coins
    pub token_commits: Vec<pallas::Base>,
    /// Value commitments of burned coins
    pub value_commits: Vec<pallas::Point>,
    /// Encrypted user data of burned coins
    pub user_data_encs: Vec<pallas::Base>,
}
