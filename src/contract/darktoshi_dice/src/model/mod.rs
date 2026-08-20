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

//! DarkToshi Dice Contract Data Models
//!
//! Per rho-calculus explicit encode/decode: bytes round-trip across module
//! boundaries is forbidden. All stored types and bridge update structs SHALL
//! use explicit `encode() -> Vec<u8>` and `decode(&[u8]) -> Result<Self, ContractError>`.

use dwow_sdk::{
    crypto::{
        pasta_prelude::PrimeField, poseidon_hash,
        tx_hash_to_base, PublicKey,
    },
    error::ContractError,
    pasta::{group::Group, group::GroupEncoding, pallas},
};

use crate::error::DiceError;
use crate::{MAX_HOUSE_EDGE, MAX_TARGET, MIN_HOUSE_EDGE, ROLL_RANGE};

// dwow_serial bridge impls for BetState

// ============================================================================
// STATE TYPES
// ============================================================================

/// Unique bet identifier (Poseidon hash of bet parameters)
pub type BetId = pallas::Base;

/// Represents the current state of a bet in the state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BetState {
    Committed = 0,
    Revealed = 1,
    SettledPlayer = 2,
    SettledHouse = 3,
    Cancelled = 4,
}

impl TryFrom<u8> for BetState {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Committed),
            1 => Ok(Self::Revealed),
            2 => Ok(Self::SettledPlayer),
            3 => Ok(Self::SettledHouse),
            4 => Ok(Self::Cancelled),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

impl BetState {
    /// Encode BetState as a single byte.
    pub fn encode(&self) -> Vec<u8> {
        vec![*self as u8]
    }

    /// Decode BetState from a single byte.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 1 {
            return Err(ContractError::IoError("Invalid BetState length".to_string()))
        }
        Self::try_from(data[0])
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Core bet data stored on-chain.
///
/// Per type-system.md: SHALL use explicit encode/decode, not SerialEncodable.
/// `roll: Option<u8>` uses Pattern 4: presence byte + 1 byte if Some.
#[derive(Debug, Clone)]
pub struct Bet {
    pub version: u8,
    pub id: BetId,
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub target: u8,
    pub secret_nonce_commit: pallas::Base,
    pub blind: pallas::Base,
    pub roll: Option<u8>,
    pub state: BetState,
    pub house_edge: u32,
    pub confirmation_depth: u8,
    pub created_at: u64,
    pub revealed_at: u64,
    pub settle_block: u64,
    pub value_commit: pallas::Point,
    pub asset_id: pallas::Base,
    pub nullifier: BetId,
    pub instance_seed: [u8; 32],
}

/// Fixed encoded size for Bet: 362 bytes.
pub const BET_ENCODED_SIZE: usize = 362;

impl Bet {
    /// Encode Bet into a fixed-size byte vector (362 bytes).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(BET_ENCODED_SIZE);
        b.push(self.version);
        let (px, py) = self.player_pub.xy().expect("pk not identity");
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&px.to_repr());
        b.extend_from_slice(&py.to_repr());
        b.extend_from_slice(&self.bet_value.to_le_bytes());
        b.push(self.target);
        b.extend_from_slice(&self.secret_nonce_commit.to_repr());
        b.extend_from_slice(&self.blind.to_repr());
        // Pattern 4: Option<u8> — presence byte + 1 byte if Some
        b.push(self.roll.is_some() as u8);
        if let Some(v) = self.roll {
            b.push(v);
        } else {
            b.push(0u8);
        }
        b.push(self.state as u8);
        b.extend_from_slice(&self.house_edge.to_le_bytes());
        b.push(self.confirmation_depth);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.revealed_at.to_le_bytes());
        b.extend_from_slice(&self.settle_block.to_le_bytes());
        {
            use pasta_curves::arithmetic::CurveAffine;
            use pasta_curves::group::Curve;
            if bool::from(self.value_commit.is_identity()) {
                b.extend_from_slice(&[0u8; 64]);
            } else {
                let vc_affine = self.value_commit.to_affine();
                let coords = vc_affine.coordinates().expect("value_commit not identity");
                b.extend_from_slice(&coords.x().to_repr());
                b.extend_from_slice(&coords.y().to_repr());
            }
        }
        b.extend_from_slice(&self.asset_id.to_repr());
        b.extend_from_slice(&self.nullifier.to_repr());
        b.extend_from_slice(&self.instance_seed);
        b
    }

    /// Decode Bet from a byte slice (362 bytes expected).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != BET_ENCODED_SIZE {
            return Err(ContractError::IoError("Invalid Bet length".to_string()))
        }
        let mut pos = 0;

        let version = data[pos];
        pos += 1;

        let id = decode_base(&data[pos..pos + 32], "id")?;
        pos += 32;

        let player_pub = decode_public_key(&data[pos..pos + 64])?;
        pos += 64;

        let bet_value = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let target = data[pos];
        pos += 1;

        let secret_nonce_commit = decode_base(&data[pos..pos + 32], "secret_nonce_commit")?;
        pos += 32;

        let blind = decode_base(&data[pos..pos + 32], "blind")?;
        pos += 32;

        // Pattern 4: Option<u8> — presence byte + value byte
        let has_roll = data[pos] != 0;
        pos += 1;
        let roll_byte = data[pos];
        pos += 1;
        let roll = if has_roll { Some(roll_byte) } else { None };

        let state = BetState::try_from(data[pos])?;
        pos += 1;

        let house_edge = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;

        let confirmation_depth = data[pos];
        pos += 1;

        let created_at = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let revealed_at = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let settle_block = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let value_commit = decode_point(&data[pos..pos + 64], "value_commit")?;
        pos += 64;

        let asset_id = decode_base(&data[pos..pos + 32], "asset_id")?;
        pos += 32;

        let nullifier = decode_base(&data[pos..pos + 32], "nullifier")?;
        pos += 32;

        let mut instance_seed = [0u8; 32];
        instance_seed.copy_from_slice(&data[pos..pos + 32]);

        Ok(Self {
            version,
            id,
            player_pub,
            bet_value,
            target,
            secret_nonce_commit,
            blind,
            roll,
            state,
            house_edge,
            confirmation_depth,
            created_at,
            revealed_at,
            settle_block,
            value_commit,
            asset_id,
            nullifier,
            instance_seed,
        })
    }

    /// Calculate payout for player winning.
    /// Formula: bet_value * (10000 - house_edge) / (target * 100)
    /// Example: bet=100, target=50, house_edge=200bp (2%)
    ///   payout = 100 * 9800 / 5000 = 196
    pub fn calculate_payout(&self) -> Option<u64> {
        let multiplier = 10000u64.checked_sub(self.house_edge as u64)?;
        let product = self.bet_value.checked_mul(multiplier)?;
        product.checked_div(self.target as u64 * 100)
    }

    /// Calculate house's take when house wins.
    /// House takes: bet_value - base_win + (base_win * house_edge / 10000)
    /// where base_win = bet_value * 100 / target
    /// This simplifies to: bet_value * (target + house_edge - 100) / target
    pub fn calculate_house_take(&self) -> Option<u64> {
        let base_win = self.bet_value.checked_mul(100)?.checked_div(self.target as u64)?;
        let profit = self.bet_value.saturating_sub(base_win);
        let house_cut = base_win.checked_mul(self.house_edge as u64)?.checked_div(10000)?;
        profit.checked_add(house_cut)
    }
}

// ============================================================================
#[allow(dead_code)]
fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }

// PARAMETER TYPES — deserialized from
// contract call data in entrypoint.rs)
// ============================================================================

/// Parameters for `Dice::CommitBetV1`
#[derive(Debug, Clone,)]
pub struct CommitBetParamsV1 {
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub target: u8,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub asset_id: pallas::Base,
    pub value_commit: pallas::Point,
    pub signature: pallas::Base,
    pub house_edge: u32,
    pub confirmation_depth: u8,
    pub instance_seed: [u8; 32],
}

impl dwow_serial::Encodable for CommitBetParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CommitBetParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CommitBetParamsV1 { pub const ENCODED_SIZE: usize = 238; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(238); b.extend_from_slice(&self.player_pub.to_bytes()); b.extend_from_slice(&self.bet_value.to_le_bytes()); b.push(self.target); b.extend_from_slice(&self.secret_nonce.to_repr()); b.extend_from_slice(&self.blind.to_repr()); b.extend_from_slice(&self.asset_id.to_repr()); b.extend_from_slice(&self.value_commit.to_bytes()); b.extend_from_slice(&self.signature.to_repr()); b.extend_from_slice(&self.house_edge.to_le_bytes()); b.push(self.confirmation_depth); b.extend_from_slice(&self.instance_seed); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 238 { return Err(ContractError::IoError(format!("CommitBetParamsV1: expected 238 bytes, got {}", data.len()))); } let player_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CommitBetParamsV1: invalid player_pub: {}", e)))?; let bet_value = u64::from_le_bytes(data[32..40].try_into().unwrap()); let target = data[40]; let secret_nonce = decode_base(&data[41..73], "secret_nonce")?; let blind = decode_base(&data[73..105], "blind")?; let asset_id = decode_base(&data[105..137], "asset_id")?; let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[137..169].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitBetParamsV1: invalid value_commit".into()))?; let signature = decode_base(&data[169..201], "signature")?; let house_edge = u32::from_le_bytes(data[201..205].try_into().unwrap()); let confirmation_depth = data[205]; let instance_seed: [u8;32] = data[206..238].try_into().unwrap(); Ok(CommitBetParamsV1 { player_pub, bet_value, target, secret_nonce, blind, asset_id, value_commit, signature, house_edge, confirmation_depth, instance_seed }) } }

/// State update for `CommitBetV1`.
///
/// Per rho-calculus: explicit encode/decode for bridge (module boundary crossing).
/// Fixed encoding: 350 bytes.
#[derive(Debug, Clone)]
pub struct CommitBetUpdateV1 {
    pub bet_id: BetId,
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub target: u8,
    pub secret_nonce_commit: pallas::Base,
    pub blind: pallas::Base,
    pub value_commit: pallas::Point,
    pub asset_id: pallas::Base,
    pub house_edge: u32,
    pub confirmation_depth: u8,
    pub settle_block: u64,
    pub nullifier: BetId,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}

pub const COMMIT_BET_UPDATE_ENCODED_SIZE: usize = 350;

impl dwow_serial::Encodable for CommitBetUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CommitBetUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CommitBetUpdateV1 {
    /// Encode into a fixed-size byte vector (350 bytes).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(COMMIT_BET_UPDATE_ENCODED_SIZE);
        let (px, py) = self.player_pub.xy().expect("pk not identity");
        b.extend_from_slice(&self.bet_id.to_repr());
        b.extend_from_slice(&px.to_repr());
        b.extend_from_slice(&py.to_repr());
        b.extend_from_slice(&self.bet_value.to_le_bytes());
        b.push(self.target);
        b.extend_from_slice(&self.secret_nonce_commit.to_repr());
        b.extend_from_slice(&self.blind.to_repr());
        {
            use pasta_curves::arithmetic::CurveAffine;
            use pasta_curves::group::Curve;
            if bool::from(self.value_commit.is_identity()) {
                b.extend_from_slice(&[0u8; 64]);
            } else {
                let vc_affine = self.value_commit.to_affine();
                let coords = vc_affine.coordinates().expect("value_commit not identity");
                b.extend_from_slice(&coords.x().to_repr());
                b.extend_from_slice(&coords.y().to_repr());
            }
        }
        b.extend_from_slice(&self.asset_id.to_repr());
        b.extend_from_slice(&self.house_edge.to_le_bytes());
        b.push(self.confirmation_depth);
        b.extend_from_slice(&self.settle_block.to_le_bytes());
        b.extend_from_slice(&self.nullifier.to_repr());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }

    /// Decode from a byte slice (350 bytes expected).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != COMMIT_BET_UPDATE_ENCODED_SIZE {
            return Err(ContractError::IoError("Invalid CommitBetUpdateV1 length".to_string()))
        }
        let mut pos = 0;

        let bet_id = decode_base(&data[pos..pos + 32], "bet_id")?;
        pos += 32;

        let player_pub = decode_public_key(&data[pos..pos + 64])?;
        pos += 64;

        let bet_value = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let target = data[pos];
        pos += 1;

        let secret_nonce_commit = decode_base(&data[pos..pos + 32], "secret_nonce_commit")?;
        pos += 32;

        let blind = decode_base(&data[pos..pos + 32], "blind")?;
        pos += 32;

        let value_commit = decode_point(&data[pos..pos + 64], "value_commit")?;
        pos += 64;

        let asset_id = decode_base(&data[pos..pos + 32], "asset_id")?;
        pos += 32;

        let house_edge = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;

        let confirmation_depth = data[pos];
        pos += 1;

        let settle_block = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let nullifier = decode_base(&data[pos..pos + 32], "nullifier")?;
        pos += 32;

        let created_at = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let mut instance_seed = [0u8; 32];
        instance_seed.copy_from_slice(&data[pos..pos + 32]);

        Ok(Self {
            bet_id,
            player_pub,
            bet_value,
            target,
            secret_nonce_commit,
            blind,
            value_commit,
            asset_id,
            house_edge,
            confirmation_depth,
            settle_block,
            nullifier,
            created_at,
            instance_seed,
        })
    }
}

/// Parameters for `Dice::RevealRollV1`
#[derive(Debug, Clone,)]
pub struct RevealRollParamsV1 {
    pub bet_id: BetId,
    pub secret_nonce: pallas::Base,
}

impl dwow_serial::Encodable for RevealRollParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RevealRollParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RevealRollParamsV1 { pub const ENCODED_SIZE: usize = 64; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.bet_id.to_repr()); b.extend_from_slice(&self.secret_nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("RevealRollParamsV1: expected 64 bytes, got {}", data.len()))); } Ok(RevealRollParamsV1 { bet_id: decode_base(&data[0..32],"bet_id")?, secret_nonce: decode_base(&data[32..64],"secret_nonce")? }) } }

/// State update for `RevealRollV1`.
///
/// Fixed encoding: bet_id(32) + Bet(362).
#[derive(Debug, Clone)]
pub struct RevealRollUpdateV1 {
    pub bet_id: BetId,
    /// Carried bet record (exec sets roll/state/revealed_at; apply re-stores it).
    pub bet: Bet,
}

pub const REVEAL_ROLL_UPDATE_ENCODED_SIZE: usize = 394;

impl dwow_serial::Encodable for RevealRollUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RevealRollUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RevealRollUpdateV1 {
    /// Encode into a fixed-size byte vector (394 bytes).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(REVEAL_ROLL_UPDATE_ENCODED_SIZE);
        b.extend_from_slice(&self.bet_id.to_repr());
        b.extend_from_slice(&self.bet.encode());
        b
    }

    /// Decode from a byte slice (394 bytes expected).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != REVEAL_ROLL_UPDATE_ENCODED_SIZE {
            return Err(ContractError::IoError("Invalid RevealRollUpdateV1 length".to_string()))
        }
        let bet_id = decode_base(&data[..32], "bet_id")?;
        let bet = Bet::decode(&data[32..])?;
        Ok(Self { bet_id, bet })
    }
}

/// Parameters for `Dice::SettleBetV1`
#[derive(Debug, Clone,)]
pub struct SettleBetParamsV1 {
    pub bet_id: BetId,
    pub proof: Vec<u8>,
    pub roll_hash: pallas::Base,
}

impl dwow_serial::Encodable for SettleBetParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SettleBetParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SettleBetParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(33 + self.proof.len()); b.extend_from_slice(&self.bet_id.to_repr()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.roll_hash.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 65 { return Err(ContractError::IoError("SettleBetParamsV1: too short".into())); } let bet_id = decode_base(&data[0..32],"bet_id")?; let proof_len = data[32] as usize; if data.len() != 33 + proof_len + 32 { return Err(ContractError::IoError(format!("SettleBetParamsV1: expected {} bytes, got {}", 33+proof_len+32, data.len()))); } let proof = data[33..33+proof_len].to_vec(); let roll_hash = decode_base(&data[33+proof_len..33+proof_len+32],"roll_hash")?; Ok(SettleBetParamsV1 { bet_id, proof, roll_hash }) } }

/// State update for `SettleBetV1`.
///
/// Fixed encoding: bet_id(32) + payout(8) + Bet(362) + house_balance(8).
#[derive(Debug, Clone)]
pub struct SettleBetUpdateV1 {
    pub bet_id: BetId,
    pub payout: u64,
    /// Carried bet record (exec sets state; apply re-stores it).
    pub bet: Bet,
    /// Carried house balance (exec advances it on a house win; apply re-stores it).
    pub house_balance: u64,
}

pub const SETTLE_BET_UPDATE_ENCODED_SIZE: usize = 410;

impl dwow_serial::Encodable for SettleBetUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SettleBetUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SettleBetUpdateV1 {
    /// Encode into a fixed-size byte vector (410 bytes).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(SETTLE_BET_UPDATE_ENCODED_SIZE);
        b.extend_from_slice(&self.bet_id.to_repr());
        b.extend_from_slice(&self.payout.to_le_bytes());
        b.extend_from_slice(&self.bet.encode());
        b.extend_from_slice(&self.house_balance.to_le_bytes());
        b
    }

    /// Decode from a byte slice (410 bytes expected).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != SETTLE_BET_UPDATE_ENCODED_SIZE {
            return Err(ContractError::IoError("Invalid SettleBetUpdateV1 length".to_string()))
        }
        let bet_id = decode_base(&data[..32], "bet_id")?;
        let payout = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let bet = Bet::decode(&data[40..402])?;
        let house_balance = u64::from_le_bytes(data[402..410].try_into().unwrap());
        Ok(Self { bet_id, payout, bet, house_balance })
    }
}

/// Parameters for `Dice::HouseCloseV1`
#[derive(Debug, Clone,)]
pub struct HouseCloseParamsV1 {
    pub bet_id: BetId,
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub close_nullifier: pallas::Base,
}

impl dwow_serial::Encodable for HouseCloseParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for HouseCloseParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl HouseCloseParamsV1 { pub const ENCODED_SIZE: usize = 128; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(128); b.extend_from_slice(&self.bet_id.to_repr()); b.extend_from_slice(&self.house_pub_x.to_repr()); b.extend_from_slice(&self.house_pub_y.to_repr()); b.extend_from_slice(&self.close_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 128 { return Err(ContractError::IoError(format!("HouseCloseParamsV1: expected 128 bytes, got {}", data.len()))); } Ok(HouseCloseParamsV1 { bet_id: decode_base(&data[0..32],"bet_id")?, house_pub_x: decode_base(&data[32..64],"house_pub_x")?, house_pub_y: decode_base(&data[64..96],"house_pub_y")?, close_nullifier: decode_base(&data[96..128],"close_nullifier")? }) } }

/// State update for `HouseCloseV1`.
///
/// Fixed encoding: bet_id(32) + close_nullifier(32) + Bet(362) + house_balance(8).
#[derive(Debug, Clone)]
pub struct HouseCloseUpdateV1 {
    pub bet_id: BetId,
    pub close_nullifier: pallas::Base,
    /// Carried bet record (exec sets state=Cancelled; apply re-stores it).
    pub bet: Bet,
    /// Carried house balance (exec advances it; apply re-stores it).
    pub house_balance: u64,
}

pub const HOUSE_CLOSE_UPDATE_ENCODED_SIZE: usize = 434;

impl dwow_serial::Encodable for HouseCloseUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for HouseCloseUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl HouseCloseUpdateV1 {
    /// Encode into a fixed-size byte vector (434 bytes).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(HOUSE_CLOSE_UPDATE_ENCODED_SIZE);
        b.extend_from_slice(&self.bet_id.to_repr());
        b.extend_from_slice(&self.close_nullifier.to_repr());
        b.extend_from_slice(&self.bet.encode());
        b.extend_from_slice(&self.house_balance.to_le_bytes());
        b
    }

    /// Decode from a byte slice (434 bytes expected).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != HOUSE_CLOSE_UPDATE_ENCODED_SIZE {
            return Err(ContractError::IoError("Invalid HouseCloseUpdateV1 length".to_string()))
        }
        let bet_id = decode_base(&data[..32], "bet_id")?;
        let close_nullifier = decode_base(&data[32..64], "close_nullifier")?;
        let bet = Bet::decode(&data[64..426])?;
        let house_balance = u64::from_le_bytes(data[426..434].try_into().unwrap());
        Ok(Self { bet_id, close_nullifier, bet, house_balance })
    }
}

// ============================================================================
// ENCODE/DECODE HELPERS
// ============================================================================

/// Decode a pallas::Base from 32 bytes.
fn decode_base(data: &[u8], _field: &str) -> Result<pallas::Base, ContractError> {
    let arr: [u8; 32] = data.try_into().map_err(|_| {
        ContractError::IoError("Invalid slice length for pallas::Base".to_string())
    })?;
    match Option::<pallas::Base>::from(pallas::Base::from_repr(arr)) {
        Some(v) => Ok(v),
        None => Err(ContractError::IoError("Invalid repr for pallas::Base".to_string())),
    }
}

/// Decode a PublicKey from 64 bytes (x || y).
fn decode_public_key(data: &[u8]) -> Result<PublicKey, ContractError> {
    use pasta_curves::arithmetic::CurveAffine;
    let x = decode_base(&data[..32], "pk_x")?;
    let y = decode_base(&data[32..64], "pk_y")?;
    let point = Option::<pasta_curves::EpAffine>::from(pasta_curves::EpAffine::from_xy(x, y))
        .map(pasta_curves::Ep::from)
        .ok_or_else(|| ContractError::IoError("Invalid public key coordinates".to_string()))?;
    PublicKey::try_from(point)
}

/// Decode a pallas::Point from 64 bytes (x || y, affine). All-zero bytes = identity.
fn decode_point(data: &[u8], _field: &str) -> Result<pallas::Point, ContractError> {
    use pasta_curves::arithmetic::CurveAffine;
    use pasta_curves::group::Group;
    let x = decode_base(&data[..32], "point_x")?;
    let y = decode_base(&data[32..64], "point_y")?;
    if x == pallas::Base::zero() && y == pallas::Base::zero() {
        return Ok(pallas::Point::identity());
    }
    match Option::<pasta_curves::EpAffine>::from(pasta_curves::EpAffine::from_xy(x, y)) {
        Some(affine) => Ok(pasta_curves::Ep::from(affine)),
        None => Err(ContractError::IoError("Invalid point affine".to_string())),
    }
}

// ============================================================================
// VALIDATION HELPERS
// ============================================================================

pub fn validate_target(target: u8) -> Result<(), DiceError> {
    if target == 0 || target > MAX_TARGET {
        return Err(DiceError::InvalidTarget)
    }
    Ok(())
}

pub fn validate_house_edge(house_edge: u32) -> Result<(), DiceError> {
    if house_edge < MIN_HOUSE_EDGE || house_edge > MAX_HOUSE_EDGE {
        return Err(DiceError::InvalidHouseEdge)
    }
    Ok(())
}

pub fn derive_bet_id(
    player_pub: &PublicKey,
    bet_value: u64,
    target: u8,
    secret_nonce: pallas::Base,
    blind: pallas::Base,
    asset_id: pallas::Base,
) -> BetId {
    let (px, py) = player_pub.xy().expect("pk not identity");
    poseidon_hash([
        pallas::Base::from(4),
        px,
        py,
        pallas::Base::from(bet_value),
        pallas::Base::from(u64::from(target)),
        secret_nonce,
        blind,
        asset_id,
    ])
}

pub fn derive_nullifier(bet_id: BetId, secret_nonce: pallas::Base) -> BetId {
    poseidon_hash([bet_id, secret_nonce])
}

/// Calculate roll using multiple block hashes for enhanced randomness.
/// Uses cumulative PoW entropy - more blocks means exponentially harder to manipulate.
///
/// Security scaling with depth:
/// - K=1: 33% manipulation chance (with 33% hash power)
/// - K=6: ~0.14% (Bitcoin "6 confirmations" standard)
/// - K=10: ~0.005%
/// Calculate a dice roll from a multi-block entropy seed (via dwow_entropy_contract::derive_seed).
/// The seed already incorporates multiple block hashes; we mix in bet-specific data for uniqueness.
pub fn calculate_roll_with_depth(
    seed: pallas::Base,
    bet_id: BetId,
    secret_nonce: pallas::Base,
) -> u8 {
    let final_entropy = poseidon_hash([seed, bet_id, secret_nonce]);
    let bytes = final_entropy.to_repr();
    ((bytes[0] as u64) % (ROLL_RANGE as u64)) as u8
}

/// Legacy single-block hash roll calculation.
/// Deprecated: Use calculate_roll_with_depth for production gambling.
#[deprecated(since = "0.1.0", note = "Use calculate_roll_with_depth with adjustable confirmation depth")]
pub fn calculate_roll(tx_hash_bytes: [u8; 32], bet_id: BetId, secret_nonce: pallas::Base) -> u8 {
    let block_hash = tx_hash_to_base(&tx_hash_bytes);
    let roll_input = poseidon_hash([block_hash, bet_id, secret_nonce]);
    let bytes = roll_input.to_repr();
    ((bytes[0] as u64) % (ROLL_RANGE as u64)) as u8
}
