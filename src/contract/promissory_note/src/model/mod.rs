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
    crypto::{pasta_prelude::PrimeField, poseidon_hash, BaseBlind, ContractId, FuncId, MerkleNode, TokenId},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
    pub fn to_coin(&self) -> Coin {
        Coin(poseidon_hash([
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
    pub spend_hook: FuncId,
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

// ============================================================================
// FUNCTION PARAMETERS (PromissoryNote for DeFi tokens)
// ============================================================================

/// Parameters for RegisterTypeV1 - create a new token type
/// This is how stablecoins, wrapped tokens, etc. are created
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

/// State update for RegisterTypeV1
#[derive(Debug, Clone)]
pub struct TokenMintUpdateV1 {
    pub token_id: TokenId,
    pub coin: Coin,
    /// Token authority public key (poseidon_hash of mint_secret)
    pub token_auth_parent: pallas::Base,
}

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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

/// State update for IssueV1
#[derive(Debug, Clone)]
pub struct MintUpdateV1 {
    pub coin: Coin,
    pub token_id: TokenId,
    pub new_coin_count: u64,
}

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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnParamsV1 {
    /// Anonymous inputs being burned
    pub inputs: Vec<Input>,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

/// State update for RevokeV1
#[derive(Debug, Clone)]
pub struct BurnUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
}

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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

/// State update for TransferV1
#[derive(Debug, Clone)]
pub struct TransferUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}

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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

/// State update for RedeemV1
#[derive(Debug, Clone)]
pub struct RedeemUpdateV1 {
    pub nullifier: Nullifier,
    pub coin: Coin,
}

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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct OtcSwapParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// Transaction binding (poseidon_hash(tx_commitment, tx_nonce))
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

/// State update for OtcSwapV1
#[derive(Debug, Clone)]
pub struct OtcSwapUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}

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
