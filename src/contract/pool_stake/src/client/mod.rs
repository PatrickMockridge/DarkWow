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

//! Pool Stake Contract Client API
//!
//! This module provides the client-side API for building Pool Stake contract calls.

pub mod zkbins;

pub mod create_pool;
pub mod join_pool;
pub mod allocate_coverage;
pub mod slash_coverage;

use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};

use crate::model::{
    AllocateCoverageParamsV1, ClaimFeesParamsV1, CreatePoolParamsV1, JoinPoolParamsV1,
    LeavePoolParamsV1, RebalancePoolSharesParamsV1, ReleaseCoverageParamsV1,
    SlashCoverageParamsV1, UpdatePoolConfigParamsV1,
};

/// Client-side pool note for tracking pools
#[derive(Debug, Clone)]
pub struct PoolNote {
    pub pool_id: pallas::Base,
    pub owner_pub: PublicKey,
    pub max_coverage_ratio: u32,
    pub operator_fee_bp: u32,
    pub total_stake: u64,
    pub member_count: u64,
}

/// Client-side member stake note
#[derive(Debug, Clone)]
pub struct MemberStakeNote {
    pub stake_id: pallas::Base,
    pub pool_id: pallas::Base,
    pub member_pub: PublicKey,
    pub relayer_id: [u8; 32],
    pub original_amount: u64,
    pub current_amount: u64,
    pub coverage_contribution: u64,
    pub pool_share_bp: u32,
    pub accumulated_fees: u64,
    pub is_active: bool,
}

/// Client-side coverage allocation note
#[derive(Debug, Clone)]
pub struct CoverageNote {
    pub allocation_id: pallas::Base,
    pub pool_id: pallas::Base,
    pub withdrawal_nullifier: [u8; 32],
    pub amount: u64,
    pub timeout_height: u64,
    pub executed: bool,
    pub slashed: bool,
}

/// Own stake with secret for operations
pub struct OwnMemberStake {
    pub note: MemberStakeNote,
    pub secret: SecretKey,
}

/// Builder for creating pool calls
pub struct CreatePoolV1Builder {
    owner_pub: PublicKey,
    max_coverage_ratio: u32,
    operator_fee_bp: u32,
}

impl CreatePoolV1Builder {
    /// Create a new CreatePoolV1 builder
    pub fn new(owner_pub: PublicKey) -> Self {
        Self { owner_pub, max_coverage_ratio: 10000, operator_fee_bp: 100 }
    }

    /// Set maximum coverage ratio (e.g., 10000 = 1:1 stake:coverage)
    pub fn max_coverage_ratio(mut self, ratio: u32) -> Self {
        self.max_coverage_ratio = ratio;
        self
    }

    /// Set operator fee in basis points
    pub fn operator_fee(mut self, fee_bp: u32) -> Self {
        self.operator_fee_bp = fee_bp;
        self
    }

    /// Build the create pool parameters
    pub fn build(&self) -> CreatePoolParamsV1 {
        CreatePoolParamsV1 {
            owner_pub: self.owner_pub,
            max_coverage_ratio: self.max_coverage_ratio,
            operator_fee_bp: self.operator_fee_bp,
            pool_config_hash: pallas::Base::zero(),
            nonce: 0,
            derived_pool_id: pallas::Base::zero(),
            instance_seed: [0u8; 32],
        }
    }
}

/// Builder for joining a pool
pub struct JoinPoolV1Builder {
    pool_id: pallas::Base,
    amount: u64,
    relayer_id: [u8; 32],
    member_pub: PublicKey,
}

impl JoinPoolV1Builder {
    /// Create a new JoinPoolV1 builder
    pub fn new(pool_id: pallas::Base, amount: u64, member_pub: PublicKey) -> Self {
        Self { pool_id, amount, relayer_id: [0u8; 32], member_pub }
    }

    /// Set the relayer ID
    pub fn relayer_id(mut self, relayer_id: [u8; 32]) -> Self {
        self.relayer_id = relayer_id;
        self
    }

    /// Build the join pool parameters
    pub fn build(&self) -> JoinPoolParamsV1 {
        JoinPoolParamsV1 {
            pool_id: self.pool_id,
            amount: self.amount,
            relayer_id: self.relayer_id,
            member_pub: self.member_pub,
            token_id: pallas::Base::zero(),
            instance_seed: [0u8; 32],
            nonce: 0,
            derived_member_id: pallas::Base::zero(),
            value_commit_x: pallas::Base::zero(),
            value_commit_y: pallas::Base::zero(),
        }
    }

    /// Build params and note
    pub fn build_with_note(&self) -> (JoinPoolParamsV1, MemberStakeNote) {
        let stake_id = poseidon_hash([
            self.pool_id,
            self.member_pub.x().expect("pk not identity"),
            self.member_pub.y().expect("pk not identity"),
            pallas::Base::from(self.amount),
        ]);

        let note = MemberStakeNote {
            stake_id,
            pool_id: self.pool_id,
            member_pub: self.member_pub,
            relayer_id: self.relayer_id,
            original_amount: self.amount,
            current_amount: self.amount,
            coverage_contribution: self.amount, // Assuming 1:1 for simplicity
            pool_share_bp: 0, // Filled by contract
            accumulated_fees: 0,
            is_active: true,
        };

        (self.build(), note)
    }
}

/// Builder for leaving a pool
pub struct LeavePoolV1Builder {
    stake_id: pallas::Base,
}

impl LeavePoolV1Builder {
    /// Create a new LeavePoolV1 builder
    pub fn new(stake_id: pallas::Base) -> Self {
        Self { stake_id }
    }

    /// Build the leave pool parameters
    pub fn build(&self) -> LeavePoolParamsV1 {
        LeavePoolParamsV1 { stake_id: self.stake_id }
    }
}

/// Builder for allocating coverage
pub struct AllocateCoverageV1Builder {
    pool_id: pallas::Base,
    withdrawal_nullifier: [u8; 32],
    amount: u64,
    timeout_height: u64,
    member_pub: PublicKey,
}

impl AllocateCoverageV1Builder {
    /// Create a new AllocateCoverageV1 builder
    pub fn new(pool_id: pallas::Base, withdrawal_nullifier: [u8; 32], amount: u64) -> Self {
        // Default to a dummy public key (replaced by actual proof generation)
        let dummy_pub = PublicKey::from_secret(
            SecretKey::from_bytes([1u8; 32]).unwrap()
        );
        Self { pool_id, withdrawal_nullifier, amount, timeout_height: 0, member_pub: dummy_pub }
    }

    /// Set the member public key
    pub fn member_pub(mut self, pub_key: PublicKey) -> Self {
        self.member_pub = pub_key;
        self
    }

    /// Set timeout height for the coverage
    pub fn timeout_height(mut self, height: u64) -> Self {
        self.timeout_height = height;
        self
    }

    /// Build the allocate coverage parameters
    pub fn build(&self) -> AllocateCoverageParamsV1 {
        AllocateCoverageParamsV1 {
            pool_id: self.pool_id,
            withdrawal_nullifier: self.withdrawal_nullifier,
            amount: self.amount,
            timeout_height: self.timeout_height,
            member_pub: self.member_pub,
            withdrawal_id: pallas::Base::zero(),
            nonce: 0,
            derived_allocation_id: pallas::Base::zero(),
        }
    }
}

/// Builder for releasing coverage
pub struct ReleaseCoverageV1Builder {
    allocation_id: pallas::Base,
    owner_pub: PublicKey,
}

impl ReleaseCoverageV1Builder {
    /// Create a new ReleaseCoverageV1 builder
    pub fn new(allocation_id: pallas::Base, owner_pub: PublicKey) -> Self {
        Self { allocation_id, owner_pub }
    }

    /// Build the release coverage parameters
    pub fn build(&self) -> ReleaseCoverageParamsV1 {
        ReleaseCoverageParamsV1 { allocation_id: self.allocation_id, owner_pub: self.owner_pub }
    }
}

/// Builder for slashing coverage
pub struct SlashCoverageV1Builder {
    allocation_id: pallas::Base,
    slash_amount: u64,
    user_pub: PublicKey,
    owner_pub: PublicKey,
}

impl SlashCoverageV1Builder {
    /// Create a new SlashCoverageV1 builder
    pub fn new(allocation_id: pallas::Base, slash_amount: u64, user_pub: PublicKey, owner_pub: PublicKey) -> Self {
        Self { allocation_id, slash_amount, user_pub, owner_pub }
    }

    /// Build the slash coverage parameters
    pub fn build(&self) -> SlashCoverageParamsV1 {
        SlashCoverageParamsV1 {
            allocation_id: self.allocation_id,
            owner_pub: self.owner_pub,
            slash_amount: self.slash_amount,
            user_pub: self.user_pub,
            nonce: 0,
            derived_slash_id: pallas::Base::zero(),
        }
    }
}

/// Builder for claiming fees
pub struct ClaimFeesV1Builder {
    stake_id: pallas::Base,
    owner_pub: PublicKey,
}

impl ClaimFeesV1Builder {
    /// Create a new ClaimFeesV1 builder
    pub fn new(stake_id: pallas::Base, owner_pub: PublicKey) -> Self {
        Self { stake_id, owner_pub }
    }

    /// Build the claim fees parameters
    pub fn build(&self) -> ClaimFeesParamsV1 {
        ClaimFeesParamsV1 { stake_id: self.stake_id, owner_pub: self.owner_pub }
    }
}

/// Builder for updating pool configuration
pub struct UpdatePoolConfigV1Builder {
    pool_id: pallas::Base,
    owner_pub: PublicKey,
    max_coverage_ratio: Option<u32>,
    operator_fee_bp: Option<u32>,
}

impl UpdatePoolConfigV1Builder {
    /// Create a new UpdatePoolConfigV1 builder
    pub fn new(pool_id: pallas::Base, owner_pub: PublicKey) -> Self {
        Self { pool_id, owner_pub, max_coverage_ratio: None, operator_fee_bp: None }
    }

    /// Set new maximum coverage ratio
    pub fn max_coverage_ratio(mut self, ratio: u32) -> Self {
        self.max_coverage_ratio = Some(ratio);
        self
    }

    /// Set new operator fee
    pub fn operator_fee(mut self, fee_bp: u32) -> Self {
        self.operator_fee_bp = Some(fee_bp);
        self
    }

    /// Build the update pool config parameters
    pub fn build(&self) -> UpdatePoolConfigParamsV1 {
        UpdatePoolConfigParamsV1 {
            pool_id: self.pool_id,
            owner_pub: self.owner_pub,
            max_coverage_ratio: self.max_coverage_ratio,
            operator_fee_bp: self.operator_fee_bp,
        }
    }
}

/// Validate minimum stake amount
pub fn validate_min_stake(amount: u64) -> Result<(), crate::error::PoolStakeError> {
    if amount < 1_000_000 {
        return Err(crate::error::PoolStakeError::InsufficientStake(1_000_000))
    }
    Ok(())
}

/// Validate coverage ratio
pub fn validate_coverage_ratio(ratio: u32) -> Result<(), crate::error::PoolStakeError> {
    if ratio == 0 {
        return Err(crate::error::PoolStakeError::InvalidCoverageRatio)
    }
    if ratio > 10000 {
        return Err(crate::error::PoolStakeError::InvalidCoverageRatio)
    }
    Ok(())
}

/// Validate operator fee basis points
pub fn validate_operator_fee(fee_bp: u32) -> Result<(), crate::error::PoolStakeError> {
    if fee_bp > 1000 {
        // Max 10%
        return Err(crate::error::PoolStakeError::InvalidParams("Operator fee exceeds maximum 10%".to_string()))
    }
    Ok(())
}

/// Calculate potential coverage from stake amount
pub fn calculate_coverage(stake_amount: u64, max_coverage_ratio: u32) -> u64 {
    (stake_amount * (max_coverage_ratio as u64)) / 10000
}

/// Builder for rebalancing pool shares
pub struct RebalancePoolSharesV1Builder {
    pool_id: pallas::Base,
    owner_pub: PublicKey,
    member_ids: Vec<pallas::Base>,
}

impl RebalancePoolSharesV1Builder {
    pub fn new(pool_id: pallas::Base, owner_pub: PublicKey) -> Self {
        Self { pool_id, owner_pub, member_ids: vec![] }
    }

    pub fn member_ids(mut self, ids: Vec<pallas::Base>) -> Self {
        self.member_ids = ids;
        self
    }

    pub fn add_member(mut self, id: pallas::Base) -> Self {
        self.member_ids.push(id);
        self
    }

    pub fn build(self) -> RebalancePoolSharesParamsV1 {
        RebalancePoolSharesParamsV1 {
            pool_id: self.pool_id,
            owner_pub: self.owner_pub,
            member_ids: self.member_ids,
        }
    }
}