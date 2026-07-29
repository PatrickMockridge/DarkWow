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
//! - RegisterTypeV1: Creates a new token type (stablecoin, wrapped, etc.)
//! - IssueV1: Mints tokens of an existing token type (proves backing capability)
//! - RevokeV1: Burns tokens
//! - TransferV1: Private token transfer

use dwow_sdk::{
    crypto::{constants::DRK_POSEIDON_DOMAIN_COIN_COMMIT, pasta_prelude::PrimeField, poseidon_hash, BaseBlind, ContractId, FuncId, MerkleNode, TokenId},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

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
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Nullifier(pallas::Base);

impl Nullifier {
    pub const ENCODED_SIZE: usize = 32;

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

    /// Create from bytes with validation.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(bytes).into() {
            Some(v) => Ok(Nullifier(v)),
            None => Err(ContractError::IoError("Nullifier: invalid field element".into())),
        }
    }

    /// Convert into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Nullifier: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        Self::from_bytes(data[0..32].try_into().unwrap())
    }
}

// ============================================================================
// COIN STRUCTURES (PRIVACY-FIRST - Pedersen value commitments)
// ============================================================================

/// A coin - hash of coin attributes using Poseidon only (no EC)
/// This is the coin commitment that gets stored in the Merkle tree
/// Coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)
/// where pub = poseidon_hash(secret) is a field element, not EC point
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Coin(pallas::Base);

impl Coin {
    pub const ENCODED_SIZE: usize = 32;

    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Convert the Coin type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Coin: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let inner = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Coin: invalid field element".into()))?;
        Ok(Coin(inner))
    }

    /// Create a Coin from coin attributes
    /// Promissory Note: public key is poseidon_hash(secret) as a field element
    pub fn from_attributes(
        public_key: pallas::Base,
        value: u64,
        token_id: TokenId,
        spend_hook: FuncId,
        user_data: pallas::Base,
        blind: BaseBlind,
    ) -> Self {
        let coin = poseidon_hash([
            public_key,
            pallas::Base::from(value),
            token_id.inner(),
            spend_hook.inner(),
            user_data,
            blind.inner(),
        ]);
        Coin(coin)
    }
}

/// Coin attributes (used in ZK circuits but NOT stored directly)
/// Promissory Note: public_key is a field element, not EC point
#[derive(Debug, Clone,)]
pub struct CoinAttributes {
    /// Public key as field element: poseidon_hash(secret)
    pub public_key: pallas::Base,
    pub value: u64,
    pub token_id: TokenId,
    pub spend_hook: FuncId,
    pub user_data: pallas::Base,
    pub blind: BaseBlind,
}

impl CoinAttributes {
    pub const ENCODED_SIZE: usize = 168;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.public_key.to_repr());
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.token_id.to_bytes());
        buf.extend_from_slice(&self.spend_hook.to_bytes());
        buf.extend_from_slice(&self.user_data.to_repr());
        buf.extend_from_slice(&self.blind.inner().to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "CoinAttributes: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let public_key = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CoinAttributes: invalid public_key".into()))?;
        let value = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let token_id = TokenId::from_bytes(data[40..72].try_into().unwrap())
            .map_err(|_| ContractError::IoError("CoinAttributes: invalid token_id".into()))?;
        let spend_hook = FuncId::from_bytes(data[72..104].try_into().unwrap())
            .map_err(|_| ContractError::IoError("CoinAttributes: invalid spend_hook".into()))?;
        let user_data = Option::<pallas::Base>::from(pallas::Base::from_repr(data[104..136].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CoinAttributes: invalid user_data".into()))?;
        let blind = dwow_sdk::crypto::Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[136..168].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CoinAttributes: invalid blind".into()))?);
        Ok(CoinAttributes { public_key, value, token_id, spend_hook, user_data, blind })
    }

    pub fn to_coin(&self) -> Coin {
        Coin(poseidon_hash([
            DRK_POSEIDON_DOMAIN_COIN_COMMIT,
            self.public_key,
            pallas::Base::from(self.value),
            self.token_id.inner(),
            self.spend_hook.inner(),
            self.user_data,
            self.blind.inner(),
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
#[derive(Debug, Clone,)]
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
    pub spend_hook: FuncId,
    /// Signature public key (Poseidon hash of secret, as field element)
    pub signature_public: pallas::Base,
}

impl Input {
    pub const ENCODED_SIZE: usize = 224;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.nullifier.encode());
        buf.extend_from_slice(&self.merkle_root.to_bytes());
        buf.extend_from_slice(&self.user_data_enc.to_repr());
        buf.extend_from_slice(&self.spend_hook.to_bytes());
        buf.extend_from_slice(&self.signature_public.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Input: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Input: invalid value_commit".into()))?;
        let token_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Input: invalid token_commit".into()))?;
        let nullifier = Nullifier::decode(&data[64..96])?;
        let merkle_root = MerkleNode::from_bytes(data[96..128].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("Input: invalid merkle_root".into()))?;
        let user_data_enc = Option::<pallas::Base>::from(pallas::Base::from_repr(data[128..160].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Input: invalid user_data_enc".into()))?;
        let spend_hook = FuncId::from_bytes(data[160..192].try_into().unwrap())
            .map_err(|_| ContractError::IoError("Input: invalid spend_hook".into()))?;
        let signature_public = Option::<pallas::Base>::from(pallas::Base::from_repr(data[192..224].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Input: invalid signature_public".into()))?;
        Ok(Input { value_commit, token_commit, nullifier, merkle_root, user_data_enc, spend_hook, signature_public })
    }
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
    pub token_id: TokenId,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: BaseBlind,
    /// Leaf position in Merkle tree (for proof generation)
    pub leaf_position: u64,
    /// Merkle path (for proof generation)
    pub merkle_path: Vec<MerkleNode>,
}

/// Output of a transaction - creates new coins
/// Promissory Note uses Pedersen commitments for value (additively homomorphic).
#[derive(Debug, Clone,)]
pub struct Output {
    /// Pedersen commitment for value (additively homomorphic)
    pub value_commit: pallas::Point,
    /// Commitment for token ID (now ZK-constrained via TransferV1)
    pub token_commit: pallas::Base,
    /// The newly created coin
    pub coin: Coin,
    /// AEAD encrypted note - only recipient can decrypt
    pub note: AeadEncryptedNote,
    /// Spend hook — verified by circuit as public input
    pub spend_hook: FuncId,
}

impl Output {
    pub fn encode(&self) -> Vec<u8> {
        let note_bytes = dwow_serial::serialize(&self.note);
        let cap = 130 + note_bytes.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.coin.encode());
        buf.extend_from_slice(&(note_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(&note_bytes);
        buf.extend_from_slice(&self.spend_hook.to_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 130 {
            return Err(ContractError::IoError(format!(
                "Output: expected at least 130 bytes, got {}", data.len()
            )));
        }
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Output: invalid value_commit".into()))?;
        let token_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Output: invalid token_commit".into()))?;
        let coin = Coin::decode(&data[64..96])?;
        let note_len = u16::from_le_bytes(data[96..98].try_into().unwrap()) as usize;
        let note_end = 98 + note_len;
        if data.len() < note_end + 32 {
            return Err(ContractError::IoError(format!(
                "Output: expected at least {} bytes, got {}", note_end + 32, data.len()
            )));
        }
        let note = dwow_serial::deserialize(&data[98..note_end])
            .map_err(|e| ContractError::IoError(format!("Output: invalid note: {:?}", e)))?;
        let spend_hook = FuncId::from_bytes(data[note_end..note_end + 32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("Output: invalid spend_hook".into()))?;
        Ok(Output { value_commit, token_commit, coin, note, spend_hook })
    }
}

// ============================================================================
// FUNCTION PARAMETERS (PromissoryNote for DeFi tokens)
// ============================================================================

/// Parameters for RegisterTypeV1 - create a new token type
/// This is how stablecoins, wrapped tokens, etc. are created
#[derive(Debug, Clone,)]
pub struct TokenMintParamsV1 {
    /// The initial coin minted with this token type
    pub coin: Coin,
    /// Pedersen value commitment for the initial mint
    pub value_commit: pallas::Point,
    /// Token ID (derived from auth_parent, user_data, blind)
    pub token_id: TokenId,
    /// Token authorization parent (bound in ZK proof)
    pub token_auth_parent: pallas::Base,
    /// Token ID commitment (hides token_id)
    pub token_commit: pallas::Base,
    /// Spend hook for the initial coin
    pub spend_hook: FuncId,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TokenMintParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TokenMintParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl TokenMintParamsV1 {
    pub const ENCODED_SIZE: usize = 256;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.coin.encode());
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.token_id.to_bytes());
        buf.extend_from_slice(&self.token_auth_parent.to_repr());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.spend_hook.to_bytes());
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "TokenMintParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let coin = Coin::decode(&data[0..32])?;
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TokenMintParamsV1: invalid value_commit".into()))?;
        let token_id = TokenId::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|_| ContractError::IoError("TokenMintParamsV1: invalid token_id".into()))?;
        let token_auth_parent = Option::<pallas::Base>::from(pallas::Base::from_repr(data[96..128].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TokenMintParamsV1: invalid token_auth_parent".into()))?;
        let token_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[128..160].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TokenMintParamsV1: invalid token_commit".into()))?;
        let spend_hook = FuncId::from_bytes(data[160..192].try_into().unwrap())
            .map_err(|_| ContractError::IoError("TokenMintParamsV1: invalid spend_hook".into()))?;
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[192..224].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TokenMintParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[224..256].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TokenMintParamsV1: invalid tx_nonce".into()))?;
        Ok(TokenMintParamsV1 { coin, value_commit, token_id, token_auth_parent, token_commit, spend_hook, tx_binding, tx_nonce })
    }
}

/// State update for RegisterTypeV1
#[derive(Debug, Clone)]
pub struct TokenMintUpdateV1 {
    pub token_id: TokenId,
    pub coin: Coin,
    /// Token authority public key (poseidon_hash of mint_secret)
    pub token_auth_parent: pallas::Base,
}

impl dwow_serial::Encodable for TokenMintUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TokenMintUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl TokenMintUpdateV1 {
    pub const ENCODED_SIZE: usize = 96; // 32 + 32 + 32
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.token_id.to_bytes());
        buf.extend_from_slice(&self.coin.to_bytes());
        buf.extend_from_slice(&self.token_auth_parent.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "TokenMintUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let token_id = TokenId::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("TokenMintUpdateV1: invalid token_id".into()))?;
        let coin = Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TokenMintUpdateV1: invalid coin".into()))?);
        let token_auth_parent = Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TokenMintUpdateV1: invalid token_auth_parent".into()))?;
        Ok(TokenMintUpdateV1 { token_id, coin, token_auth_parent })
    }
}

/// The token registry maps token_id → token_auth_parent (the current mint authority's
/// public key). This is a capability datum, not metadata — it's what the rotation
/// ZK proof validates against (old_mint_public must match the stored authority).
/// No TokenInfo struct is needed; the registry value is just the serialized
/// pallas::Base authority key.

/// Parameters for IssueV1 - mint tokens of existing token type
/// Proves knowledge of the backing secret directly against stored token_auth_parent
#[derive(Debug, Clone,)]
pub struct MintParamsV1 {
    /// The newly minted coin
    pub coin: Coin,
    /// Pedersen value commitment
    pub value_commit: pallas::Point,
    /// The token ID being minted
    pub token_id: TokenId,
    /// Token registry Merkle root (proves token exists)
    pub token_registry_root: MerkleNode,
    /// Backing capability public key (poseidon_hash of backing secret)
    pub mint_public: pallas::Base,
    /// Spend hook for the newly minted coin
    pub spend_hook: FuncId,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for MintParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for MintParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl MintParamsV1 {
    pub const ENCODED_SIZE: usize = 256;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.coin.encode());
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.token_id.to_bytes());
        buf.extend_from_slice(&self.token_registry_root.to_bytes());
        buf.extend_from_slice(&self.mint_public.to_repr());
        buf.extend_from_slice(&self.spend_hook.to_bytes());
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "MintParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let coin = Coin::decode(&data[0..32])?;
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("MintParamsV1: invalid value_commit".into()))?;
        let token_id = TokenId::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|_| ContractError::IoError("MintParamsV1: invalid token_id".into()))?;
        let token_registry_root = MerkleNode::from_bytes(data[96..128].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("MintParamsV1: invalid token_registry_root".into()))?;
        let mint_public = Option::<pallas::Base>::from(pallas::Base::from_repr(data[128..160].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("MintParamsV1: invalid mint_public".into()))?;
        let spend_hook = FuncId::from_bytes(data[160..192].try_into().unwrap())
            .map_err(|_| ContractError::IoError("MintParamsV1: invalid spend_hook".into()))?;
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[192..224].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("MintParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[224..256].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("MintParamsV1: invalid tx_nonce".into()))?;
        Ok(MintParamsV1 { coin, value_commit, token_id, token_registry_root, mint_public, spend_hook, tx_binding, tx_nonce })
    }
}

/// State update for IssueV1
#[derive(Debug, Clone)]
pub struct MintUpdateV1 {
    pub coin: Coin,
    pub token_id: TokenId,
    pub new_coin_count: u64,
}

impl dwow_serial::Encodable for MintUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for MintUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl MintUpdateV1 {
    pub const ENCODED_SIZE: usize = 72; // 32 + 32 + 8
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.coin.to_bytes());
        buf.extend_from_slice(&self.token_id.to_bytes());
        buf.extend_from_slice(&self.new_coin_count.to_le_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "MintUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let coin = Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("MintUpdateV1: invalid coin".into()))?);
        let token_id = TokenId::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|_| ContractError::IoError("MintUpdateV1: invalid token_id".into()))?;
        let new_coin_count = u64::from_le_bytes(data[64..72].try_into().unwrap());
        Ok(MintUpdateV1 { coin, token_id, new_coin_count })
    }
}

/// Parameters for RevokeV1 - destroy tokens
/// Reveals nullifier to prove spending without revealing coin content
#[derive(Debug, Clone,)]
pub struct BurnParamsV1 {
    /// Anonymous inputs being burned
    pub inputs: Vec<Input>,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for BurnParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BurnParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl BurnParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1 + self.inputs.len() * Input::ENCODED_SIZE + 64;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.inputs.len() as u8);
        for input in &self.inputs { buf.extend_from_slice(&input.encode()); }
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 65 { return Err(ContractError::IoError("BurnParamsV1: too short".into())); }
        let count = data[0] as usize;
        let mut pos = 1;
        let mut inputs = Vec::with_capacity(count);
        for i in 0..count {
            if data.len() < pos + Input::ENCODED_SIZE {
                return Err(ContractError::IoError(format!("BurnParamsV1: input[{}] truncated", i)));
            }
            inputs.push(Input::decode(&data[pos..pos + Input::ENCODED_SIZE])?);
            pos += Input::ENCODED_SIZE;
        }
        if data.len() < pos + 64 { return Err(ContractError::IoError("BurnParamsV1: missing trailing fields".into())); }
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("BurnParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("BurnParamsV1: invalid tx_nonce".into()))?;
        Ok(BurnParamsV1 { inputs, tx_binding, tx_nonce })
    }
}

/// State update for RevokeV1
#[derive(Debug, Clone)]
pub struct BurnUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
}

impl dwow_serial::Encodable for BurnUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BurnUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl BurnUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.nullifiers.len() * 32);
        buf.push(self.nullifiers.len() as u8);
        for nf in &self.nullifiers {
            buf.extend_from_slice(&nf.to_bytes());
        }
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() {
            return Err(ContractError::IoError("BurnUpdateV1: empty data".into()));
        }
        let count = data[0] as usize;
        let expected = 1 + count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "BurnUpdateV1: expected {} bytes for {} nullifiers, got {}", expected, count, data.len()
            )));
        }
        let mut nullifiers = Vec::with_capacity(count);
        for i in 0..count {
            let start = 1 + i * 32;
            let nf = Nullifier(Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[start..start + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| ContractError::IoError(format!("BurnUpdateV1: invalid nullifier[{}]", i)))?);
            nullifiers.push(nf);
        }
        Ok(BurnUpdateV1 { nullifiers })
    }
}

/// Parameters for TransferV1 - private token transfer
/// Atomic burn + mint to prevent value leakage
#[derive(Debug, Clone,)]
pub struct TransferParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TransferParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TransferParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl TransferParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let input_cap = self.inputs.len() * Input::ENCODED_SIZE;
        let output_bytes: Vec<Vec<u8>> = self.outputs.iter().map(|o| o.encode()).collect();
        let output_cap: usize = output_bytes.iter().map(|b| b.len()).sum();
        let mut buf = Vec::with_capacity(2 + input_cap + 2 + output_cap + output_bytes.len() * 2 + 64);
        buf.push(self.inputs.len() as u8);
        for input in &self.inputs { buf.extend_from_slice(&input.encode()); }
        buf.push(self.outputs.len() as u8);
        for ob in &output_bytes {
            buf.extend_from_slice(&(ob.len() as u16).to_le_bytes());
            buf.extend_from_slice(ob);
        }
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 3 { return Err(ContractError::IoError("TransferParamsV1: too short".into())); }
        let input_count = data[0] as usize;
        let mut pos = 1;
        let mut inputs = Vec::with_capacity(input_count);
        for i in 0..input_count {
            if data.len() < pos + Input::ENCODED_SIZE {
                return Err(ContractError::IoError(format!("TransferParamsV1: input[{}] truncated", i)));
            }
            inputs.push(Input::decode(&data[pos..pos + Input::ENCODED_SIZE])?);
            pos += Input::ENCODED_SIZE;
        }
        if data.len() < pos + 1 { return Err(ContractError::IoError("TransferParamsV1: missing output count".into())); }
        let output_count = data[pos] as usize;
        pos += 1;
        let mut outputs = Vec::with_capacity(output_count);
        for i in 0..output_count {
            if data.len() < pos + 2 { return Err(ContractError::IoError(format!("TransferParamsV1: output[{}] truncated", i))); }
            let out_len = u16::from_le_bytes(data[pos..pos+2].try_into().unwrap()) as usize;
            pos += 2;
            if data.len() < pos + out_len { return Err(ContractError::IoError(format!("TransferParamsV1: output[{}] data truncated", i))); }
            outputs.push(Output::decode(&data[pos..pos + out_len])?);
            pos += out_len;
        }
        if data.len() < pos + 64 { return Err(ContractError::IoError("TransferParamsV1: missing trailing fields".into())); }
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TransferParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("TransferParamsV1: invalid tx_nonce".into()))?;
        Ok(TransferParamsV1 { inputs, outputs, tx_binding, tx_nonce })
    }
}

/// State update for TransferV1
#[derive(Debug, Clone)]
pub struct TransferUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}

impl dwow_serial::Encodable for TransferUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TransferUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl TransferUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.nullifiers.len() * 32 + self.coins.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.nullifiers.len() as u8);
        for nf in &self.nullifiers { buf.extend_from_slice(&nf.to_bytes()); }
        buf.push(self.coins.len() as u8);
        for c in &self.coins { buf.extend_from_slice(&c.to_bytes()); }
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 {
            return Err(ContractError::IoError("TransferUpdateV1: data too short".into()));
        }
        let nf_count = data[0] as usize;
        let nf_end = 1 + nf_count * 32;
        if data.len() < nf_end + 1 {
            return Err(ContractError::IoError(format!(
                "TransferUpdateV1: expected at least {} bytes, got {}", nf_end + 1, data.len()
            )));
        }
        let coin_count = data[nf_end] as usize;
        let expected = nf_end + 1 + coin_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "TransferUpdateV1: expected {} bytes ({} nf + {} coins), got {}",
                expected, nf_count, coin_count, data.len()
            )));
        }
        let mut nullifiers = Vec::with_capacity(nf_count);
        for i in 0..nf_count {
            let start = 1 + i * 32;
            nullifiers.push(Nullifier(Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[start..start + 32].try_into().unwrap(),
            )).ok_or_else(|| ContractError::IoError(format!("TransferUpdateV1: invalid nullifier[{}]", i)))?));
        }
        let mut coins = Vec::with_capacity(coin_count);
        for i in 0..coin_count {
            let start = nf_end + 1 + i * 32;
            coins.push(Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[start..start + 32].try_into().unwrap(),
            )).ok_or_else(|| ContractError::IoError(format!("TransferUpdateV1: invalid coin[{}]", i)))?));
        }
        Ok(TransferUpdateV1 { nullifiers, coins })
    }
}

/// Parameters for RedeemV1 - redeem a coin, destroying its monetary value
///
/// RedeemV1 is the lifecycle counterpart to RegisterTypeV1: where 0x00 opens the
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
#[derive(Debug, Clone,)]
pub struct RedeemParamsV1 {
    /// Coin being redeemed (burn proof)
    pub input: Input,
    /// Receipt coin (blind output proof, value = 0)
    pub output: Output,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for RedeemParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RedeemParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl RedeemParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let input_bytes = self.input.encode();
        let output_bytes = self.output.encode();
        let mut buf = Vec::with_capacity(input_bytes.len() + output_bytes.len() + 64);
        buf.extend_from_slice(&input_bytes);
        buf.extend_from_slice(&output_bytes);
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let input = Input::decode(data)?;
        let pos = Input::ENCODED_SIZE;
        let output = Output::decode(&data[pos..])?;
        let output_bytes = output.encode();
        let pos = pos + output_bytes.len();
        if data.len() < pos + 64 {
            return Err(ContractError::IoError(format!("RedeemParamsV1: expected at least {} bytes, got {}", pos + 64, data.len())));
        }
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("RedeemParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("RedeemParamsV1: invalid tx_nonce".into()))?;
        Ok(RedeemParamsV1 { input, output, tx_binding, tx_nonce })
    }
}

/// State update for RedeemV1
#[derive(Debug, Clone)]
pub struct RedeemUpdateV1 {
    pub nullifier: Nullifier,
    pub coin: Coin,
}

impl dwow_serial::Encodable for RedeemUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RedeemUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl RedeemUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.coin.to_bytes());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "RedeemUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let nullifier = Nullifier(Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        )).ok_or_else(|| ContractError::IoError("RedeemUpdateV1: invalid nullifier".into()))?);
        let coin = Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[32..64].try_into().unwrap(),
        )).ok_or_else(|| ContractError::IoError("RedeemUpdateV1: invalid coin".into()))?);
        Ok(RedeemUpdateV1 { nullifier, coin })
    }
}

/// Parameters for OtcSwapV1 - atomic OTC token swap
/// Swaps tokens between two parties: inputs[0] -> outputs[1], inputs[1] -> outputs[0]
/// Uses the same burn + mint proof structure as TransferV1
#[derive(Debug, Clone,)]
pub struct OtcSwapParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for OtcSwapParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for OtcSwapParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl OtcSwapParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let input_cap = self.inputs.len() * Input::ENCODED_SIZE;
        let output_bytes: Vec<Vec<u8>> = self.outputs.iter().map(|o| o.encode()).collect();
        let output_cap: usize = output_bytes.iter().map(|b| b.len()).sum();
        let mut buf = Vec::with_capacity(2 + input_cap + 2 + output_cap + output_bytes.len() * 2 + 64);
        buf.push(self.inputs.len() as u8);
        for input in &self.inputs { buf.extend_from_slice(&input.encode()); }
        buf.push(self.outputs.len() as u8);
        for ob in &output_bytes {
            buf.extend_from_slice(&(ob.len() as u16).to_le_bytes());
            buf.extend_from_slice(ob);
        }
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 3 { return Err(ContractError::IoError("OtcSwapParamsV1: too short".into())); }
        let input_count = data[0] as usize;
        let mut pos = 1;
        let mut inputs = Vec::with_capacity(input_count);
        for i in 0..input_count {
            if data.len() < pos + Input::ENCODED_SIZE {
                return Err(ContractError::IoError(format!("OtcSwapParamsV1: input[{}] truncated", i)));
            }
            inputs.push(Input::decode(&data[pos..pos + Input::ENCODED_SIZE])?);
            pos += Input::ENCODED_SIZE;
        }
        if data.len() < pos + 1 { return Err(ContractError::IoError("OtcSwapParamsV1: missing output count".into())); }
        let output_count = data[pos] as usize;
        pos += 1;
        let mut outputs = Vec::with_capacity(output_count);
        for i in 0..output_count {
            if data.len() < pos + 2 { return Err(ContractError::IoError(format!("OtcSwapParamsV1: output[{}] truncated", i))); }
            let out_len = u16::from_le_bytes(data[pos..pos+2].try_into().unwrap()) as usize;
            pos += 2;
            if data.len() < pos + out_len { return Err(ContractError::IoError(format!("OtcSwapParamsV1: output[{}] data truncated", i))); }
            outputs.push(Output::decode(&data[pos..pos + out_len])?);
            pos += out_len;
        }
        if data.len() < pos + 64 { return Err(ContractError::IoError("OtcSwapParamsV1: missing trailing fields".into())); }
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("OtcSwapParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("OtcSwapParamsV1: invalid tx_nonce".into()))?;
        Ok(OtcSwapParamsV1 { inputs, outputs, tx_binding, tx_nonce })
    }
}

/// State update for OtcSwapV1
#[derive(Debug, Clone)]
pub struct OtcSwapUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}

impl dwow_serial::Encodable for OtcSwapUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for OtcSwapUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl OtcSwapUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.nullifiers.len() * 32 + self.coins.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.nullifiers.len() as u8);
        for nf in &self.nullifiers { buf.extend_from_slice(&nf.to_bytes()); }
        buf.push(self.coins.len() as u8);
        for c in &self.coins { buf.extend_from_slice(&c.to_bytes()); }
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 {
            return Err(ContractError::IoError("OtcSwapUpdateV1: data too short".into()));
        }
        let nf_count = data[0] as usize;
        let nf_end = 1 + nf_count * 32;
        if data.len() < nf_end + 1 {
            return Err(ContractError::IoError(format!(
                "OtcSwapUpdateV1: expected at least {} bytes, got {}", nf_end + 1, data.len()
            )));
        }
        let coin_count = data[nf_end] as usize;
        let expected = nf_end + 1 + coin_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "OtcSwapUpdateV1: expected {} bytes ({} nf + {} coins), got {}",
                expected, nf_count, coin_count, data.len()
            )));
        }
        let mut nullifiers = Vec::with_capacity(nf_count);
        for i in 0..nf_count {
            let start = 1 + i * 32;
            nullifiers.push(Nullifier(Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[start..start + 32].try_into().unwrap(),
            )).ok_or_else(|| ContractError::IoError(format!("OtcSwapUpdateV1: invalid nullifier[{}]", i)))?));
        }
        let mut coins = Vec::with_capacity(coin_count);
        for i in 0..coin_count {
            let start = nf_end + 1 + i * 32;
            coins.push(Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[start..start + 32].try_into().unwrap(),
            )).ok_or_else(|| ContractError::IoError(format!("OtcSwapUpdateV1: invalid coin[{}]", i)))?));
        }
        Ok(OtcSwapUpdateV1 { nullifiers, coins })
    }
}

// ============================================================================
// SPEND HOOK CALLBACK
// ============================================================================

/// Payload delivered to the spend_hook target contract during RevokeV1.
#[derive(Debug, Clone)]
pub struct BurnSpendHookPayload {
    pub caller_contract_id: ContractId,
    pub nullifiers: Vec<pallas::Base>,
    pub token_commits: Vec<pallas::Base>,
    pub value_commits: Vec<pallas::Point>,
    pub user_data_encs: Vec<pallas::Base>,
}

impl BurnSpendHookPayload {
    pub fn encode(&self) -> Vec<u8> {
        let n = self.nullifiers.len();
        let cap = 33 + n * 32 * 4; // cid(32) + 4 x [u8 count + n*32]
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.caller_contract_id.to_bytes());
        buf.push(n as u8);
        for v in &self.nullifiers { buf.extend_from_slice(&v.to_repr()); }
        buf.push(n as u8);
        for v in &self.token_commits { buf.extend_from_slice(&v.to_repr()); }
        buf.push(n as u8);
        for v in &self.value_commits { buf.extend_from_slice(&v.to_bytes()); }
        buf.push(n as u8);
        for v in &self.user_data_encs { buf.extend_from_slice(&v.to_repr()); }
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 33 {
            return Err(ContractError::IoError("BurnSpendHookPayload: data too short".into()));
        }
        let caller_contract_id = ContractId::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("BurnSpendHookPayload: invalid caller_contract_id".into()))?;
        let n = data[32] as usize;
        fn decode_vec_base(data: &[u8], start: usize, n: usize) -> Result<(Vec<pallas::Base>, usize), ContractError> {
            let mut v = Vec::with_capacity(n);
            for i in 0..n {
                let s = start + i * 32;
                v.push(Option::<pallas::Base>::from(pallas::Base::from_repr(
                    data[s..s + 32].try_into().unwrap(),
                )).ok_or_else(|| ContractError::IoError("BurnSpendHookPayload: invalid field".into()))?);
            }
            Ok((v, start + n * 32))
        }
        fn decode_vec_point(data: &[u8], start: usize, n: usize) -> Result<(Vec<pallas::Point>, usize), ContractError> {
            let mut v = Vec::with_capacity(n);
            for i in 0..n {
                let s = start + i * 32;
                v.push(Option::<pallas::Point>::from(pallas::Point::from_bytes(
                    data[s..s + 32].try_into().unwrap(),
                )).ok_or_else(|| ContractError::IoError("BurnSpendHookPayload: invalid point".into()))?);
            }
            Ok((v, start + n * 32))
        }
        let (nullifiers, pos) = decode_vec_base(data, 33, n)?;
        let (token_commits, pos) = decode_vec_base(data, pos + 1, data[pos] as usize)?;
        let (value_commits, pos) = decode_vec_point(data, pos + 1, data[pos] as usize)?;
        let (user_data_encs, _) = decode_vec_base(data, pos + 1, data[pos] as usize)?;
        Ok(BurnSpendHookPayload { caller_contract_id, nullifiers, token_commits, value_commits, user_data_encs })
    }
}
