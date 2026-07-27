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

//! Data structures for pool_stake contract calls

use dwow_serial::{SerialDecodable, SerialEncodable};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::pallas,
};

/// Pool stake registry - one per pool
#[derive(Debug, Clone)]
pub struct PoolStakeRegistry {
    pub version: u8,
    /// Unique pool identifier (poseidon hash)
    pub pool_id: pallas::Base,
    /// Pool creator/owner public key
    pub owner_pub: PublicKey,
    /// Total stake amount in the pool (sum of all member stakes)
    pub total_stake: u64,
    /// Available coverage (total_coverage - allocated_coverage)
    pub available_coverage: u64,
    /// Currently allocated coverage for in-flight withdrawals
    pub allocated_coverage: u64,
    /// Number of pool members
    pub member_count: u64,
    /// Maximum coverage ratio (e.g., 10000 = 1:1 stake:coverage)
    pub max_coverage_ratio: u32,
    /// Fee percentage for pool operator (in basis points)
    pub operator_fee_bp: u32,
    /// Block when pool was created
    pub created_at: u64,
    /// Total amount slashed from this pool
    pub total_slashed: u64,
    /// Number of slash events in this pool
    pub pool_slash_count: u64,
    /// Whether pool is active
    pub is_active: bool,
}

impl PoolStakeRegistry {
    pub const ENCODED_SIZE: usize = 130; // 1+32+32+8+8+8+8+4+4+8+8+8+1

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.owner_pub.to_bytes());
        buf.extend_from_slice(&self.total_stake.to_le_bytes());
        buf.extend_from_slice(&self.available_coverage.to_le_bytes());
        buf.extend_from_slice(&self.allocated_coverage.to_le_bytes());
        buf.extend_from_slice(&self.member_count.to_le_bytes());
        buf.extend_from_slice(&self.max_coverage_ratio.to_le_bytes());
        buf.extend_from_slice(&self.operator_fee_bp.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.total_slashed.to_le_bytes());
        buf.extend_from_slice(&self.pool_slash_count.to_le_bytes());
        buf.push(self.is_active as u8);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PoolStakeRegistry: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let version = data[0];
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[1..33].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("PoolStakeRegistry: invalid pool_id".into()))?;
        let owner_pub = PublicKey::from_bytes(data[33..65].try_into().unwrap())?;
        let total_stake = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let available_coverage = u64::from_le_bytes(data[73..81].try_into().unwrap());
        let allocated_coverage = u64::from_le_bytes(data[81..89].try_into().unwrap());
        let member_count = u64::from_le_bytes(data[89..97].try_into().unwrap());
        let max_coverage_ratio = u32::from_le_bytes(data[97..101].try_into().unwrap());
        let operator_fee_bp = u32::from_le_bytes(data[101..105].try_into().unwrap());
        let created_at = u64::from_le_bytes(data[105..113].try_into().unwrap());
        let total_slashed = u64::from_le_bytes(data[113..121].try_into().unwrap());
        let pool_slash_count = u64::from_le_bytes(data[121..129].try_into().unwrap());
        let is_active = data[129] != 0;
        Ok(PoolStakeRegistry {
            version,
            pool_id,
            owner_pub,
            total_stake,
            available_coverage,
            allocated_coverage,
            member_count,
            max_coverage_ratio,
            operator_fee_bp,
            created_at,
            total_slashed,
            pool_slash_count,
            is_active,
        })
    }
}

/// Individual member stake position in a pool
#[derive(Debug, Clone)]
pub struct PoolMemberStake {
    pub version: u8,
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Unique stake identifier
    pub stake_id: pallas::Base,
    /// Pool this stake belongs to
    pub pool_id: pallas::Base,
    /// Member public key
    pub member_pub: PublicKey,
    /// Relayer ID this member controls
    pub relayer_id: [u8; 32],
    /// Original stake amount
    pub original_amount: u64,
    /// Current stake amount (after losses)
    pub current_amount: u64,
    /// Coverage contribution to pool
    pub coverage_contribution: u64,
    /// Share of pool in basis points
    pub pool_share_bp: u32,
    /// Accumulated fees claimable by this member
    pub accumulated_fees: u64,
    /// Block when stake was created
    pub created_at: u64,
    /// Block when leave was requested (if requested)
    pub leave_requested_at: Option<u64>,
    /// Number of times this member has been slashed
    pub slash_count: u64,
    /// Whether this stake is active
    pub is_active: bool,
}

impl PoolMemberStake {
    pub const ENCODED_SIZE: usize = 223; // 1+32+32+32+32+32+8+8+8+4+8+8+9+8+1

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.member_pub.to_bytes());
        buf.extend_from_slice(&self.relayer_id);
        buf.extend_from_slice(&self.original_amount.to_le_bytes());
        buf.extend_from_slice(&self.current_amount.to_le_bytes());
        buf.extend_from_slice(&self.coverage_contribution.to_le_bytes());
        buf.extend_from_slice(&self.pool_share_bp.to_le_bytes());
        buf.extend_from_slice(&self.accumulated_fees.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        // Pattern 4: Option<u64> — 1-byte discriminant + 8-byte value
        match self.leave_requested_at {
            Some(v) => {
                buf.push(1u8);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            None => {
                buf.push(0u8);
                buf.extend_from_slice(&0u64.to_le_bytes());
            }
        }
        buf.extend_from_slice(&self.slash_count.to_le_bytes());
        buf.push(self.is_active as u8);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PoolMemberStake: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let version = data[0];
        let instance_seed: [u8; 32] = data[1..33].try_into().unwrap();
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[33..65].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("PoolMemberStake: invalid stake_id".into()))?;
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[65..97].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("PoolMemberStake: invalid pool_id".into()))?;
        let member_pub = PublicKey::from_bytes(data[97..129].try_into().unwrap())?;
        let relayer_id: [u8; 32] = data[129..161].try_into().unwrap();
        let original_amount = u64::from_le_bytes(data[161..169].try_into().unwrap());
        let current_amount = u64::from_le_bytes(data[169..177].try_into().unwrap());
        let coverage_contribution = u64::from_le_bytes(data[177..185].try_into().unwrap());
        let pool_share_bp = u32::from_le_bytes(data[185..189].try_into().unwrap());
        let accumulated_fees = u64::from_le_bytes(data[189..197].try_into().unwrap());
        let created_at = u64::from_le_bytes(data[197..205].try_into().unwrap());
        // Pattern 4: Option<u64> — 1-byte discriminant + 8-byte value
        let leave_requested_at = if data[205] == 0 {
            None
        } else {
            Some(u64::from_le_bytes(data[206..214].try_into().unwrap()))
        };
        let slash_count = u64::from_le_bytes(data[214..222].try_into().unwrap());
        let is_active = data[222] != 0;
        Ok(PoolMemberStake {
            version,
            instance_seed,
            stake_id,
            pool_id,
            member_pub,
            relayer_id,
            original_amount,
            current_amount,
            coverage_contribution,
            pool_share_bp,
            accumulated_fees,
            created_at,
            leave_requested_at,
            slash_count,
            is_active,
        })
    }
}

/// Active coverage allocation for a guaranteed withdrawal
#[derive(Debug, Clone)]
pub struct CoverageAllocation {
    pub version: u8,
    /// Unique allocation identifier
    pub allocation_id: pallas::Base,
    /// Pool this allocation is from
    pub pool_id: pallas::Base,
    /// Withdrawal nullifier this covers
    pub withdrawal_nullifier: [u8; 32],
    /// Amount of coverage allocated
    pub amount: u64,
    /// Member IDs that contributed to this coverage
    pub contributing_members: Vec<pallas::Base>,
    /// Block when allocation was created
    pub created_at: u64,
    /// Block when allocation times out (for cleanup)
    pub timeout_height: u64,
    /// Whether this allocation has been executed (success)
    pub executed: bool,
    /// Whether this allocation has been slashed (failure)
    pub slashed: bool,
}

impl CoverageAllocation {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 124 + self.contributing_members.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.version);
        buf.extend_from_slice(&self.allocation_id.to_repr());
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.withdrawal_nullifier);
        buf.extend_from_slice(&self.amount.to_le_bytes());
        // Pattern 2: Vec<pallas::Base> — u8 length prefix + elements
        buf.push(self.contributing_members.len() as u8);
        for m in &self.contributing_members {
            buf.extend_from_slice(&m.to_repr());
        }
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.timeout_height.to_le_bytes());
        buf.push(self.executed as u8);
        buf.push(self.slashed as u8);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 124 {
            return Err(ContractError::IoError(format!(
                "CoverageAllocation: expected at least 124 bytes, got {}",
                data.len()
            )));
        }
        let version = data[0];
        let allocation_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[1..33].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("CoverageAllocation: invalid allocation_id".into()))?;
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[33..65].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("CoverageAllocation: invalid pool_id".into()))?;
        let withdrawal_nullifier: [u8; 32] = data[65..97].try_into().unwrap();
        let amount = u64::from_le_bytes(data[97..105].try_into().unwrap());
        // Pattern 2: Vec<pallas::Base> — u8 length prefix + elements
        let member_count = data[105] as usize;
        let expected = 106 + member_count * 32 + 8 + 8 + 1 + 1;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "CoverageAllocation: expected {} bytes for {} members, got {}",
                expected, member_count, data.len()
            )));
        }
        let mut contributing_members = Vec::with_capacity(member_count);
        for i in 0..member_count {
            let start = 106 + i * 32;
            contributing_members.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(
                    data[start..start + 32].try_into().unwrap(),
                ))
                .ok_or_else(|| {
                    ContractError::IoError(format!(
                        "CoverageAllocation: invalid member[{}]",
                        i
                    ))
                })?,
            );
        }
        let pos = 106 + member_count * 32;
        let created_at = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        let timeout_height = u64::from_le_bytes(data[pos + 8..pos + 16].try_into().unwrap());
        let executed = data[pos + 16] != 0;
        let slashed = data[pos + 17] != 0;
        Ok(CoverageAllocation {
            version,
            allocation_id,
            pool_id,
            withdrawal_nullifier,
            amount,
            contributing_members,
            created_at,
            timeout_height,
            executed,
            slashed,
        })
    }
}

// ============================================================================
// PARAMETER STRUCTS
// ============================================================================

/// Parameters for creating a new pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePoolParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Pool creator/owner public key
    pub owner_pub: PublicKey,
    /// Maximum coverage ratio (e.g., 10000 = 1:1 stake:coverage)
    pub max_coverage_ratio: u32,
    /// Fee percentage for pool operator (in basis points)
    pub operator_fee_bp: u32,
    /// Pool configuration hash (poseidon hash of config params) — ZK public input
    pub pool_config_hash: pallas::Base,
    /// Nonce for uniqueness — ZK public input
    pub nonce: u64,
    /// Derived pool ID from ZK proof — ZK public input
    pub derived_pool_id: pallas::Base,
}

/// Update returned after creating a pool
#[derive(Debug, Clone)]
pub struct CreatePoolUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub pool_id: pallas::Base,
    pub owner_pub: PublicKey,
    pub max_coverage_ratio: u32,
    pub operator_fee_bp: u32,
    pub created_at: u64,
}

impl CreatePoolUpdateV1 {
    pub const ENCODED_SIZE: usize = 112; // 32+32+32+4+4+8

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.owner_pub.to_bytes());
        buf.extend_from_slice(&self.max_coverage_ratio.to_le_bytes());
        buf.extend_from_slice(&self.operator_fee_bp.to_le_bytes());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "CreatePoolUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let instance_seed: [u8; 32] = data[0..32].try_into().unwrap();
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[32..64].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("CreatePoolUpdateV1: invalid pool_id".into()))?;
        let owner_pub = PublicKey::from_bytes(data[64..96].try_into().unwrap())?;
        let max_coverage_ratio = u32::from_le_bytes(data[96..100].try_into().unwrap());
        let operator_fee_bp = u32::from_le_bytes(data[100..104].try_into().unwrap());
        let created_at = u64::from_le_bytes(data[104..112].try_into().unwrap());
        Ok(CreatePoolUpdateV1 {
            instance_seed,
            pool_id,
            owner_pub,
            max_coverage_ratio,
            operator_fee_bp,
            created_at,
        })
    }
}

/// Parameters for joining a pool (staking)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct JoinPoolParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Pool to join
    pub pool_id: pallas::Base,
    /// Stake amount (must meet minimum)
    pub amount: u64,
    /// Relayer ID this member controls
    pub relayer_id: [u8; 32],
    /// Public key of the member joining the pool
    pub member_pub: PublicKey,
    /// Token ID for staking — ZK public input
    pub token_id: pallas::Base,
    /// Nonce for uniqueness — ZK public input
    pub nonce: u64,
    /// Derived member/stake ID from ZK proof — ZK public input
    pub derived_member_id: pallas::Base,
    /// Value commitment X coordinate from ZK proof — ZK public input
    pub value_commit_x: pallas::Base,
    /// Value commitment Y coordinate from ZK proof — ZK public input
    pub value_commit_y: pallas::Base,
}

/// Update returned after joining a pool
#[derive(Debug, Clone)]
pub struct JoinPoolUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub stake_id: pallas::Base,
    pub pool_id: pallas::Base,
    pub member_pub: PublicKey,
    pub relayer_id: [u8; 32],
    pub amount: u64,
    pub coverage_contribution: u64,
    pub pool_share_bp: u32,
    pub total_stake: u64,
    pub member_count: u64,
}

impl JoinPoolUpdateV1 {
    pub const ENCODED_SIZE: usize = 196; // 32+32+32+32+32+8+8+4+8+8

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.member_pub.to_bytes());
        buf.extend_from_slice(&self.relayer_id);
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.coverage_contribution.to_le_bytes());
        buf.extend_from_slice(&self.pool_share_bp.to_le_bytes());
        buf.extend_from_slice(&self.total_stake.to_le_bytes());
        buf.extend_from_slice(&self.member_count.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "JoinPoolUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let instance_seed: [u8; 32] = data[0..32].try_into().unwrap();
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[32..64].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("JoinPoolUpdateV1: invalid stake_id".into()))?;
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[64..96].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("JoinPoolUpdateV1: invalid pool_id".into()))?;
        let member_pub = PublicKey::from_bytes(data[96..128].try_into().unwrap())?;
        let relayer_id: [u8; 32] = data[128..160].try_into().unwrap();
        let amount = u64::from_le_bytes(data[160..168].try_into().unwrap());
        let coverage_contribution = u64::from_le_bytes(data[168..176].try_into().unwrap());
        let pool_share_bp = u32::from_le_bytes(data[176..180].try_into().unwrap());
        let total_stake = u64::from_le_bytes(data[180..188].try_into().unwrap());
        let member_count = u64::from_le_bytes(data[188..196].try_into().unwrap());
        Ok(JoinPoolUpdateV1 {
            instance_seed,
            stake_id,
            pool_id,
            member_pub,
            relayer_id,
            amount,
            coverage_contribution,
            pool_share_bp,
            total_stake,
            member_count,
        })
    }
}

/// Parameters for leaving a pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LeavePoolParamsV1 {
    /// Stake ID to unstake
    pub stake_id: pallas::Base,
}

/// Update returned after leaving a pool
#[derive(Debug, Clone)]
pub struct LeavePoolUpdateV1 {
    pub stake_id: pallas::Base,
    pub payout_amount: u64,
    pub unstake_penalty: u64,
}

impl LeavePoolUpdateV1 {
    pub const ENCODED_SIZE: usize = 48; // 32+8+8

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.payout_amount.to_le_bytes());
        buf.extend_from_slice(&self.unstake_penalty.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "LeavePoolUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("LeavePoolUpdateV1: invalid stake_id".into()))?;
        let payout_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let unstake_penalty = u64::from_le_bytes(data[40..48].try_into().unwrap());
        Ok(LeavePoolUpdateV1 { stake_id, payout_amount, unstake_penalty })
    }
}

/// Parameters for allocating coverage to a withdrawal
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AllocateCoverageParamsV1 {
    /// Pool to allocate from
    pub pool_id: pallas::Base,
    /// Withdrawal nullifier to cover
    pub withdrawal_nullifier: [u8; 32],
    /// Amount of coverage needed
    pub amount: u64,
    /// Timeout height for the withdrawal
    pub timeout_height: u64,
    /// Member public key requesting coverage — ZK public input
    pub member_pub: PublicKey,
    /// Withdrawal ID being covered — ZK public input
    pub withdrawal_id: pallas::Base,
    /// Nonce for uniqueness — ZK public input
    pub nonce: u64,
    /// Derived allocation ID from ZK proof — ZK public input
    pub derived_allocation_id: pallas::Base,
}

/// Update returned after allocating coverage
#[derive(Debug, Clone)]
pub struct AllocateCoverageUpdateV1 {
    pub allocation_id: pallas::Base,
    pub pool_id: pallas::Base,
    pub withdrawal_nullifier: [u8; 32],
    pub amount: u64,
    pub contributing_members: Vec<pallas::Base>,
    pub available_coverage: u64,
    pub allocated_coverage: u64,
    pub timeout_height: u64,
}

impl AllocateCoverageUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 129 + self.contributing_members.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.allocation_id.to_repr());
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.withdrawal_nullifier);
        buf.extend_from_slice(&self.amount.to_le_bytes());
        // Pattern 2: Vec<pallas::Base> — u8 length prefix + elements
        buf.push(self.contributing_members.len() as u8);
        for m in &self.contributing_members {
            buf.extend_from_slice(&m.to_repr());
        }
        buf.extend_from_slice(&self.available_coverage.to_le_bytes());
        buf.extend_from_slice(&self.allocated_coverage.to_le_bytes());
        buf.extend_from_slice(&self.timeout_height.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 129 {
            return Err(ContractError::IoError(format!(
                "AllocateCoverageUpdateV1: expected at least 129 bytes, got {}",
                data.len()
            )));
        }
        let allocation_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("AllocateCoverageUpdateV1: invalid allocation_id".into())
        })?;
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[32..64].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("AllocateCoverageUpdateV1: invalid pool_id".into())
        })?;
        let withdrawal_nullifier: [u8; 32] = data[64..96].try_into().unwrap();
        let amount = u64::from_le_bytes(data[96..104].try_into().unwrap());
        // Pattern 2: Vec<pallas::Base> — u8 length prefix + elements
        let member_count = data[104] as usize;
        let expected = 105 + member_count * 32 + 8 + 8 + 8;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "AllocateCoverageUpdateV1: expected {} bytes for {} members, got {}",
                expected, member_count, data.len()
            )));
        }
        let mut contributing_members = Vec::with_capacity(member_count);
        for i in 0..member_count {
            let start = 105 + i * 32;
            contributing_members.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(
                    data[start..start + 32].try_into().unwrap(),
                ))
                .ok_or_else(|| {
                    ContractError::IoError(format!(
                        "AllocateCoverageUpdateV1: invalid member[{}]",
                        i
                    ))
                })?,
            );
        }
        let pos = 105 + member_count * 32;
        let available_coverage = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        let allocated_coverage = u64::from_le_bytes(data[pos + 8..pos + 16].try_into().unwrap());
        let timeout_height = u64::from_le_bytes(data[pos + 16..pos + 24].try_into().unwrap());
        Ok(AllocateCoverageUpdateV1 {
            allocation_id,
            pool_id,
            withdrawal_nullifier,
            amount,
            contributing_members,
            available_coverage,
            allocated_coverage,
            timeout_height,
        })
    }
}

/// Parameters for releasing coverage after success
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ReleaseCoverageParamsV1 {
    /// Allocation ID to release
    pub allocation_id: pallas::Base,
    /// Pool owner's public key (authorization)
    pub owner_pub: PublicKey,
}

/// Update returned after releasing coverage
#[derive(Debug, Clone)]
pub struct ReleaseCoverageUpdateV1 {
    pub allocation_id: pallas::Base,
    pub released_amount: u64,
    pub available_coverage: u64,
    pub allocated_coverage: u64,
}

impl ReleaseCoverageUpdateV1 {
    pub const ENCODED_SIZE: usize = 56; // 32+8+8+8

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.allocation_id.to_repr());
        buf.extend_from_slice(&self.released_amount.to_le_bytes());
        buf.extend_from_slice(&self.available_coverage.to_le_bytes());
        buf.extend_from_slice(&self.allocated_coverage.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ReleaseCoverageUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let allocation_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("ReleaseCoverageUpdateV1: invalid allocation_id".into())
        })?;
        let released_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let available_coverage = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let allocated_coverage = u64::from_le_bytes(data[48..56].try_into().unwrap());
        Ok(ReleaseCoverageUpdateV1 { allocation_id, released_amount, available_coverage, allocated_coverage })
    }
}

/// Parameters for slashing coverage after failure
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlashCoverageParamsV1 {
    /// Allocation ID to slash
    pub allocation_id: pallas::Base,
    /// Pool owner's public key (authorization)
    pub owner_pub: PublicKey,
    /// Slash amount (amount to give to user as compensation)
    pub slash_amount: u64,
    /// Public key of user to receive compensation
    pub user_pub: PublicKey,
    /// Nonce for uniqueness — ZK public input
    pub nonce: u64,
    /// Derived slash ID from ZK proof — ZK public input
    pub derived_slash_id: pallas::Base,
}

/// Update returned after slashing coverage
#[derive(Debug, Clone)]
pub struct SlashCoverageUpdateV1 {
    pub allocation_id: pallas::Base,
    pub slashed_amount: u64,
    pub compensated_user: [u8; 32],
    pub available_coverage: u64,
    pub allocated_coverage: u64,
}

impl SlashCoverageUpdateV1 {
    pub const ENCODED_SIZE: usize = 88; // 32+8+32+8+8

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.allocation_id.to_repr());
        buf.extend_from_slice(&self.slashed_amount.to_le_bytes());
        buf.extend_from_slice(&self.compensated_user);
        buf.extend_from_slice(&self.available_coverage.to_le_bytes());
        buf.extend_from_slice(&self.allocated_coverage.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "SlashCoverageUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let allocation_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("SlashCoverageUpdateV1: invalid allocation_id".into())
        })?;
        let slashed_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let compensated_user: [u8; 32] = data[40..72].try_into().unwrap();
        let available_coverage = u64::from_le_bytes(data[72..80].try_into().unwrap());
        let allocated_coverage = u64::from_le_bytes(data[80..88].try_into().unwrap());
        Ok(SlashCoverageUpdateV1 {
            allocation_id,
            slashed_amount,
            compensated_user,
            available_coverage,
            allocated_coverage,
        })
    }
}

/// Parameters for claiming accumulated fees
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimFeesParamsV1 {
    /// Stake ID to claim fees for
    pub stake_id: pallas::Base,
    /// Pool owner's public key (authorization)
    pub owner_pub: PublicKey,
}

/// Update returned after claiming fees
#[derive(Debug, Clone)]
pub struct ClaimFeesUpdateV1 {
    pub stake_id: pallas::Base,
    pub claimed_amount: u64,
    pub remaining_fees: u64,
}

impl ClaimFeesUpdateV1 {
    pub const ENCODED_SIZE: usize = 48; // 32+8+8

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.stake_id.to_repr());
        buf.extend_from_slice(&self.claimed_amount.to_le_bytes());
        buf.extend_from_slice(&self.remaining_fees.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ClaimFeesUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let stake_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("ClaimFeesUpdateV1: invalid stake_id".into()))?;
        let claimed_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let remaining_fees = u64::from_le_bytes(data[40..48].try_into().unwrap());
        Ok(ClaimFeesUpdateV1 { stake_id, claimed_amount, remaining_fees })
    }
}

/// Parameters for updating pool configuration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdatePoolConfigParamsV1 {
    /// Pool to update
    pub pool_id: pallas::Base,
    /// Pool owner's public key (authorization)
    pub owner_pub: PublicKey,
    /// New maximum coverage ratio
    pub max_coverage_ratio: Option<u32>,
    /// New operator fee
    pub operator_fee_bp: Option<u32>,
}

/// Update returned after updating pool config
#[derive(Debug, Clone)]
pub struct UpdatePoolConfigUpdateV1 {
    pub pool_id: pallas::Base,
    pub max_coverage_ratio: u32,
    pub operator_fee_bp: u32,
}

impl UpdatePoolConfigUpdateV1 {
    pub const ENCODED_SIZE: usize = 40; // 32+4+4

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.max_coverage_ratio.to_le_bytes());
        buf.extend_from_slice(&self.operator_fee_bp.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "UpdatePoolConfigUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("UpdatePoolConfigUpdateV1: invalid pool_id".into())
        })?;
        let max_coverage_ratio = u32::from_le_bytes(data[32..36].try_into().unwrap());
        let operator_fee_bp = u32::from_le_bytes(data[36..40].try_into().unwrap());
        Ok(UpdatePoolConfigUpdateV1 { pool_id, max_coverage_ratio, operator_fee_bp })
    }
}

// ============================================================================
// REBALANCE POOL SHARES (Phase 2d hardening)
// ============================================================================

/// Parameters for rebalancing pool member shares based on reputation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RebalancePoolSharesParamsV1 {
    /// Pool ID to rebalance
    pub pool_id: pallas::Base,
    /// Pool owner's public key (authorization)
    pub owner_pub: PublicKey,
    /// Member stake IDs to rebalance (caller provides these since DB lacks iteration)
    pub member_ids: Vec<pallas::Base>,
}

/// Update returned after rebalancing pool shares
#[derive(Debug, Clone)]
pub struct RebalancePoolSharesUpdateV1 {
    pub pool_id: pallas::Base,
    pub members_rebalanced: u64,
    pub total_share_bp: u32,
}

impl RebalancePoolSharesUpdateV1 {
    pub const ENCODED_SIZE: usize = 44; // 32+8+4

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.pool_id.to_repr());
        buf.extend_from_slice(&self.members_rebalanced.to_le_bytes());
        buf.extend_from_slice(&self.total_share_bp.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "RebalancePoolSharesUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let pool_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[0..32].try_into().unwrap(),
        ))
        .ok_or_else(|| {
            ContractError::IoError("RebalancePoolSharesUpdateV1: invalid pool_id".into())
        })?;
        let members_rebalanced = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let total_share_bp = u32::from_le_bytes(data[40..44].try_into().unwrap());
        Ok(RebalancePoolSharesUpdateV1 { pool_id, members_rebalanced, total_share_bp })
    }
}
