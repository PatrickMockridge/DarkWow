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
    crypto::PublicKey,
    pasta::pallas,
};

/// Pool stake registry - one per pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PoolStakeRegistry {
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

/// Individual member stake position in a pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PoolMemberStake {
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

/// Active coverage allocation for a guaranteed withdrawal
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CoverageAllocation {
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

// ============================================================================
// PARAMETER STRUCTS
// ============================================================================

/// Parameters for creating a new pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePoolParamsV1 {
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePoolUpdateV1 {
    pub pool_id: pallas::Base,
    pub owner_pub: PublicKey,
    pub max_coverage_ratio: u32,
    pub operator_fee_bp: u32,
    pub created_at: u64,
}

/// Parameters for joining a pool (staking)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct JoinPoolParamsV1 {
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct JoinPoolUpdateV1 {
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

/// Parameters for leaving a pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LeavePoolParamsV1 {
    /// Stake ID to unstake
    pub stake_id: pallas::Base,
}

/// Update returned after leaving a pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LeavePoolUpdateV1 {
    pub stake_id: pallas::Base,
    pub payout_amount: u64,
    pub unstake_penalty: u64,
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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

/// Parameters for releasing coverage after success
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ReleaseCoverageParamsV1 {
    /// Allocation ID to release
    pub allocation_id: pallas::Base,
}

/// Update returned after releasing coverage
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ReleaseCoverageUpdateV1 {
    pub allocation_id: pallas::Base,
    pub released_amount: u64,
    pub available_coverage: u64,
    pub allocated_coverage: u64,
}

/// Parameters for slashing coverage after failure
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlashCoverageParamsV1 {
    /// Allocation ID to slash
    pub allocation_id: pallas::Base,
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlashCoverageUpdateV1 {
    pub allocation_id: pallas::Base,
    pub slashed_amount: u64,
    pub compensated_user: [u8; 32],
    pub available_coverage: u64,
    pub allocated_coverage: u64,
}

/// Parameters for claiming accumulated fees
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimFeesParamsV1 {
    /// Stake ID to claim fees for
    pub stake_id: pallas::Base,
}

/// Update returned after claiming fees
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimFeesUpdateV1 {
    pub stake_id: pallas::Base,
    pub claimed_amount: u64,
    pub remaining_fees: u64,
}

/// Parameters for updating pool configuration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdatePoolConfigParamsV1 {
    /// Pool to update
    pub pool_id: pallas::Base,
    /// New maximum coverage ratio
    pub max_coverage_ratio: Option<u32>,
    /// New operator fee
    pub operator_fee_bp: Option<u32>,
}

/// Update returned after updating pool config
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdatePoolConfigUpdateV1 {
    pub pool_id: pallas::Base,
    pub max_coverage_ratio: u32,
    pub operator_fee_bp: u32,
}

// ============================================================================
// REBALANCE POOL SHARES (Phase 2d hardening)
// ============================================================================

/// Parameters for rebalancing pool member shares based on reputation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RebalancePoolSharesParamsV1 {
    /// Pool ID to rebalance
    pub pool_id: pallas::Base,
    /// Member stake IDs to rebalance (caller provides these since DB lacks iteration)
    pub member_ids: Vec<pallas::Base>,
}

/// Update returned after rebalancing pool shares
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RebalancePoolSharesUpdateV1 {
    pub pool_id: pallas::Base,
    pub members_rebalanced: u64,
    pub total_share_bp: u32,
}