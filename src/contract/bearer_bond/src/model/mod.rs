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

//! Bearer Bond data models — Fixed-Interest Staking Model
//!
//! A stake coin is a tradeable capital position. The holder provides capital
//! to the issuer and earns a fixed interest rate set at series creation.
//! Interest is computed deterministically from on-chain state — no issuer
//! reporting is needed and the holder's privacy is preserved.
//!
//! Maturity is ZK-committed in the coin commitment, making it a
//! cryptographically bound property of the bond token.
//!
//! ## Lifecycle
//!
//! - IssueStakeV1 (0x00): Issuer creates staking pool, sets terms, receives
//!   capital, mints stake coins to the staker.
//! - TransferStakeV1 (0x01): Holder transfers stake position to new holder.
//!   Unclaimed interest travels with the coin — the new coin
//!   preserves `last_claim_block`.
//! - RequestInterestV1 (0x02): Holder requests interest payment (prove ownership).
//! - PayInterestV1 (0x08): Issuer pays a pending interest claim.
//!   Stake coin persists (not consumed).
//! - EmergencyUnstakeV1 (0x03): Holder exits before maturity when coverage
//!   falls below the minimum threshold.
//! - UnstakeV1 (0x04): Burn stake coin, receive principal plus any unclaimed
//!   interest back. Enforced at or after maturity.
//! - BurnStakeV1 (0x05): Issuer retires staking pool.

use dwow_sdk::{
    crypto::{pasta_prelude::{Group, PrimeField}, ContractId, MerkleNode},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Minimum claim value (1 unit — prevents dust claims)
pub const DEFAULT_MIN_CLAIM: u64 = 1;

/// Maximum principal value (prevent overflow)
pub const MAX_PRINCIPAL: u64 = 1_000_000_000_000;

// ============================================================================
// COIN ATTRIBUTES (for ZK circuit coin commitment)
// ============================================================================

/// Coin attributes that the ZK circuits (Burn_V1, BlindOutput_V1, Redeem_V1)
/// commit to. The coin commitment is:
/// `poseidon_hash([public_key, value, token_id, spend_hook, user_data, blind, maturity_block])`
///
/// Maturity is ZK-committed so it becomes a cryptographically bound property
/// of the bond token — the issuer cannot alter it after issuance.
///
/// Principal, last_claim_block, and issuer_contract remain as plaintext on
/// `BondCoin` since they don't need cryptographic binding for security.
#[derive(Debug, Clone)]
pub struct CoinAttributes {
    /// Poseidon hash of the owner's secret
    pub public_key: pallas::Base,
    /// Coin value
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blinding factor
    pub blind: pallas::Base,
    /// Block height when stake matures (ZK-committed)
    pub maturity_block: u64,
}

impl CoinAttributes {
    /// Compute the coin commitment (Poseidon hash of all attributes).
    pub fn to_coin(&self) -> pallas::Base {
        dwow_sdk::crypto::poseidon_hash([
            self.public_key,
            pallas::Base::from(self.value),
            self.token_id,
            self.spend_hook,
            self.user_data,
            self.blind,
            pallas::Base::from(self.maturity_block),
        ])
    }
}

// ============================================================================
// NULLIFIER
// ============================================================================

/// Nullifier for double-spend prevention.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Nullifier(pallas::Base);

impl Nullifier {
    pub fn new(secret: pallas::Base, coin: pallas::Base) -> Self {
        Nullifier(dwow_sdk::crypto::poseidon_hash([secret, coin]))
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn from_base(base: pallas::Base) -> Self {
        Nullifier(base)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(b);
        pallas::Base::from_repr(arr).into_option().map(Self)
    }
}

// ============================================================================
// BOND SERIES INFO
// ============================================================================

/// Status of a bond series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SeriesStatus {
    /// Series is active — stakes, transfers, and interest claims are allowed
    Active = 0,
    /// Series has been voided due to coverage failure — only emergency unstake allowed
    Voided = 1,
    /// Series has reached maturity — only unstake allowed
    Matured = 2,
}

impl TryFrom<u8> for SeriesStatus {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(SeriesStatus::Active),
            1 => Ok(SeriesStatus::Voided),
            2 => Ok(SeriesStatus::Matured),
            _ => Err(ContractError::IoError(format!("Invalid SeriesStatus: {}", b))),
        }
    }
}

/// Per-series configuration stored in the `bonds_info` tree.
///
/// Keyed by `poseidon_hash(series_token_id)`.
#[derive(Debug, Clone)]
pub struct BondSeriesInfo {
    /// Token ID of the staking pool series
    pub series_token_id: pallas::Base,
    /// Annual interest rate in basis points (e.g. 500 = 5%)
    pub interest_rate_bps: u64,
    /// Block height when the series matures
    pub maturity_block: u64,
    /// Current status of the series
    pub status: SeriesStatus,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// Total staked principal across all coins in this series
    pub total_staked: u64,
}

impl BondSeriesInfo {
    pub const ENCODED_SIZE: usize = 89;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(89);
        b.extend_from_slice(&self.series_token_id.to_repr());
        b.extend_from_slice(&self.interest_rate_bps.to_le_bytes());
        b.extend_from_slice(&self.maturity_block.to_le_bytes());
        b.push(self.status as u8);
        b.extend_from_slice(&self.issuer_contract.to_bytes());
        b.extend_from_slice(&self.total_staked.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 89 {
            return Err(ContractError::IoError(format!(
                "BondSeriesInfo: expected 89 bytes, got {}",
                data.len()
            )));
        }
        Ok(BondSeriesInfo {
            series_token_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondSeriesInfo: invalid series_token_id".into()))?,
            interest_rate_bps: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            maturity_block: u64::from_le_bytes(data[40..48].try_into().unwrap()),
            status: SeriesStatus::try_from(data[48])?,
            issuer_contract: ContractId::from_bytes(data[49..81].try_into().unwrap())?,
            total_staked: u64::from_le_bytes(data[81..89].try_into().unwrap()),
        })
    }
}

impl SeriesStatus {
    pub fn encode(&self) -> Vec<u8> { vec![*self as u8] }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("SeriesStatus: empty data".into())); }
        SeriesStatus::try_from(data[0])
    }
}

// ============================================================================
// STAKE COIN
// ============================================================================

/// An on-chain stake coin.
///
/// Stake coins are tracked in a Merkle tree. Each coin carries staking
/// metadata: value_commit (Pedersen), last_claim_block, maturity_block, and issuer_contract.
#[derive(Debug, Clone)]
pub struct BondCoin {
    /// Pedersen commitment of the principal value (additively homomorphic)
    pub value_commit: pallas::Point,
    /// Commitment of the stake pool series token_id (Poseidon hash)
    pub token_commit: pallas::Base,
    /// Nullifier — proves the coin has not been spent
    pub nullifier: Nullifier,
    /// Merkle root at the time the coin was created
    pub merkle_root: MerkleNode,
    /// Encrypted user data field
    pub user_data_enc: pallas::Base,
    /// Spend hook — set to the BondContract itself to prevent raw PN transfers
    pub spend_hook: pallas::Base,
    /// Signature public key (Poseidon hash of secret, as field element)
    pub signature_public: pallas::Base,
    /// Block height of last interest claim
    pub last_claim_block: u64,
    /// Block height when stake matures (can be unstaked)
    pub maturity_block: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
}

impl BondCoin {
    pub const ENCODED_SIZE: usize = 272;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(272);
        b.extend_from_slice(&self.value_commit.to_bytes());
        b.extend_from_slice(&self.token_commit.to_repr());
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.merkle_root.to_bytes());
        b.extend_from_slice(&self.user_data_enc.to_repr());
        b.extend_from_slice(&self.spend_hook.to_repr());
        b.extend_from_slice(&self.signature_public.to_repr());
        b.extend_from_slice(&self.last_claim_block.to_le_bytes());
        b.extend_from_slice(&self.maturity_block.to_le_bytes());
        b.extend_from_slice(&self.issuer_contract.to_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 272 {
            return Err(ContractError::IoError(format!(
                "BondCoin: expected 272 bytes, got {}",
                data.len()
            )));
        }
        Ok(BondCoin {
            value_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(&data[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("BondCoin: invalid value_commit".into()))?,
            token_commit: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondCoin: invalid token_commit".into()))?,
            nullifier: Nullifier::from_bytes(&data[64..96])
                .ok_or_else(|| ContractError::IoError("BondCoin: invalid nullifier".into()))?,
            merkle_root: MerkleNode::from_bytes(data[96..128].try_into().unwrap())
                .ok_or_else(|| ContractError::IoError("BondCoin: invalid merkle_root".into()))?,
            user_data_enc: pallas::Base::from_repr(data[128..160].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondCoin: invalid user_data_enc".into()))?,
            spend_hook: pallas::Base::from_repr(data[160..192].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondCoin: invalid spend_hook".into()))?,
            signature_public: pallas::Base::from_repr(data[192..224].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondCoin: invalid signature_public".into()))?,
            last_claim_block: u64::from_le_bytes(data[224..232].try_into().unwrap()),
            maturity_block: u64::from_le_bytes(data[232..240].try_into().unwrap()),
            issuer_contract: ContractId::from_bytes(data[240..272].try_into().unwrap())?,
        })
    }
}

impl Default for BondCoin {
    fn default() -> Self {
        BondCoin {
            value_commit: pallas::Point::identity(),
            token_commit: pallas::Base::zero(),
            nullifier: Nullifier::from_base(pallas::Base::zero()),
            merkle_root: MerkleNode::from_base(pallas::Base::zero()),
            user_data_enc: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            signature_public: pallas::Base::zero(),
            last_claim_block: 0,
            maturity_block: 0,
            issuer_contract: ContractId::from_base(pallas::Base::zero()),
        }
    }
}

/// Client-side witness data for ZK proof generation.
///
/// These fields are NEVER serialized on-chain. They are passed from the
/// client to the ZK prover alongside the on-chain coin data.
#[derive(Debug, Clone)]
pub struct BondCoinWitness {
    /// Principal value
    pub principal: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Block height of last interest claim
    pub last_claim_block: u64,
    /// Block height when stake matures (ZK-committed via CoinAttributes)
    pub maturity_block: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
    /// Value blind (for Pedersen value commitment)
    pub value_blind: pallas::Scalar,
    /// Token blind (for Poseidon token commitment)
    pub token_blind: pallas::Base,
}

// ============================================================================
// ISSUE STAKE
// ============================================================================

/// Parameters for IssueStakeV1 — create a new staking position.
///
/// Maturity is derived from the bond series (BondSeriesInfo), not set by
/// the wallet at issuance time.
#[derive(Debug, Clone)]
pub struct IssueStakeParamsV1 {
    /// Minimum claim value (dust protection)
    pub min_claim: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// Token ID for the stake pool series
    pub token_id: pallas::Base,
    /// Initial stake coin
    pub coin: BondCoin,
}

impl IssueStakeParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(72 + BondCoin::ENCODED_SIZE);
        b.extend_from_slice(&self.min_claim.to_le_bytes());
        b.extend_from_slice(&self.issuer_contract.to_bytes());
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.coin.encode());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 72 + BondCoin::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "IssueStakeParamsV1: expected at least {} bytes, got {}",
                72 + BondCoin::ENCODED_SIZE,
                data.len()
            )));
        }
        let min_claim = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let issuer_contract = ContractId::from_bytes(data[8..40].try_into().unwrap())?;
        let token_id = pallas::Base::from_repr(data[40..72].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("IssueStakeParamsV1: invalid token_id".into()))?;
        let coin = BondCoin::decode(&data[72..])?;
        Ok(IssueStakeParamsV1 { min_claim, issuer_contract, token_id, coin })
    }
}

/// State update for IssueStakeV1.
#[derive(Debug, Clone)]
pub struct IssueStakeUpdateV1 {
    pub coins: Vec<BondCoin>,
}

impl IssueStakeUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1 + self.coins.len() * BondCoin::ENCODED_SIZE;
        let mut b = Vec::with_capacity(cap);
        b.push(self.coins.len() as u8);
        for coin in &self.coins {
            b.extend_from_slice(&coin.encode());
        }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() {
            return Err(ContractError::IoError("IssueStakeUpdateV1: empty data".into()));
        }
        let count = data[0] as usize;
        let expected = 1 + count * BondCoin::ENCODED_SIZE;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "IssueStakeUpdateV1: expected {} bytes, got {}",
                expected,
                data.len()
            )));
        }
        let mut coins = Vec::with_capacity(count);
        for i in 0..count {
            let start = 1 + i * BondCoin::ENCODED_SIZE;
            coins.push(BondCoin::decode(&data[start..start + BondCoin::ENCODED_SIZE])?);
        }
        Ok(IssueStakeUpdateV1 { coins })
    }
}

// ============================================================================
// TRANSFER STAKE
// ============================================================================

/// On-chain input for TransferStakeV1 — proves ownership of an existing stake.
#[derive(Debug, Clone)]
pub struct BondInput {
    /// Pedersen commitment of the principal
    pub value_commit: pallas::Point,
    /// Token commitment
    pub token_commit: pallas::Base,
    /// Nullifier proving coin is not double-spent
    pub nullifier: Nullifier,
    /// Merkle root proving coin existed
    pub merkle_root: MerkleNode,
    /// Encrypted user data
    pub user_data_enc: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// Signature public key
    pub signature_public: pallas::Base,
}

impl BondInput {
    pub const ENCODED_SIZE: usize = 224;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(224);
        b.extend_from_slice(&self.value_commit.to_bytes());
        b.extend_from_slice(&self.token_commit.to_repr());
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.merkle_root.to_bytes());
        b.extend_from_slice(&self.user_data_enc.to_repr());
        b.extend_from_slice(&self.spend_hook.to_repr());
        b.extend_from_slice(&self.signature_public.to_repr());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 224 {
            return Err(ContractError::IoError(format!(
                "BondInput: expected 224 bytes, got {}",
                data.len()
            )));
        }
        Ok(BondInput {
            value_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(&data[0..32].try_into().unwrap()))
                .ok_or_else(|| ContractError::IoError("BondInput: invalid value_commit".into()))?,
            token_commit: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondInput: invalid token_commit".into()))?,
            nullifier: Nullifier::from_bytes(&data[64..96])
                .ok_or_else(|| ContractError::IoError("BondInput: invalid nullifier".into()))?,
            merkle_root: MerkleNode::from_bytes(data[96..128].try_into().unwrap())
                .ok_or_else(|| ContractError::IoError("BondInput: invalid merkle_root".into()))?,
            user_data_enc: pallas::Base::from_repr(data[128..160].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondInput: invalid user_data_enc".into()))?,
            spend_hook: pallas::Base::from_repr(data[160..192].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondInput: invalid spend_hook".into()))?,
            signature_public: pallas::Base::from_repr(data[192..224].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("BondInput: invalid signature_public".into()))?,
        })
    }
}

/// Client-side witness for transfer input.
#[derive(Debug, Clone)]
pub struct BondInputWitness {
    pub principal: u64,
    pub token_id: pallas::Base,
    pub last_claim_block: u64,
    pub maturity_block: u64,
    pub issuer_contract: ContractId,
    pub user_data: pallas::Base,
    pub coin_blind: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub token_blind: pallas::Base,
    pub leaf_position: u64,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: pallas::Base,
    pub ephemeral_signature_secret: pallas::Base,
}

/// Parameters for TransferStakeV1 — burn old stake, create new with same metadata.
#[derive(Debug, Clone)]
pub struct TransferStakeParamsV1 {
    pub inputs: Vec<BondInput>,
    pub outputs: Vec<BondCoin>,
}

impl TransferStakeParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(2 + self.inputs.len() * BondInput::ENCODED_SIZE + self.outputs.len() * BondCoin::ENCODED_SIZE);
        b.push(self.inputs.len() as u8);
        for input in &self.inputs { b.extend_from_slice(&input.encode()); }
        b.push(self.outputs.len() as u8);
        for output in &self.outputs { b.extend_from_slice(&output.encode()); }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 {
            return Err(ContractError::IoError("TransferStakeParamsV1: data too short".into()));
        }
        let input_count = data[0] as usize;
        let mut pos = 1usize;
        let mut inputs = Vec::with_capacity(input_count);
        for _ in 0..input_count {
            inputs.push(BondInput::decode(&data[pos..pos + BondInput::ENCODED_SIZE])?);
            pos += BondInput::ENCODED_SIZE;
        }
        if pos >= data.len() {
            return Err(ContractError::IoError("TransferStakeParamsV1: missing output count".into()));
        }
        let output_count = data[pos] as usize;
        pos += 1;
        let mut outputs = Vec::with_capacity(output_count);
        for _ in 0..output_count {
            outputs.push(BondCoin::decode(&data[pos..pos + BondCoin::ENCODED_SIZE])?);
            pos += BondCoin::ENCODED_SIZE;
        }
        Ok(TransferStakeParamsV1 { inputs, outputs })
    }
}

/// State update for TransferStakeV1.
#[derive(Debug, Clone)]
pub struct TransferStakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<BondCoin>,
}

impl TransferStakeUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + self.nullifiers.len() * 32 + self.coins.len() * BondCoin::ENCODED_SIZE;
        let mut b = Vec::with_capacity(cap);
        b.push(self.nullifiers.len() as u8);
        for n in &self.nullifiers {
            b.extend_from_slice(&n.to_bytes());
        }
        b.push(self.coins.len() as u8);
        for coin in &self.coins {
            b.extend_from_slice(&coin.encode());
        }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 {
            return Err(ContractError::IoError("TransferStakeUpdateV1: data too short".into()));
        }
        let null_count = data[0] as usize;
        let mut pos = 1usize;
        let mut nullifiers = Vec::with_capacity(null_count);
        for _ in 0..null_count {
            nullifiers.push(Nullifier::from_bytes(&data[pos..pos + 32])
                .ok_or_else(|| ContractError::IoError("TransferStakeUpdateV1: invalid nullifier".into()))?);
            pos += 32;
        }
        if pos >= data.len() {
            return Err(ContractError::IoError("TransferStakeUpdateV1: missing coin count".into()));
        }
        let coin_count = data[pos] as usize;
        pos += 1;
        let mut coins = Vec::with_capacity(coin_count);
        for _ in 0..coin_count {
            coins.push(BondCoin::decode(&data[pos..pos + BondCoin::ENCODED_SIZE])?);
            pos += BondCoin::ENCODED_SIZE;
        }
        Ok(TransferStakeUpdateV1 { nullifiers, coins })
    }
}

// ============================================================================
// REQUEST INTEREST
// ============================================================================

/// Status of an interest claim request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClaimStatus {
    /// Claim is awaiting payment from the issuer
    Pending = 0,
    /// Claim has been paid
    Paid = 1,
}

impl TryFrom<u8> for ClaimStatus {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(ClaimStatus::Pending),
            1 => Ok(ClaimStatus::Paid),
            _ => Err(ContractError::IoError(format!("Invalid ClaimStatus: {}", b))),
        }
    }
}

/// An on-chain record of a holder's interest claim request.
///
/// Like a physical bond coupon — the holder presents it, the issuer pays
/// against it. Stored in the `bonds_info` tree keyed by
/// `(token_commit, claim_block)`.
#[derive(Debug, Clone)]
pub struct RequestedClaim {
    /// Interest amount owed (computed deterministically)
    pub interest_amount: u64,
    /// Holder's one-time key for receiving payment
    pub payment_key: pallas::Base,
    /// Claim status
    pub status: ClaimStatus,
}

impl RequestedClaim {
    pub const ENCODED_SIZE: usize = 41;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(41);
        b.extend_from_slice(&self.interest_amount.to_le_bytes());
        b.extend_from_slice(&self.payment_key.to_repr());
        b.push(self.status as u8);
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 41 {
            return Err(ContractError::IoError(format!(
                "RequestedClaim: expected 41 bytes, got {}",
                data.len()
            )));
        }
        Ok(RequestedClaim {
            interest_amount: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            payment_key: pallas::Base::from_repr(data[8..40].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RequestedClaim: invalid payment_key".into()))?,
            status: ClaimStatus::try_from(data[40])?,
        })
    }
}

/// Parameters for RequestInterestV1 — holder requests interest payment.
///
/// The holder proves bond ownership (via Burn_V1 ZK proof) and provides
/// a fresh one-time key for the issuer to pay to. This is like presenting
/// a physical bond coupon — the burden is on the holder to ask.
///
/// Interest is computed deterministically from on-chain state:
/// ```text
/// interest = principal * interest_rate_bps * blocks_elapsed / (BP_PRECISION * BLOCKS_PER_YEAR)
/// ```
/// where `blocks_elapsed = current_block - last_claim_block`.
///
/// `last_claim_block` is NOT updated yet — only when the issuer pays.
/// The pending claim record blocks duplicate claims for the same period.
#[derive(Debug, Clone)]
pub struct RequestInterestParamsV1 {
    /// The stake coin being claimed against (not consumed)
    pub bond_input: BondInput,
    /// Current block height
    pub claim_block: u64,
    /// Fresh one-time key for the issuer to pay to
    pub payment_key: pallas::Base,
    /// Minimum claim threshold (dust protection)
    pub min_claim: u64,
}

impl RequestInterestParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(BondInput::ENCODED_SIZE + 48);
        b.extend_from_slice(&self.bond_input.encode());
        b.extend_from_slice(&self.claim_block.to_le_bytes());
        b.extend_from_slice(&self.payment_key.to_repr());
        b.extend_from_slice(&self.min_claim.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < BondInput::ENCODED_SIZE + 48 {
            return Err(ContractError::IoError(format!(
                "RequestInterestParamsV1: expected at least {} bytes, got {}",
                BondInput::ENCODED_SIZE + 48,
                data.len()
            )));
        }
        let bond_input = BondInput::decode(&data[..BondInput::ENCODED_SIZE])?;
        let pos = BondInput::ENCODED_SIZE;
        let claim_block = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        let payment_key = pallas::Base::from_repr(data[pos + 8..pos + 40].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("RequestInterestParamsV1: invalid payment_key".into()))?;
        let min_claim = u64::from_le_bytes(data[pos + 40..pos + 48].try_into().unwrap());
        Ok(RequestInterestParamsV1 { bond_input, claim_block, payment_key, min_claim })
    }
}

/// State update for RequestInterestV1 — stores the claim record on-chain.
#[derive(Debug, Clone)]
pub struct RequestInterestUpdateV1 {
    /// Token commit of the bond being claimed against
    pub bond_token_commit: pallas::Base,
    /// Block height of the claim
    pub claim_block: u64,
    /// The claim record to store
    pub claim: RequestedClaim,
}

impl RequestInterestUpdateV1 {
    pub const ENCODED_SIZE: usize = 81;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(81);
        b.extend_from_slice(&self.bond_token_commit.to_repr());
        b.extend_from_slice(&self.claim_block.to_le_bytes());
        b.extend_from_slice(&self.claim.encode());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 81 {
            return Err(ContractError::IoError(format!(
                "RequestInterestUpdateV1: expected 81 bytes, got {}",
                data.len()
            )));
        }
        Ok(RequestInterestUpdateV1 {
            bond_token_commit: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RequestInterestUpdateV1: invalid bond_token_commit".into()))?,
            claim_block: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            claim: RequestedClaim::decode(&data[40..81])?,
        })
    }
}

// ============================================================================
// PAY INTEREST
// ============================================================================

/// Parameters for PayInterestV1 — issuer pays a pending interest claim.
///
/// The issuer reads the claim record, verifies reserves are sufficient
/// (via latest CoverageReport), and creates a fresh payment coin
/// (BlindOutput_V1) addressed to the holder's one-time `payment_key`.
/// Updates `last_claim_block` on the stake coin and marks the claim Paid.
#[derive(Debug, Clone)]
pub struct PayInterestParamsV1 {
    /// Token commit identifying the bond
    pub bond_token_commit: pallas::Base,
    /// Block height of the claim being paid
    pub claim_block: u64,
    /// Payment coin (BlindOutput_V1 to holder's payment_key)
    pub interest_coin: BondCoin,
}

impl PayInterestParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(40 + BondCoin::ENCODED_SIZE);
        b.extend_from_slice(&self.bond_token_commit.to_repr());
        b.extend_from_slice(&self.claim_block.to_le_bytes());
        b.extend_from_slice(&self.interest_coin.encode());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 40 + BondCoin::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PayInterestParamsV1: expected at least {} bytes, got {}",
                40 + BondCoin::ENCODED_SIZE,
                data.len()
            )));
        }
        let bond_token_commit = pallas::Base::from_repr(data[0..32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("PayInterestParamsV1: invalid bond_token_commit".into()))?;
        let claim_block = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let interest_coin = BondCoin::decode(&data[40..])?;
        Ok(PayInterestParamsV1 { bond_token_commit, claim_block, interest_coin })
    }
}

/// State update for PayInterestV1 — updates stake coin and stores payment.
#[derive(Debug, Clone)]
pub struct PayInterestUpdateV1 {
    /// Stake coin with updated last_claim_block
    pub updated_coin: BondCoin,
    /// Payment coin (BlindOutput_V1)
    pub interest_coin: BondCoin,
    /// Token commit of the bond
    pub bond_token_commit: pallas::Base,
    /// Block height of the claim
    pub claim_block: u64,
    /// Full claim record with status pre-set to Paid (set in exec, written in apply)
    pub claim: RequestedClaim,
}

impl PayInterestUpdateV1 {
    pub const ENCODED_SIZE: usize = 625;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(625);
        b.extend_from_slice(&self.updated_coin.encode());
        b.extend_from_slice(&self.interest_coin.encode());
        b.extend_from_slice(&self.bond_token_commit.to_repr());
        b.extend_from_slice(&self.claim_block.to_le_bytes());
        b.extend_from_slice(&self.claim.encode());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 625 {
            return Err(ContractError::IoError(format!(
                "PayInterestUpdateV1: expected 625 bytes, got {}",
                data.len()
            )));
        }
        Ok(PayInterestUpdateV1 {
            updated_coin: BondCoin::decode(&data[0..272])?,
            interest_coin: BondCoin::decode(&data[272..544])?,
            bond_token_commit: pallas::Base::from_repr(data[544..576].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("PayInterestUpdateV1: invalid bond_token_commit".into()))?,
            claim_block: u64::from_le_bytes(data[576..584].try_into().unwrap()),
            claim: RequestedClaim::decode(&data[584..625])?,
        })
    }
}

// ============================================================================
// EMERGENCY UNSTAKE
// ============================================================================

/// Parameters for EmergencyUnstakeV1 — unstake before maturity when coverage fails.
///
/// Only valid when the latest coverage report shows
/// `coverage_ratio_bps < MIN_COVERAGE_RATIO_BPS` for the series.
#[derive(Debug, Clone)]
pub struct EmergencyUnstakeParamsV1 {
    pub bond_input: BondInput,
    /// Coverage report proving the series is under-collateralized
    pub coverage_report: CoverageReport,
}

impl EmergencyUnstakeParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(BondInput::ENCODED_SIZE + CoverageReport::ENCODED_SIZE);
        b.extend_from_slice(&self.bond_input.encode());
        b.extend_from_slice(&self.coverage_report.encode());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < BondInput::ENCODED_SIZE + CoverageReport::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "EmergencyUnstakeParamsV1: expected at least {} bytes, got {}",
                BondInput::ENCODED_SIZE + CoverageReport::ENCODED_SIZE,
                data.len()
            )));
        }
        let bond_input = BondInput::decode(&data[..BondInput::ENCODED_SIZE])?;
        let coverage_report = CoverageReport::decode(&data[BondInput::ENCODED_SIZE..])?;
        Ok(EmergencyUnstakeParamsV1 { bond_input, coverage_report })
    }
}

/// State update for EmergencyUnstakeV1.
#[derive(Debug, Clone)]
pub struct EmergencyUnstakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    /// Receipt coin proving emergency unstake
    pub receipt_coin: BondCoin,
}

impl EmergencyUnstakeUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1 + self.nullifiers.len() * 32 + 1 + BondCoin::ENCODED_SIZE;
        let mut b = Vec::with_capacity(cap);
        b.push(self.nullifiers.len() as u8);
        for n in &self.nullifiers {
            b.extend_from_slice(&n.to_bytes());
        }
        b.push(1u8);
        b.extend_from_slice(&self.receipt_coin.encode());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 + 32 + BondCoin::ENCODED_SIZE {
            return Err(ContractError::IoError("EmergencyUnstakeUpdateV1: data too short".into()));
        }
        let null_count = data[0] as usize;
        if data.len() < 1 + null_count * 32 + 1 + BondCoin::ENCODED_SIZE {
            return Err(ContractError::IoError("EmergencyUnstakeUpdateV1: data too short for nullifiers".into()));
        }
        let mut nullifiers = Vec::with_capacity(null_count);
        for i in 0..null_count {
            let start = 1 + i * 32;
            nullifiers.push(Nullifier::from_bytes(&data[start..start + 32])
                .ok_or_else(|| ContractError::IoError("EmergencyUnstakeUpdateV1: invalid nullifier".into()))?);
        }
        let coin_pos = 1 + null_count * 32 + 1;
        let receipt_coin = BondCoin::decode(&data[coin_pos..coin_pos + BondCoin::ENCODED_SIZE])?;
        Ok(EmergencyUnstakeUpdateV1 { nullifiers, receipt_coin })
    }
}

// ============================================================================
// UNSTAKE
// ============================================================================

/// Parameters for UnstakeV1 — withdraw principal at maturity.
#[derive(Debug, Clone)]
pub struct UnstakeParamsV1 {
    pub bond_input: BondInput,
    /// Current block height (public input, verified by host)
    pub current_block: u64,
}

impl UnstakeParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(BondInput::ENCODED_SIZE + 8);
        b.extend_from_slice(&self.bond_input.encode());
        b.extend_from_slice(&self.current_block.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < BondInput::ENCODED_SIZE + 8 {
            return Err(ContractError::IoError(format!(
                "UnstakeParamsV1: expected at least {} bytes, got {}",
                BondInput::ENCODED_SIZE + 8,
                data.len()
            )));
        }
        let bond_input = BondInput::decode(&data[..BondInput::ENCODED_SIZE])?;
        let current_block = u64::from_le_bytes(data[BondInput::ENCODED_SIZE..BondInput::ENCODED_SIZE + 8].try_into().unwrap());
        Ok(UnstakeParamsV1 { bond_input, current_block })
    }
}

/// State update for UnstakeV1.
#[derive(Debug, Clone)]
pub struct UnstakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    /// Receipt coin proving unstake
    pub receipt_coin: BondCoin,
}

impl UnstakeUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1 + self.nullifiers.len() * 32 + 1 + BondCoin::ENCODED_SIZE;
        let mut b = Vec::with_capacity(cap);
        b.push(self.nullifiers.len() as u8);
        for n in &self.nullifiers {
            b.extend_from_slice(&n.to_bytes());
        }
        b.push(1u8);
        b.extend_from_slice(&self.receipt_coin.encode());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 + 32 + BondCoin::ENCODED_SIZE {
            return Err(ContractError::IoError("UnstakeUpdateV1: data too short".into()));
        }
        let null_count = data[0] as usize;
        if data.len() < 1 + null_count * 32 + 1 + BondCoin::ENCODED_SIZE {
            return Err(ContractError::IoError("UnstakeUpdateV1: data too short for nullifiers".into()));
        }
        let mut nullifiers = Vec::with_capacity(null_count);
        for i in 0..null_count {
            let start = 1 + i * 32;
            nullifiers.push(Nullifier::from_bytes(&data[start..start + 32])
                .ok_or_else(|| ContractError::IoError("UnstakeUpdateV1: invalid nullifier".into()))?);
        }
        let coin_pos = 1 + null_count * 32 + 1;
        let receipt_coin = BondCoin::decode(&data[coin_pos..coin_pos + BondCoin::ENCODED_SIZE])?;
        Ok(UnstakeUpdateV1 { nullifiers, receipt_coin })
    }
}

// ============================================================================
// BURN STAKE
// ============================================================================

/// Parameters for BurnStakeV1 — issuer retires staking pool.
#[derive(Debug, Clone)]
pub struct BurnStakeParamsV1 {
    pub inputs: Vec<BondInput>,
}

impl BurnStakeParamsV1 {
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(1+self.inputs.len()*BondInput::ENCODED_SIZE); b.push(self.inputs.len() as u8); for input in &self.inputs { b.extend_from_slice(&input.encode()); } b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() {
            return Err(ContractError::IoError("BurnStakeParamsV1: empty data".into()));
        }
        let count = data[0] as usize;
        let expected = 1 + count * BondInput::ENCODED_SIZE;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "BurnStakeParamsV1: expected {} bytes, got {}",
                expected,
                data.len()
            )));
        }
        let mut inputs = Vec::with_capacity(count);
        for i in 0..count {
            let start = 1 + i * BondInput::ENCODED_SIZE;
            inputs.push(BondInput::decode(&data[start..start + BondInput::ENCODED_SIZE])?);
        }
        Ok(BurnStakeParamsV1 { inputs })
    }
}

/// State update for BurnStakeV1.
#[derive(Debug, Clone)]
pub struct BurnStakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
}

impl BurnStakeUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1 + self.nullifiers.len() * 32;
        let mut b = Vec::with_capacity(cap);
        b.push(self.nullifiers.len() as u8);
        for n in &self.nullifiers {
            b.extend_from_slice(&n.to_bytes());
        }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() {
            return Err(ContractError::IoError("BurnStakeUpdateV1: empty data".into()));
        }
        let count = data[0] as usize;
        let expected = 1 + count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "BurnStakeUpdateV1: expected {} bytes, got {}",
                expected,
                data.len()
            )));
        }
        let mut nullifiers = Vec::with_capacity(count);
        for i in 0..count {
            let start = 1 + i * 32;
            nullifiers.push(Nullifier::from_bytes(&data[start..start + 32])
                .ok_or_else(|| ContractError::IoError("BurnStakeUpdateV1: invalid nullifier".into()))?);
        }
        Ok(BurnStakeUpdateV1 { nullifiers })
    }
}

// ============================================================================
// INTEREST CALCULATION (host-side helper)
// ============================================================================

/// Basis point precision (10000 = 100%).
pub const BP_PRECISION: u64 = 10000;

/// Approximate blocks per year (2-second block time: ~15_768_000 blocks/year).
pub const BLOCKS_PER_YEAR: u64 = 15_768_000;

/// Calculate deterministic interest accrued on a stake position.
///
/// ```text
/// interest = principal * interest_rate_bps * blocks_elapsed / (BP_PRECISION * BLOCKS_PER_YEAR)
/// ```
///
/// Returns `None` on overflow or if `blocks_elapsed` is zero.
pub fn calculate_interest(
    principal: u64,
    interest_rate_bps: u64,
    blocks_elapsed: u64,
) -> Option<u64> {
    if blocks_elapsed == 0 {
        return Some(0);
    }
    let numerator = (principal as u128) * (interest_rate_bps as u128) * (blocks_elapsed as u128);
    let denominator = (BP_PRECISION as u128) * (BLOCKS_PER_YEAR as u128);
    let result = numerator / denominator;
    if result > u64::MAX as u128 {
        return None;
    }
    Some(result as u64)
}

// ============================================================================
// PROVE COVERAGE (GOVERNANCE)
// ============================================================================

/// Parameters for ProveCoverageV1 — proves solvency (callable by issuer or holder).
///
/// The ZK circuit (ProveCoverage_V1) uses `base_div` to compute
/// `coverage_ratio_bps = reserve_amount / (total_outstanding + total_interest_obligation) * 10000`
/// and constrains it against the submitted value. The entrypoint
/// independently verifies `reserve_amount >= total_outstanding + total_interest_obligation`
/// (>= 100% coverage required for both principal and interest).
#[derive(Debug, Clone)]
pub struct ProveCoverageParamsV1 {
    /// Staking pool series identifier
    pub series_token_id: pallas::Base,
    /// Total staked principal across all stake coins in the series
    pub total_outstanding: u64,
    /// Total accrued interest obligation across all outstanding stakes
    pub total_interest_obligation: u64,
    /// Issuer's reserve balance (must be >= total_outstanding + total_interest_obligation)
    pub reserve_amount: u64,
    /// coverage_ratio_bps = reserve_amount / (total_outstanding + total_interest_obligation) * 10000
    pub coverage_ratio_bps: u64,
    /// Block height of this report
    pub report_block: u64,
    /// ZK proof (ProveCoverage_V1 circuit)
    pub proof: Vec<u8>,
}

impl ProveCoverageParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(72 + self.proof.len());
        b.extend_from_slice(&self.series_token_id.to_repr());
        b.extend_from_slice(&self.total_outstanding.to_le_bytes());
        b.extend_from_slice(&self.total_interest_obligation.to_le_bytes());
        b.extend_from_slice(&self.reserve_amount.to_le_bytes());
        b.extend_from_slice(&self.coverage_ratio_bps.to_le_bytes());
        b.extend_from_slice(&self.report_block.to_le_bytes());
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 72 {
            return Err(ContractError::IoError(format!(
                "ProveCoverageParamsV1: expected at least 72 bytes, got {}",
                data.len()
            )));
        }
        let series_token_id = pallas::Base::from_repr(data[0..32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ProveCoverageParamsV1: invalid series_token_id".into()))?;
        let total_outstanding = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let total_interest_obligation = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let reserve_amount = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let coverage_ratio_bps = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let report_block = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let proof = data[72..].to_vec();
        Ok(ProveCoverageParamsV1 {
            series_token_id,
            total_outstanding,
            total_interest_obligation,
            reserve_amount,
            coverage_ratio_bps,
            report_block,
            proof,
        })
    }
}

/// On-chain record of a coverage report.
///
/// Stored in the `bonds_info` tree keyed by
/// `poseidon_hash(series_token_id, report_block)`.
#[derive(Debug, Clone)]
pub struct CoverageReport {
    /// Staking pool series identifier
    pub series_token_id: pallas::Base,
    /// Total staked principal at time of report
    pub total_outstanding: u64,
    /// Total interest obligation across all outstanding stakes
    pub total_interest_obligation: u64,
    /// Issuer's reserve balance at time of report
    pub reserve_amount: u64,
    /// Coverage ratio in basis points (10000 = 100%)
    /// Computed as: reserve_amount / (total_outstanding + total_interest_obligation) * 10000
    pub coverage_ratio_bps: u64,
    /// Block height of this report
    pub report_block: u64,
}

impl CoverageReport {
    pub const ENCODED_SIZE: usize = 72;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(72);
        b.extend_from_slice(&self.series_token_id.to_repr());
        b.extend_from_slice(&self.total_outstanding.to_le_bytes());
        b.extend_from_slice(&self.total_interest_obligation.to_le_bytes());
        b.extend_from_slice(&self.reserve_amount.to_le_bytes());
        b.extend_from_slice(&self.coverage_ratio_bps.to_le_bytes());
        b.extend_from_slice(&self.report_block.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 72 {
            return Err(ContractError::IoError(format!(
                "CoverageReport: expected 72 bytes, got {}",
                data.len()
            )));
        }
        Ok(CoverageReport {
            series_token_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CoverageReport: invalid series_token_id".into()))?,
            total_outstanding: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            total_interest_obligation: u64::from_le_bytes(data[40..48].try_into().unwrap()),
            reserve_amount: u64::from_le_bytes(data[48..56].try_into().unwrap()),
            coverage_ratio_bps: u64::from_le_bytes(data[56..64].try_into().unwrap()),
            report_block: u64::from_le_bytes(data[64..72].try_into().unwrap()),
        })
    }
}

/// State update for ProveCoverageV1.
#[derive(Debug, Clone)]
pub struct ProveCoverageUpdateV1 {
    pub report: CoverageReport,
}

impl ProveCoverageUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        self.report.encode()
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let report = CoverageReport::decode(data)?;
        Ok(ProveCoverageUpdateV1 { report })
    }
}
