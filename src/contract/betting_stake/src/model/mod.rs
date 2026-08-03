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

//! Betting Stake Contract Model
//!
//! Data structures for capital staking against betting contracts.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, PublicKey, schnorr::Signature},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

use crate::EARNINGS_BP;

// =============================================================================
// STAKE REGISTRY
// =============================================================================

/// Registry of a single betting table's staking info
#[derive(Debug, Clone)]
pub struct TableStakeRegistry {
    pub version: u8,
    /// Contract ID of the betting contract (Dice, Baccarat, etc.)
    pub betting_contract_id: pallas::Base,
    /// Total capital staked against this table
    pub total_stake: u64,
    /// Accumulated earnings for stakers
    pub accumulated_earnings: u64,
    /// Accumulated losses (payouts that stakers absorbed)
    pub accumulated_losses: u64,
    /// Number of active stakers
    pub staker_count: u64,
    /// House edge of the underlying betting contract (in basis points)
    pub house_edge_bp: u32,
    /// Risk profile (determines risk premium)
    pub risk_profile: u8, // 0=Low, 1=Medium, 2=High
}

impl TableStakeRegistry {
    /// Calculate earnings rate per unit staked
    pub fn earnings_rate_bp(&self) -> u32 {
        // Base house edge - accumulated losses ratio + risk premium
        let base = self.house_edge_bp;
        let losses_ratio = if self.total_stake > 0 {
            self.accumulated_losses
                .checked_mul(EARNINGS_BP as u64)
                .and_then(|v| v.checked_div(self.total_stake))
                .map(|v| v as u32)
                .unwrap_or(0)
        } else {
            0
        };
        // Net earnings rate (can be negative if losses exceed house edge)
        base.saturating_sub(losses_ratio)
    }

    /// Calculate loss absorption capacity
    pub fn loss_absorption_capacity(&self) -> u64 {
        self.total_stake.saturating_sub(self.accumulated_losses)
    }
}

// =============================================================================
// INDIVIDUAL STAKE
// =============================================================================

/// Individual stake position
#[derive(Debug, Clone)]
pub struct Stake {
    pub version: u8,
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Unique stake ID
    pub stake_id: pallas::Base,
    /// Associated table registry ID
    pub table_id: pallas::Base,
    /// Staker's public key
    pub staker_pub: PublicKey,
    /// Original stake amount
    pub original_amount: u64,
    /// Current stake amount (decreases with losses)
    pub current_amount: u64,
    /// Accumulated earnings (can be claimed)
    pub accumulated_earnings: u64,
    /// Timestamp of when stake was created
    pub created_at: u64,
    /// Timestamp of when unstake was requested (if any)
    pub unstake_requested_at: Option<u64>,
    /// Whether stake is active
    pub is_active: bool,
}

impl Stake {
    /// Calculate current share of the table's earnings
    pub fn earnings_share(&self, table: &TableStakeRegistry) -> u64 {
        if table.total_stake == 0 {
            return 0
        }
        // Proportional share of earnings based on stake proportion
        (table.accumulated_earnings * self.current_amount) / table.total_stake
    }

    /// Calculate proportional loss
    pub fn loss_share(&self, loss_amount: u64, table: &TableStakeRegistry) -> u64 {
        if table.total_stake == 0 {
            return 0
        }
        (loss_amount * self.current_amount) / table.total_stake
    }

    /// Check if unstake is available (no pending unstake request or lock period passed)
    pub fn can_unstake(&self, lock_blocks: u64, current_block: u64) -> bool {
        if !self.is_active {
            return false
        }
        match self.unstake_requested_at {
            Some(req_at) => current_block >= req_at + lock_blocks,
            None => true, // Can unstake immediately if no pending request
        }
    }
}

// =============================================================================
// PARAMS AND UPDATES
#[allow(dead_code)]
fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }
// =============================================================================

/// Parameters for InitializeV1
#[derive(Debug, Clone)]
pub struct InitializeParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Contract ID of the betting contract this staking is for
    pub betting_contract_id: pallas::Base,
    /// House edge of the betting contract (in basis points)
    pub house_edge_bp: u32,
    /// Risk profile (0=Low, 1=Medium, 2=High)
    pub risk_profile: u8,
    /// Nonce for table_id derivation (public input for ZK proof)
    pub nonce: pallas::Base,
    /// Signature from betting contract verifying these params
    pub signature: Signature,
}

impl dwow_serial::Encodable for InitializeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl InitializeParamsV1 {
    pub const ENCODED_SIZE: usize = 165;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.betting_contract_id.to_repr());
        buf.extend_from_slice(&self.house_edge_bp.to_le_bytes());
        buf.push(self.risk_profile);
        buf.extend_from_slice(&self.nonce.to_repr());
        buf.extend_from_slice(&self.signature.encode());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 101 { return Err(ContractError::IoError("InitializeParamsV1: too short".into())); }
        let instance_seed: [u8; 32] = data[0..32].try_into().unwrap();
        let betting_contract_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("InitializeParamsV1: invalid betting_contract_id".into()))?;
        let house_edge_bp = u32::from_le_bytes(data[64..68].try_into().unwrap());
        let risk_profile = data[68];
        let nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[69..101].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("InitializeParamsV1: invalid nonce".into()))?;
        let signature = Signature::decode(&data[101..165])
            .ok_or_else(|| ContractError::IoError("InitializeParamsV1: invalid signature".into()))?;
        Ok(InitializeParamsV1 { instance_seed, betting_contract_id, house_edge_bp, risk_profile, nonce, signature })
    }
}

/// Update produced by InitializeV1
#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub table_id: pallas::Base,
    pub betting_contract_id: pallas::Base,
    pub house_edge_bp: u32,
    pub risk_profile: u8,
}

/// Parameters for StakeV1
#[derive(Debug, Clone)]
pub struct StakeParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Table ID to stake against
    pub table_id: pallas::Base,
    /// Staker's public key
    pub staker_pub: PublicKey,
    /// Amount to stake
    pub amount: u64,
    /// Nonce for stake_id derivation (public input for ZK proof)
    pub nonce: pallas::Base,
    /// Value commitment point (public input for ZK proof)
    pub value_commit: pallas::Point,
    /// Staker nullifier = H(stake_id, staker_secret) for ZK replay protection
    pub staker_nullifier: pallas::Base,
    /// Spend hook FuncId for promissory_note::transfer_v1 callback
    pub spend_hook: pallas::Base,
    /// User data for spend hook callback
    pub user_data: pallas::Base,
}

impl dwow_serial::Encodable for StakeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for StakeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl StakeParamsV1 {
    pub const ENCODED_SIZE: usize = 264;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.staker_pub.to_bytes());
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.nonce.to_repr());
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.staker_nullifier.to_repr());
        buf.extend_from_slice(&self.spend_hook.to_repr());
        buf.extend_from_slice(&self.user_data.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("StakeParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        let instance_seed: [u8; 32] = data[0..32].try_into().unwrap();
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("StakeParamsV1: invalid table_id".into()))?;
        let staker_pub = PublicKey::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("StakeParamsV1: invalid staker_pub: {}", e)))?;
        let amount = u64::from_le_bytes(data[96..104].try_into().unwrap());
        let nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[104..136].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("StakeParamsV1: invalid nonce".into()))?;
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[136..168].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("StakeParamsV1: invalid value_commit".into()))?;
        let staker_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[168..200].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("StakeParamsV1: invalid staker_nullifier".into()))?;
        let spend_hook = Option::<pallas::Base>::from(pallas::Base::from_repr(data[200..232].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("StakeParamsV1: invalid spend_hook".into()))?;
        let user_data = Option::<pallas::Base>::from(pallas::Base::from_repr(data[232..264].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("StakeParamsV1: invalid user_data".into()))?;
        Ok(StakeParamsV1 { instance_seed, table_id, staker_pub, amount, nonce, value_commit, staker_nullifier, spend_hook, user_data })
    }
}

/// Update produced by StakeV1
#[derive(Debug, Clone)]
pub struct StakeUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub stake_id: pallas::Base,
    pub table_id: pallas::Base,
    pub staker_pub: PublicKey,
    pub amount: u64,
    pub total_stake: u64,
    pub staker_count: u64,
    pub staker_nullifier: pallas::Base,
}

/// Parameters for UnstakeV1
#[derive(Debug, Clone)]
pub struct UnstakeParamsV1 {
    /// Stake ID to unstake
    pub stake_id: pallas::Base,
    /// Associated table registry ID
    pub table_id: pallas::Base,
    /// Staker's public key
    pub staker_pub: PublicKey,
    /// Original stake amount (used in stake_id derivation)
    pub original_amount: u64,
    /// Nonce for stake_id derivation (public input for ZK proof)
    pub nonce: pallas::Base,
    /// Value commitment point (public input for ZK proof)
    pub value_commit: pallas::Point,
    /// Staker nullifier = H(stake_id, staker_secret) for ZK replay protection
    pub staker_nullifier: pallas::Base,
    /// Spend hook FuncId for promissory_note::transfer_v1 callback
    pub spend_hook: pallas::Base,
    /// User data for spend hook callback
    pub user_data: pallas::Base,
}

impl dwow_serial::Encodable for UnstakeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UnstakeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl UnstakeParamsV1 {
    pub const ENCODED_SIZE: usize = 264;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.staker_pub.to_bytes());
        buf.extend_from_slice(&self.original_amount.to_le_bytes());
        buf.extend_from_slice(&self.nonce.to_repr());
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.staker_nullifier.to_repr());
        buf.extend_from_slice(&self.spend_hook.to_repr());
        buf.extend_from_slice(&self.user_data.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("UnstakeParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UnstakeParamsV1: invalid stake_id".into()))?;
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UnstakeParamsV1: invalid table_id".into()))?;
        let staker_pub = PublicKey::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("UnstakeParamsV1: invalid staker_pub: {}", e)))?;
        let original_amount = u64::from_le_bytes(data[96..104].try_into().unwrap());
        let nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[104..136].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UnstakeParamsV1: invalid nonce".into()))?;
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[136..168].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UnstakeParamsV1: invalid value_commit".into()))?;
        let staker_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[168..200].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UnstakeParamsV1: invalid staker_nullifier".into()))?;
        let spend_hook = Option::<pallas::Base>::from(pallas::Base::from_repr(data[200..232].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UnstakeParamsV1: invalid spend_hook".into()))?;
        let user_data = Option::<pallas::Base>::from(pallas::Base::from_repr(data[232..264].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UnstakeParamsV1: invalid user_data".into()))?;
        Ok(UnstakeParamsV1 { stake_id, table_id, staker_pub, original_amount, nonce, value_commit, staker_nullifier, spend_hook, user_data })
    }
}

/// Update produced by UnstakeV1
#[derive(Debug, Clone)]
pub struct UnstakeUpdateV1 {
    pub stake_id: pallas::Base,
    pub payout_amount: u64, // original stake + earnings - losses
    pub unstake_penalty: u64, // any penalty for early unstake
    pub staker_nullifier: pallas::Base,
}

/// Parameters for ClaimEarningsV1
#[derive(Debug, Clone)]
pub struct ClaimEarningsParamsV1 {
    /// Stake ID to claim earnings for
    pub stake_id: pallas::Base,
    /// Associated table registry ID
    pub table_id: pallas::Base,
    /// Staker's public key
    pub staker_pub: PublicKey,
    /// Current stake amount (used in stake_id derivation)
    pub current_amount: u64,
    /// Nonce for stake_id derivation (public input for ZK proof)
    pub nonce: pallas::Base,
    /// Value commitment point (public input for ZK proof)
    pub value_commit: pallas::Point,
    /// Staker nullifier = H(stake_id, staker_secret) for ZK replay protection
    pub staker_nullifier: pallas::Base,
}

impl dwow_serial::Encodable for ClaimEarningsParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClaimEarningsParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl ClaimEarningsParamsV1 {
    pub const ENCODED_SIZE: usize = 200;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.staker_pub.to_bytes());
        buf.extend_from_slice(&self.current_amount.to_le_bytes());
        buf.extend_from_slice(&self.nonce.to_repr());
        buf.extend_from_slice(&self.value_commit.to_bytes());
        buf.extend_from_slice(&self.staker_nullifier.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("ClaimEarningsParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClaimEarningsParamsV1: invalid stake_id".into()))?;
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClaimEarningsParamsV1: invalid table_id".into()))?;
        let staker_pub = PublicKey::from_bytes(data[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("ClaimEarningsParamsV1: invalid staker_pub: {}", e)))?;
        let current_amount = u64::from_le_bytes(data[96..104].try_into().unwrap());
        let nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[104..136].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClaimEarningsParamsV1: invalid nonce".into()))?;
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[136..168].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClaimEarningsParamsV1: invalid value_commit".into()))?;
        let staker_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[168..200].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("ClaimEarningsParamsV1: invalid staker_nullifier".into()))?;
        Ok(ClaimEarningsParamsV1 { stake_id, table_id, staker_pub, current_amount, nonce, value_commit, staker_nullifier })
    }
}

/// Update produced by ClaimEarningsV1
#[derive(Debug, Clone)]
pub struct ClaimEarningsUpdateV1 {
    pub stake_id: pallas::Base,
    pub claimed_amount: u64,
    pub remaining_earnings: u64,
    pub staker_nullifier: pallas::Base,
}

/// Parameters for UpdateRiskV1 (called by betting contracts when payouts occur)
#[derive(Debug, Clone)]
pub struct UpdateRiskParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// Total payout amount that stakers must absorb
    pub payout_amount: u64,
    /// House's share of the payout (if any)
    pub house_share: u64,
    /// Betting contract ID (public input for ZK proof)
    pub betting_contract_id: pallas::Base,
    /// Nonce for table_id derivation (public input for ZK proof)
    pub nonce: pallas::Base,
}

impl dwow_serial::Encodable for UpdateRiskParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateRiskParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl UpdateRiskParamsV1 {
    pub const ENCODED_SIZE: usize = 112;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.payout_amount.to_le_bytes());
        buf.extend_from_slice(&self.house_share.to_le_bytes());
        buf.extend_from_slice(&self.betting_contract_id.to_repr());
        buf.extend_from_slice(&self.nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("UpdateRiskParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UpdateRiskParamsV1: invalid table_id".into()))?;
        let payout_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let house_share = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let betting_contract_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[48..80].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UpdateRiskParamsV1: invalid betting_contract_id".into()))?;
        let nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[80..112].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UpdateRiskParamsV1: invalid nonce".into()))?;
        Ok(UpdateRiskParamsV1 { table_id, payout_amount, house_share, betting_contract_id, nonce })
    }
}

/// Update produced by UpdateRiskV1
#[derive(Debug, Clone)]
pub struct UpdateRiskUpdateV1 {
    pub table_id: pallas::Base,
    pub total_payout: u64,
    pub staker_loss: u64, // Total loss distributed among stakers
    pub staker_count: u64,
    pub new_total_stake: u64,
}

// =============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE — STORED TYPES + BRIDGE UPDATES
// =============================================================================
// Per type-system.md §2.2: bytes round-trip across module boundaries is forbidden.
// Per contract-wasm-type-system.md §3.1: bridge SHALL use explicit per-type encode/decode.

impl TableStakeRegistry {
    /// Fixed canonical byte size:
    /// version(1) + betting_contract_id(32) + total_stake(8) + accumulated_earnings(8)
    /// + accumulated_losses(8) + staker_count(8) + house_edge_bp(4) + risk_profile(1)
    pub const ENCODED_SIZE: usize = 70;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.betting_contract_id.to_repr());
        buf.extend_from_slice(&self.total_stake.to_le_bytes());
        buf.extend_from_slice(&self.accumulated_earnings.to_le_bytes());
        buf.extend_from_slice(&self.accumulated_losses.to_le_bytes());
        buf.extend_from_slice(&self.staker_count.to_le_bytes());
        buf.extend_from_slice(&self.house_edge_bp.to_le_bytes());
        buf.push(self.risk_profile);
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "TableStakeRegistry: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let version = data[0];
        let betting_contract_id =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[1..33].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "TableStakeRegistry: invalid betting_contract_id".into(),
                )
            })?;
        let total_stake = u64::from_le_bytes(data[33..41].try_into().unwrap());
        let accumulated_earnings =
            u64::from_le_bytes(data[41..49].try_into().unwrap());
        let accumulated_losses =
            u64::from_le_bytes(data[49..57].try_into().unwrap());
        let staker_count = u64::from_le_bytes(data[57..65].try_into().unwrap());
        let house_edge_bp = u32::from_le_bytes(data[65..69].try_into().unwrap());
        let risk_profile = data[69];
        Ok(TableStakeRegistry {
            version,
            betting_contract_id,
            total_stake,
            accumulated_earnings,
            accumulated_losses,
            staker_count,
            house_edge_bp,
            risk_profile,
        })
    }
}

impl Stake {
    /// Fixed canonical byte size:
    /// version(1) + instance_seed(32) + stake_id(32) + table_id(32) + staker_pub(32)
    /// + original_amount(8) + current_amount(8) + accumulated_earnings(8)
    /// + created_at(8) + unstake_requested_at(9: 1 flag + 8) + is_active(1)
    pub const ENCODED_SIZE: usize = 171;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.staker_pub.to_bytes());
        buf.extend_from_slice(&self.original_amount.to_le_bytes());
        buf.extend_from_slice(&self.current_amount.to_le_bytes());
        buf.extend_from_slice(&self.accumulated_earnings.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        match self.unstake_requested_at {
            Some(v) => {
                buf.push(1);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            None => {
                buf.push(0);
                buf.extend_from_slice(&[0u8; 8]);
            }
        }
        buf.push(if self.is_active { 1 } else { 0 });
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Stake: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let version = data[0];
        let instance_seed: [u8; 32] = data[1..33].try_into().unwrap();
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[33..65].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("Stake: invalid stake_id".into()))?;
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[65..97].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("Stake: invalid table_id".into()))?;
        let staker_pub = PublicKey::from_bytes(data[97..129].try_into().unwrap())
            .map_err(|e| {
                ContractError::IoError(format!("Stake: invalid staker_pub: {:?}", e))
            })?;
        let original_amount =
            u64::from_le_bytes(data[129..137].try_into().unwrap());
        let current_amount =
            u64::from_le_bytes(data[137..145].try_into().unwrap());
        let accumulated_earnings =
            u64::from_le_bytes(data[145..153].try_into().unwrap());
        let created_at = u64::from_le_bytes(data[153..161].try_into().unwrap());
        let unstake_requested_at = if data[161] == 1 {
            Some(u64::from_le_bytes(data[162..170].try_into().unwrap()))
        } else {
            None
        };
        let is_active = data[170] != 0;
        Ok(Stake {
            version,
            instance_seed,
            stake_id,
            table_id,
            staker_pub,
            original_amount,
            current_amount,
            accumulated_earnings,
            created_at,
            unstake_requested_at,
            is_active,
        })
    }
}

impl dwow_serial::Encodable for InitializeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl InitializeUpdateV1 {
    /// instance_seed(32) + table_id(32) + betting_contract_id(32) + house_edge_bp(4) + risk_profile(1)
    pub const ENCODED_SIZE: usize = 101;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.betting_contract_id.to_repr());
        buf.extend_from_slice(&self.house_edge_bp.to_le_bytes());
        buf.push(self.risk_profile);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "InitializeUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let instance_seed: [u8; 32] = data[0..32].try_into().unwrap();
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[32..64].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("InitializeUpdateV1: invalid table_id".into())
        })?;
        let betting_contract_id =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[64..96].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "InitializeUpdateV1: invalid betting_contract_id".into(),
                )
            })?;
        let house_edge_bp = u32::from_le_bytes(data[96..100].try_into().unwrap());
        let risk_profile = data[100];
        Ok(InitializeUpdateV1 {
            instance_seed,
            table_id,
            betting_contract_id,
            house_edge_bp,
            risk_profile,
        })
    }
}

impl dwow_serial::Encodable for StakeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for StakeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl StakeUpdateV1 {
    /// instance_seed(32) + stake_id(32) + table_id(32) + staker_pub(32)
    /// + amount(8) + total_stake(8) + staker_count(8) + staker_nullifier(32)
    pub const ENCODED_SIZE: usize = 184;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.staker_pub.to_bytes());
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.total_stake.to_le_bytes());
        buf.extend_from_slice(&self.staker_count.to_le_bytes());
        buf.extend_from_slice(&self.staker_nullifier.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "StakeUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let instance_seed: [u8; 32] = data[0..32].try_into().unwrap();
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[32..64].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("StakeUpdateV1: invalid stake_id".into())
        })?;
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[64..96].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("StakeUpdateV1: invalid table_id".into())
        })?;
        let staker_pub = PublicKey::from_bytes(data[96..128].try_into().unwrap())
            .map_err(|e| {
                ContractError::IoError(format!(
                    "StakeUpdateV1: invalid staker_pub: {:?}",
                    e
                ))
            })?;
        let amount = u64::from_le_bytes(data[128..136].try_into().unwrap());
        let total_stake = u64::from_le_bytes(data[136..144].try_into().unwrap());
        let staker_count = u64::from_le_bytes(data[144..152].try_into().unwrap());
        let staker_nullifier =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[152..184].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "StakeUpdateV1: invalid staker_nullifier".into(),
                )
            })?;
        Ok(StakeUpdateV1 {
            instance_seed,
            stake_id,
            table_id,
            staker_pub,
            amount,
            total_stake,
            staker_count,
            staker_nullifier,
        })
    }
}

impl dwow_serial::Encodable for UnstakeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UnstakeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl UnstakeUpdateV1 {
    /// stake_id(32) + payout_amount(8) + unstake_penalty(8) + staker_nullifier(32)
    pub const ENCODED_SIZE: usize = 80;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.payout_amount.to_le_bytes());
        buf.extend_from_slice(&self.unstake_penalty.to_le_bytes());
        buf.extend_from_slice(&self.staker_nullifier.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "UnstakeUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("UnstakeUpdateV1: invalid stake_id".into())
        })?;
        let payout_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let unstake_penalty =
            u64::from_le_bytes(data[40..48].try_into().unwrap());
        let staker_nullifier =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[48..80].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "UnstakeUpdateV1: invalid staker_nullifier".into(),
                )
            })?;
        Ok(UnstakeUpdateV1 {
            stake_id,
            payout_amount,
            unstake_penalty,
            staker_nullifier,
        })
    }
}

impl dwow_serial::Encodable for ClaimEarningsUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClaimEarningsUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl ClaimEarningsUpdateV1 {
    /// stake_id(32) + claimed_amount(8) + remaining_earnings(8) + staker_nullifier(32)
    pub const ENCODED_SIZE: usize = 80;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.claimed_amount.to_le_bytes());
        buf.extend_from_slice(&self.remaining_earnings.to_le_bytes());
        buf.extend_from_slice(&self.staker_nullifier.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ClaimEarningsUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError(
                "ClaimEarningsUpdateV1: invalid stake_id".into(),
            )
        })?;
        let claimed_amount =
            u64::from_le_bytes(data[32..40].try_into().unwrap());
        let remaining_earnings =
            u64::from_le_bytes(data[40..48].try_into().unwrap());
        let staker_nullifier =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[48..80].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "ClaimEarningsUpdateV1: invalid staker_nullifier".into(),
                )
            })?;
        Ok(ClaimEarningsUpdateV1 {
            stake_id,
            claimed_amount,
            remaining_earnings,
            staker_nullifier,
        })
    }
}

impl dwow_serial::Encodable for UpdateRiskUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateRiskUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl UpdateRiskUpdateV1 {
    /// table_id(32) + total_payout(8) + staker_loss(8) + staker_count(8) + new_total_stake(8)
    pub const ENCODED_SIZE: usize = 64;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.total_payout.to_le_bytes());
        buf.extend_from_slice(&self.staker_loss.to_le_bytes());
        buf.extend_from_slice(&self.staker_count.to_le_bytes());
        buf.extend_from_slice(&self.new_total_stake.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "UpdateRiskUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let table_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("UpdateRiskUpdateV1: invalid table_id".into())
        })?;
        let total_payout = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let staker_loss = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let staker_count = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let new_total_stake =
            u64::from_le_bytes(data[56..64].try_into().unwrap());
        Ok(UpdateRiskUpdateV1 {
            table_id,
            total_payout,
            staker_loss,
            staker_count,
            new_total_stake,
        })
    }
}

// =============================================================================
// HELPERS
// =============================================================================

/// Derive table ID from betting contract ID
pub fn derive_table_id(betting_contract_id: pallas::Base, nonce: u64) -> pallas::Base {
    use dwow_sdk::crypto::poseidon_hash;
    poseidon_hash([betting_contract_id, pallas::Base::from(nonce)])
}

/// Derive stake ID
pub fn derive_stake_id(table_id: pallas::Base, staker_pub: &PublicKey, amount: u64, nonce: u64) -> pallas::Base {
    use dwow_sdk::crypto::poseidon_hash;
    poseidon_hash([table_id, staker_pub.x().expect("pk not identity"), staker_pub.y().expect("pk not identity"), pallas::Base::from(amount), pallas::Base::from(nonce)])
}
