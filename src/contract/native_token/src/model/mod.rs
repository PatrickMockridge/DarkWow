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

//! NativeToken data models
//!
//! Privacy-first native token design following money_v2 patterns.
//! Uses Pedersen commitments for hidden values and nullifiers for double-spend prevention.

use dwow_sdk::{
    blockchain::BlockHeight,
    crypto::{note::AeadEncryptedNote, pasta_prelude::PrimeField, poseidon_hash, BaseBlind, Blind, FuncId, MerkleNode, PublicKey, TokenId},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

/// Nullifier definitions (for double-spend prevention)
pub mod nullifier;
pub use self::nullifier::Nullifier;

// ============================================================================
// TOKEN/SYMBOLIC CONSTANTS
// ============================================================================

/// DRKW token ID — the native consensus asset (↓mine).
/// DRKW is unique: it is the only token minted by coinbase, needs no
/// per-contract token ID, and SHALL NOT be counterfeited (enforced by
/// block proof per consensus-coinbase.md §2).
pub const DRKW_TOKEN_ID: TokenId = TokenId::DRKW;

/// DRKW token commitment — the canonical Poseidon hash of the native token
/// with zero blind. Used by all entrypoints that verify ↓denominate.
/// `tc = poseidon_hash([DRKW_TOKEN_ID.inner(), pallas::Base::zero()])`
/// = `poseidon_hash([zero(), zero()])`.
pub const DRKW_TOKEN_COMMITMENT: pallas::Base = pallas::Base::zero();
// Computed as poseidon_hash([zero(), zero()]) — lazily evaluated at first use
// since const poseidon_hash is not available at compile time.
// Use poseidon_hash([pallas::Base::zero(), pallas::Base::zero()]) to compute.

/// Maximum value per coin (prevent overflow)
pub const MAX_COIN_VALUE: u64 = 1_000_000_000_000;

// ============================================================================
// COIN STRUCTURES (PRIVACY-FIRST - following money_v2 pattern)
// ============================================================================

/// A coin - just the hash of coin attributes (like money_v2)
/// This is the coin commitment that gets stored in the Merkle tree
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Coin(pallas::Base);

impl Coin {
    /// Fixed canonical byte size: 32 bytes (pallas::Base repr)
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

    /// Create a Coin from coin attributes (same as money_v2::Coin)
    pub fn from_attributes(
        public_key: &PublicKey,
        value: u64,
        token_id: TokenId,
        spend_hook: FuncId,
        user_data: pallas::Base,
        blind: BaseBlind,
    ) -> Self {
        // PublicKey constructor rejects identity, so xy() is always Some
        let (pub_x, pub_y) = public_key.xy().expect("pk not identity");
        let coin = poseidon_hash([
            pub_x,
            pub_y,
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
/// This is the witness data that proves ownership.
///
/// Per spec §8.1: spend_hook SHALL be FuncId (↓gate), blind SHALL be BaseBlind.
/// These are not raw pallas::Base — they are distinct behavioral positions.
#[derive(Debug, Clone)]
pub struct CoinAttributes {
    pub version: u8,
    pub public_key: PublicKey,
    pub value: u64,
    pub token_id: TokenId,
    /// Spend hook — typed FuncId per spec §8.1 (↓gate barb).
    /// FuncId::none() for coins with no spend hook.
    pub spend_hook: FuncId,
    pub user_data: pallas::Base,
    /// Coin blind — typed BaseBlind per spec §8.1.
    pub blind: BaseBlind,
}

impl CoinAttributes {
    pub const ENCODED_SIZE: usize = 169;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.public_key.to_bytes());
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
        let version = data[0];
        let public_key = PublicKey::from_bytes(data[1..33].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("CoinAttributes: invalid public_key: {}", e)))?;
        let value = u64::from_le_bytes(data[33..41].try_into().unwrap());
        let token_id = TokenId::from_bytes(data[41..73].try_into().unwrap())
            .map_err(|_| ContractError::IoError("CoinAttributes: invalid token_id".into()))?;
        let spend_hook = FuncId::from_bytes(data[73..105].try_into().unwrap())
            .map_err(|_| ContractError::IoError("CoinAttributes: invalid spend_hook".into()))?;
        let user_data = Option::<pallas::Base>::from(pallas::Base::from_repr(data[105..137].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CoinAttributes: invalid user_data".into()))?;
        let blind = Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[137..169].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("CoinAttributes: invalid blind".into()))?);
        Ok(CoinAttributes { version, public_key, value, token_id, spend_hook, user_data, blind })
    }

    pub fn to_coin(&self) -> Coin {
        // PublicKey constructor rejects identity, so xy() is always Some
        let (pub_x, pub_y) = self.public_key.xy().expect("pk not identity");
        let coin = poseidon_hash([
            pub_x,
            pub_y,
            pallas::Base::from(self.value),
            self.token_id.inner(),
            self.spend_hook.inner(),
            self.user_data,
            self.blind.inner(),
        ]);
        Coin(coin)
    }
}

// ============================================================================
// INPUT/OUTPUT (TRANSACTION BUILDING BLOCKS - following money_v2 pattern)
// ============================================================================

/// Input to a transaction — proves right to spend a coin.
///
/// This struct contains ONLY on-chain fields that are serialized into the
/// transaction.  Private witness data for client-side ZK proof generation
/// lives in [`InputWitness`].
///
/// Native token uses Pedersen commitments for value (homomorphic).
#[derive(Debug, Clone)]
pub struct Input {
    /// Pedersen commitment of the value (homomorphic)
    pub value_commit: pallas::Point,
    /// Commitment of the token ID
    pub token_commit: pallas::Base,
    /// Nullifier - proves coin is not double-spent
    pub nullifier: Nullifier,
    /// Merkle root proving coin existed
    pub merkle_root: MerkleNode,
    /// Encrypted user data field
    pub user_data_enc: pallas::Base,
    /// Spend hook (ZK circuit public input — constrains coin commitment)
    /// Typed FuncId per spec §8.1 (↓gate barb).
    pub spend_hook: FuncId,
    /// Signature public key
    pub signature_public: PublicKey,
}

impl Input {
    pub const ENCODED_SIZE: usize = 224;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.merkle_root.to_bytes());
        buf.extend_from_slice(&self.user_data_enc.to_repr());
        buf.extend_from_slice(&self.spend_hook.to_bytes());
        buf.extend_from_slice(&self.signature_public.to_bytes());
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
        let nullifier = Nullifier::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Input: invalid nullifier: {}", e)))?;
        let merkle_root = MerkleNode::from_bytes(data[96..128].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("Input: invalid merkle_root".into()))?;
        let user_data_enc = Option::<pallas::Base>::from(pallas::Base::from_repr(data[128..160].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("Input: invalid user_data_enc".into()))?;
        let spend_hook = FuncId::from_bytes(data[160..192].try_into().unwrap())
            .map_err(|_| ContractError::IoError("Input: invalid spend_hook".into()))?;
        let signature_public = PublicKey::from_bytes(data[192..224].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Input: invalid signature_public: {}", e)))?;
        Ok(Input { value_commit, token_commit, nullifier, merkle_root, user_data_enc, spend_hook, signature_public })
    }
}

/// Client-side witness data for ZK proof generation (never serialized on-chain).
#[derive(Debug, Clone)]
pub struct InputWitness {
    /// Value of the coin being spent
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind — typed BaseBlind per spec §8.1.
    pub coin_blind: BaseBlind,
    /// Leaf position in Merkle tree (for proof generation)
    pub leaf_position: u64,
    /// Merkle path (for proof generation)
    pub merkle_path: Vec<MerkleNode>,
}

/// Output of a transaction - creates new coins (money_v2 style)
#[derive(Debug, Clone)]
pub struct Output {
    /// Pedersen commitment for value (homomorphic)
    pub value_commit: pallas::Point,
    /// Commitment for token ID
    pub token_commit: pallas::Base,
    /// The newly created coin
    pub coin: Coin,
    /// Nullifier for this output coin: nf = poseidon_hash(coin_secret, coin)
    pub nullifier: Nullifier,
    /// AEAD encrypted note - only recipient can decrypt
    pub note: AeadEncryptedNote,
}

impl Output {
    pub fn encode(&self) -> Vec<u8> {
        let note_bytes = dwow_serial::serialize(&self.note);
        let cap = 128 + 2 + note_bytes.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.coin.encode());
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&(note_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(&note_bytes);
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
        let nullifier = Nullifier::from_bytes(data[96..128].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Output: invalid nullifier: {}", e)))?;
        let note_len = u16::from_le_bytes(data[128..130].try_into().unwrap()) as usize;
        let expected = 130 + note_len;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "Output: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let note = dwow_serial::deserialize(&data[130..130 + note_len])
            .map_err(|e| ContractError::IoError(format!("Output: invalid note: {:?}", e)))?;
        Ok(Output { value_commit, token_commit, coin, nullifier, note })
    }
}

/// Clear input (for genesis/rewards - no privacy needed)
#[derive(Debug, Clone)]
pub struct ClearInput {
    /// Input value
    pub value: u64,
    /// Input token ID
    pub token_id: pallas::Base,
    /// Value blinding factor
    pub value_blind: Blind<pallas::Scalar>,
    /// Token blinding factor — typed BaseBlind per spec §8.1.
    pub token_blind: BaseBlind,
    /// Signature public key
    pub signature_public: PublicKey,
}

impl ClearInput {
    pub const ENCODED_SIZE: usize = 168;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.token_id.to_repr());
        buf.extend_from_slice(&self.value_blind.inner().to_repr());
        buf.extend_from_slice(&self.token_blind.inner().to_repr());
        buf.extend_from_slice(&self.signature_public.to_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ClearInput: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let value = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[8..40].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClearInput: invalid token_id".into()))?;
        let value_blind = Blind(Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(data[40..72].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClearInput: invalid value_blind".into()))?);
        let token_blind = Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[72..104].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClearInput: invalid token_blind".into()))?);
        let signature_public = PublicKey::from_bytes(data[104..136].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("ClearInput: invalid signature_public: {}", e)))?;
        Ok(ClearInput { value, token_id, value_blind, token_blind, signature_public })
    }
}

// ============================================================================
// FUNCTION PARAMETERS (matching money_v2 naming)
// ============================================================================

/// Parameters for FeeV1 - pay network fees (CONSENSUS CRITICAL)
#[derive(Debug, Clone)]
pub struct FeeParamsV1 {
    pub input: Input,
    pub output: Output,
    /// Blinding for fee value commitment
    pub fee_value_blind: pallas::Scalar,
    /// Blinding for fee token commitment — typed BaseBlind per spec §8.1.
    pub fee_token_blind: BaseBlind,
    /// Fee amount in native tokens (u64)
    pub fee: u64,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

impl FeeParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let input_bytes = self.input.encode();
        let output_bytes = self.output.encode();
        let cap = input_bytes.len() + output_bytes.len() + 136;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&input_bytes);
        buf.extend_from_slice(&output_bytes);
        buf.extend_from_slice(&self.fee_value_blind.to_repr());
        buf.extend_from_slice(&self.fee_token_blind.inner().to_repr());
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let input = Input::decode(data)?;
        let input_len = Input::ENCODED_SIZE;
        if data.len() < input_len + 130 {
            return Err(ContractError::IoError(format!(
                "FeeParamsV1: expected at least {} bytes, got {}", input_len + 130, data.len()
            )));
        }
        let output = Output::decode(&data[input_len..])?;
        let output_bytes = output.encode();
        let pos = input_len + output_bytes.len();
        if data.len() < pos + 136 {
            return Err(ContractError::IoError(format!(
                "FeeParamsV1: expected at least {} bytes, got {}", pos + 136, data.len()
            )));
        }
        let fee_value_blind = Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("FeeParamsV1: invalid fee_value_blind".into()))?;
        let fee_token_blind = Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("FeeParamsV1: invalid fee_token_blind".into()))?);
        let fee = u64::from_le_bytes(data[pos+64..pos+72].try_into().unwrap());
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+72..pos+104].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("FeeParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+104..pos+136].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("FeeParamsV1: invalid tx_nonce".into()))?;
        Ok(FeeParamsV1 { input, output, fee_value_blind, fee_token_blind, fee, tx_binding, tx_nonce })
    }
}

/// State update for FeeV1
#[derive(Debug, Clone)]
pub struct FeeUpdateV1 {
    pub nullifier: Nullifier,
    pub coin: Coin,
    pub height: BlockHeight,
    pub fee: u64,
}

/// Parameters for PoWRewardV1 - distribute block rewards (CONSENSUS CRITICAL)
#[derive(Debug, Clone)]
pub struct PoWRewardParamsV1 {
    pub input: ClearInput,
    pub output: Output,
    /// Nullifier: nf = poseidon_hash(coin_secret, coin) — capability claim.
    /// The miner proves knowledge of the per-block derived key and publishes
    /// this nullifier to claim the block reward. Verified against nullifier SMT.
    pub nullifier: Nullifier,
    /// Expected cumulative total supply at this block height.
    pub expected_cumulative_supply: u64,
    /// Previous cumulative value commitment (Pedersen point: S_{H-1}).
    /// The ZK circuit constrains S_H = S_{H-1} + coin_value_commit,
    /// creating a verifiable cumulative supply chain from genesis to tip.
    pub old_cumulative_commit: pallas::Point,
    /// Previous cumulative blind (scalar sum of all coinbase blinds).
    pub old_cumulative_blind: pallas::Scalar,
    /// New cumulative value commitment (Pedersen point: S_H).
    /// Exposed as circuit public input (constrain_instance).
    pub new_cumulative_commit: pallas::Point,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

impl PoWRewardParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let input_bytes = self.input.encode();
        let output_bytes = self.output.encode();
        let cap = input_bytes.len() + output_bytes.len() + 200;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&input_bytes);
        buf.extend_from_slice(&output_bytes);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.expected_cumulative_supply.to_le_bytes());
        buf.extend_from_slice(&self.old_cumulative_commit.to_bytes());
        buf.extend_from_slice(&self.old_cumulative_blind.to_repr());
        buf.extend_from_slice(&self.new_cumulative_commit.to_bytes());
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let input = ClearInput::decode(data)?;
        let input_len = ClearInput::ENCODED_SIZE;
        if data.len() < input_len + 130 {
            return Err(ContractError::IoError(format!(
                "PoWRewardParamsV1: expected at least {} bytes, got {}", input_len + 130, data.len()
            )));
        }
        let output = Output::decode(&data[input_len..])?;
        let output_bytes = output.encode();
        let pos = input_len + output_bytes.len();
        if data.len() < pos + 200 {
            return Err(ContractError::IoError(format!(
                "PoWRewardParamsV1: expected at least {} bytes, got {}", pos + 200, data.len()
            )));
        }
        let nullifier = Nullifier::from_bytes(data[pos..pos+32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("PoWRewardParamsV1: invalid nullifier: {}", e)))?;
        let expected_cumulative_supply = u64::from_le_bytes(data[pos+32..pos+40].try_into().unwrap());
        let old_cumulative_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[pos+40..pos+72].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PoWRewardParamsV1: invalid old_cumulative_commit".into()))?;
        let old_cumulative_blind = Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(data[pos+72..pos+104].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PoWRewardParamsV1: invalid old_cumulative_blind".into()))?;
        let new_cumulative_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[pos+104..pos+136].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PoWRewardParamsV1: invalid new_cumulative_commit".into()))?;
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+136..pos+168].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PoWRewardParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+168..pos+200].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("PoWRewardParamsV1: invalid tx_nonce".into()))?;
        Ok(PoWRewardParamsV1 { input, output, nullifier, expected_cumulative_supply, old_cumulative_commit, old_cumulative_blind, new_cumulative_commit, tx_binding, tx_nonce })
    }
}

/// State update for PoWRewardV1
#[derive(Debug, Clone)]
pub struct PoWRewardUpdateV1 {
    pub coin: Coin,
    pub height: BlockHeight,
    /// Cumulative total supply after this reward (for supply cap enforcement)
    pub new_total_supply: u64,
    /// Cumulative value commitment after this reward (Pedersen point: S_H).
    /// Persisted for the next block's S_{H-1} and externally verifiable.
    pub cumulative_value_commit: pallas::Point,
    /// Cumulative blind after this reward (scalar sum of all coinbase blinds).
    /// Persisted for the next block's circuit witness derivation.
    pub aggregate_blind: pallas::Scalar,
}

/// Parameters for TransferV1 - private token transfer (PRIVACY)
#[derive(Debug, Clone)]
pub struct TransferParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

impl TransferParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let input_cap = self.inputs.len() * Input::ENCODED_SIZE;
        let output_bytes: Vec<Vec<u8>> = self.outputs.iter().map(|o| o.encode()).collect();
        let output_cap: usize = output_bytes.iter().map(|b| b.len()).sum();
        let cap = 2 + input_cap + 2 + output_cap + output_bytes.len() * 2 + 64;
        let mut buf = Vec::with_capacity(cap);
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

/// Parameters for SpendV1 - spend with change (PRIVACY)
#[derive(Debug, Clone)]
pub struct SpendParamsV1 {
    pub input: Input,
    pub output: Output,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

impl SpendParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let input_bytes = self.input.encode();
        let output_bytes = self.output.encode();
        let cap = input_bytes.len() + output_bytes.len() + 64;
        let mut buf = Vec::with_capacity(cap);
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
            return Err(ContractError::IoError(format!("SpendParamsV1: expected at least {} bytes, got {}", pos + 64, data.len())));
        }
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("SpendParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("SpendParamsV1: invalid tx_nonce".into()))?;
        Ok(SpendParamsV1 { input, output, tx_binding, tx_nonce })
    }
}

/// State update for SpendV1
#[derive(Debug, Clone)]
pub struct SpendUpdateV1 {
    pub nullifier: Nullifier,
    pub coin: Coin,
}

/// Parameters for BurnV1 - destroy coins (Z-cash style burn)
#[derive(Debug, Clone)]
pub struct BurnParamsV1 {
    /// Anonymous inputs being burned
    pub inputs: Vec<Input>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

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

/// State update for BurnV1
#[derive(Debug, Clone)]
pub struct BurnUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
}

/// Parameters for FeeCollectV1 — collects accumulated fees for miner (CONSENSUS)
///
/// This is the "collection plate" — the final transaction in every block that forwards
/// all FeeV1 burns to the miner. Uses the dedicated FeeCollect_V1 ZK circuit
/// (12 witnesses, 7 public inputs, no cumulative supply chain — fees are
/// redistribution, not minting). Zero public key exposure; miner identity proven
/// via nullifier only (same o-cap model as PoWRewardV1).
#[derive(Debug, Clone)]
pub struct FeeCollectParamsV1 {
    /// Total fees accumulated in fees_db[height] for this block
    pub total_fees: u64,
    /// The fee coin output — pays the miner using the same pk_H as coinbase
    pub output: Output,
    /// Nullifier: nf = poseidon_hash(sk_H, fee_coin)
    pub nullifier: Nullifier,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

impl FeeCollectParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let output_bytes = self.output.encode();
        let cap = 8 + output_bytes.len() + 96;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.total_fees.to_le_bytes());
        buf.extend_from_slice(&output_bytes);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 105 { return Err(ContractError::IoError("FeeCollectParamsV1: too short".into())); }
        let total_fees = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let output = Output::decode(&data[8..])?;
        let output_bytes = output.encode();
        let pos = 8 + output_bytes.len();
        if data.len() < pos + 96 { return Err(ContractError::IoError(format!("FeeCollectParamsV1: expected at least {} bytes, got {}", pos + 96, data.len()))); }
        let nullifier = Nullifier::from_bytes(data[pos..pos+32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("FeeCollectParamsV1: invalid nullifier: {}", e)))?;
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("FeeCollectParamsV1: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+64..pos+96].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("FeeCollectParamsV1: invalid tx_nonce".into()))?;
        Ok(FeeCollectParamsV1 { total_fees, output, nullifier, tx_binding, tx_nonce })
    }
}

/// State update for FeeCollectV1
///
/// Per consensus-coinbase.md §3.8: the claim nullifier is NOT stored in the
/// contract nullifiers_db — it equals the future spend nullifier and would
/// make the fee coin born-unspendable (same model as PoWRewardV1's empty
/// SMT batch). Claim-replay prevention: zero-claim rejection (check #1),
/// pot zeroing, Phase 0.5 structural rules, host-level nullifier tracking.
#[derive(Debug, Clone)]
pub struct FeeCollectUpdateV1 {
    /// The fee coin created for the miner
    pub coin: Coin,
    /// Block height (must match verifying block height)
    pub height: BlockHeight,
    /// Total fees collected (must match fees_db[height])
    pub total_fees: u64,
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE — BRIDGE UPDATE STRUCTS
// ============================================================================
// Per type-system.md §2.2: bytes round-trip across module boundaries is forbidden.
// Per §10.5: re-lift validation SHALL use named constructors (from_bytes).
// Per contract-wasm-type-system.md §3.1: SHALL NOT use derive macros for state values.
//
// These replace the former #[derive(SerialEncodable, SerialDecodable)] pattern.
// Each type has fixed byte layout with per-field validating constructors.

impl FeeUpdateV1 {
    /// Fixed canonical byte size: nullifier(32) + coin(32) + height(8) + fee(8)
    pub const ENCODED_SIZE: usize = 80;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.coin.to_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "FeeUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let nullifier = Nullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!(
                "FeeUpdateV1: invalid nullifier: {}", e
            )))?;
        let coin_bytes: [u8; 32] = data[32..64].try_into().unwrap();
        let coin = Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(coin_bytes))
            .ok_or_else(|| ContractError::IoError("FeeUpdateV1: invalid coin".into()))?);
        let height =
            BlockHeight::from_le_bytes(data[64..72].try_into().unwrap());
        let fee = u64::from_le_bytes(data[72..80].try_into().unwrap());
        Ok(FeeUpdateV1 { nullifier, coin, height, fee })
    }
}

impl BurnUpdateV1 {
    /// Encode to canonical bytes with u8-prefixed nullifier count.
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1 + self.nullifiers.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.nullifiers.len() as u8);
        for nf in &self.nullifiers {
            buf.extend_from_slice(&nf.to_bytes());
        }
        buf
    }

    /// Decode from canonical bytes with per-nullifier validation.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() {
            return Err(ContractError::IoError(
                "BurnUpdateV1: empty data".into()
            ));
        }
        let count = data[0] as usize;
        let expected = 1 + count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "BurnUpdateV1: expected {} bytes for {} nullifiers, got {}",
                expected, count, data.len()
            )));
        }
        let mut nullifiers = Vec::with_capacity(count);
        for i in 0..count {
            let start = 1 + i * 32;
            let nf = Nullifier::from_bytes(
                data[start..start + 32].try_into().unwrap(),
            )
            .map_err(|e| ContractError::IoError(format!(
                "BurnUpdateV1: invalid nullifier[{}]: {}", i, e
            )))?;
            nullifiers.push(nf);
        }
        Ok(BurnUpdateV1 { nullifiers })
    }
}

impl TransferUpdateV1 {
    /// Encode to canonical bytes: u8 nullifier count + N*32 + u8 coin count + N*32.
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.nullifiers.len() * 32 + self.coins.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.nullifiers.len() as u8);
        for nf in &self.nullifiers {
            buf.extend_from_slice(&nf.to_bytes());
        }
        buf.push(self.coins.len() as u8);
        for coin in &self.coins {
            buf.extend_from_slice(&coin.to_bytes());
        }
        buf
    }

    /// Decode from canonical bytes with per-element validation.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 {
            return Err(ContractError::IoError(
                "TransferUpdateV1: data too short".into()
            ));
        }
        let nf_count = data[0] as usize;
        let nf_end = 1 + nf_count * 32;
        if data.len() < nf_end + 1 {
            return Err(ContractError::IoError(format!(
                "TransferUpdateV1: expected at least {} bytes for {} nullifiers, got {}",
                nf_end + 1, nf_count, data.len()
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
            let nf = Nullifier::from_bytes(
                data[start..start + 32].try_into().unwrap(),
            )
            .map_err(|e| ContractError::IoError(format!(
                "TransferUpdateV1: invalid nullifier[{}]: {}", i, e
            )))?;
            nullifiers.push(nf);
        }
        let mut coins = Vec::with_capacity(coin_count);
        for i in 0..coin_count {
            let start = nf_end + 1 + i * 32;
            let coin_bytes: [u8; 32] = data[start..start + 32].try_into().unwrap();
            let coin = Coin(
                Option::<pallas::Base>::from(pallas::Base::from_repr(coin_bytes))
                    .ok_or_else(|| ContractError::IoError(format!(
                        "TransferUpdateV1: invalid coin[{}]", i
                    )))?
            );
            coins.push(coin);
        }
        Ok(TransferUpdateV1 { nullifiers, coins })
    }
}

impl SpendUpdateV1 {
    /// Fixed canonical byte size: nullifier(32) + coin(32)
    pub const ENCODED_SIZE: usize = 64;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.nullifier.to_bytes());
        buf.extend_from_slice(&self.coin.to_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "SpendUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let nullifier = Nullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!(
                "SpendUpdateV1: invalid nullifier: {}", e
            )))?;
        let coin_bytes: [u8; 32] = data[32..64].try_into().unwrap();
        let coin = Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(coin_bytes))
            .ok_or_else(|| ContractError::IoError("SpendUpdateV1: invalid coin".into()))?);
        Ok(SpendUpdateV1 { nullifier, coin })
    }
}

impl PoWRewardUpdateV1 {
    /// Fixed canonical byte size: coin(32) + height(8) + supply(8) + point(32) + scalar(32)
    pub const ENCODED_SIZE: usize = 112;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.coin.to_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&self.new_total_supply.to_le_bytes());
        buf.extend_from_slice(&self.cumulative_value_commit.to_bytes());
        buf.extend_from_slice(&self.aggregate_blind.to_repr());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PoWRewardUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let coin_bytes: [u8; 32] = data[0..32].try_into().unwrap();
        let coin = Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(coin_bytes))
            .ok_or_else(|| ContractError::IoError("PoWRewardUpdateV1: invalid coin".into()))?);
        let height =
            BlockHeight::from_le_bytes(data[32..40].try_into().unwrap());
        let new_total_supply = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let cumulative_value_commit = Option::<pallas::Point>::from(
            pallas::Point::from_bytes(data[48..80].try_into().unwrap()),
        )
        .ok_or_else(|| {
            ContractError::IoError(
                "PoWRewardUpdateV1: invalid cumulative_value_commit".into(),
            )
        })?;
        let aggregate_blind = Option::<pallas::Scalar>::from(
            pallas::Scalar::from_repr(data[80..112].try_into().unwrap()),
        )
        .ok_or_else(|| {
            ContractError::IoError("PoWRewardUpdateV1: invalid aggregate_blind".into())
        })?;
        Ok(PoWRewardUpdateV1 {
            coin,
            height,
            new_total_supply,
            cumulative_value_commit,
            aggregate_blind,
        })
    }
}

impl FeeCollectUpdateV1 {
    /// Fixed canonical byte size: coin(32) + height(8) + total_fees(8)
    pub const ENCODED_SIZE: usize = 48;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.coin.to_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&self.total_fees.to_le_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "FeeCollectUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let coin_bytes: [u8; 32] = data[0..32].try_into().unwrap();
        let coin = Coin(Option::<pallas::Base>::from(pallas::Base::from_repr(coin_bytes))
            .ok_or_else(|| ContractError::IoError("FeeCollectUpdateV1: invalid coin".into()))?);
        let height =
            BlockHeight::from_le_bytes(data[32..40].try_into().unwrap());
        let total_fees = u64::from_le_bytes(data[40..48].try_into().unwrap());
        Ok(FeeCollectUpdateV1 { coin, height, total_fees })
    }
}