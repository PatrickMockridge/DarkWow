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
    crypto::{note::AeadEncryptedNote, pasta_prelude::PrimeField, poseidon_hash, BaseBlind, Blind, FuncId, MerkleNode, PublicKey, TokenId},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Nullifier definitions (for double-spend prevention)
pub mod nullifier;
pub use self::nullifier::Nullifier;

// ============================================================================
// TOKEN/SYMBOLIC CONSTANTS
// ============================================================================

/// DARK token ID (native token)
pub const DRKW_TOKEN_ID: TokenId = TokenId::DRKW;

/// Maximum value per coin (prevent overflow)
pub const MAX_COIN_VALUE: u64 = 1_000_000_000_000;

// ============================================================================
// COIN STRUCTURES (PRIVACY-FIRST - following money_v2 pattern)
// ============================================================================

/// A coin - just the hash of coin attributes (like money_v2)
/// This is the coin commitment that gets stored in the Merkle tree
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

/// Clear input (for genesis/rewards - no privacy needed)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

// ============================================================================
// FUNCTION PARAMETERS (matching money_v2 naming)
// ============================================================================

/// Parameters for FeeV1 - pay network fees (CONSENSUS CRITICAL)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

/// State update for FeeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FeeUpdateV1 {
    pub nullifier: Nullifier,
    pub coin: Coin,
    pub height: u32,
    pub fee: u64,
}

/// Parameters for GenesisMintV1 - create initial supply (CONSENSUS CRITICAL)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GenesisMintParamsV1 {
    /// Clear input (no privacy needed for genesis)
    pub input: ClearInput,
    /// Anonymous outputs
    pub outputs: Vec<Output>,
}

/// State update for GenesisMintV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GenesisMintUpdateV1 {
    pub coins: Vec<Coin>,
}

/// Parameters for PoWRewardV1 - distribute block rewards (CONSENSUS CRITICAL)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

/// State update for PoWRewardV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PoWRewardUpdateV1 {
    pub coin: Coin,
    pub height: u32,
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferParamsV1 {
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

/// State update for TransferV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}

/// Parameters for SpendV1 - spend with change (PRIVACY)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpendParamsV1 {
    pub input: Input,
    pub output: Output,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

/// State update for SpendV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpendUpdateV1 {
    pub nullifier: Nullifier,
    pub coin: Coin,
}

/// Parameters for MintV1 - create new coins (Z-cash style mint)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintParamsV1 {
    /// The newly minted coin
    pub coin: Coin,
    /// Pedersen commitment of the value
    pub value_commit: pallas::Point,
    /// Commitment of the token ID
    pub token_commit: pallas::Base,
    /// Transaction binding: poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce: unique per transaction
    pub tx_nonce: pallas::Base,
}

/// State update for MintV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintUpdateV1 {
    pub coin: Coin,
}

/// Parameters for BurnV1 - destroy coins (Z-cash style burn)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnParamsV1 {
    /// Anonymous inputs being burned
    pub inputs: Vec<Input>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// State update for BurnV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
}