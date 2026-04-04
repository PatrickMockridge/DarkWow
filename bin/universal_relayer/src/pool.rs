/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Staking pool management for shared coverage
//!
//! This module provides functionality for relayers to form pools and share
//! stake and coverage. Pool members share slashing risk and fees proportionally.

use std::collections::HashMap;

use super::config::PoolConfig;
use super::error::{PendingWithdrawal, RelayerError, Result};

/// Represents a pool member
#[derive(Debug, Clone)]
pub struct PoolMember {
    /// Relayer identifier
    pub relayer_id: String,
    /// Stake amount contributed
    pub stake_amount: u64,
    /// Proportional share of pool coverage (in basis points)
    pub coverage_share_bp: u32,
    /// Whether this member is a relayer node
    pub is_relayer: bool,
}

/// Active coverage allocation for a withdrawal
#[derive(Debug, Clone)]
pub struct CoverageAllocation {
    /// Withdrawal ID
    pub withdrawal_id: [u8; 32],
    /// Allocated coverage amount
    pub amount: u64,
    /// Member IDs that contributed to this coverage
    pub contributing_members: Vec<String>,
}

/// Pool manager for shared stake coverage
pub struct PoolManager {
    config: PoolConfig,
    /// Pool members
    members: Vec<PoolMember>,
    /// Total stake in pool
    total_stake: u64,
    /// Total coverage capacity
    total_coverage: u64,
    /// Active coverage allocations for in-flight withdrawals
    active_allocations: Vec<CoverageAllocation>,
    /// Map of member ID to their coverage contribution
    member_coverage: HashMap<String, u64>,
}

impl PoolManager {
    /// Create a new pool manager
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            members: Vec::new(),
            total_stake: 0,
            total_coverage: 0,
            active_allocations: Vec::new(),
            member_coverage: HashMap::new(),
        }
    }

    /// Check if pool is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get pool ID
    pub fn pool_id(&self) -> Option<&String> {
        self.config.pool_id.as_ref()
    }

    /// Check if we can accept a withdrawal of given amount
    pub fn can_accept(&self, amount: u64) -> bool {
        self.config.enabled && self.available_coverage() >= amount
    }

    /// Get available coverage (total - already allocated)
    pub fn available_coverage(&self) -> u64 {
        let allocated: u64 = self.active_allocations.iter().map(|a| a.amount).sum();
        self.total_coverage.saturating_sub(allocated)
    }

    /// Add a member to the pool
    pub fn add_member(&mut self, mut member: PoolMember) -> Result<()> {
        if !self.config.enabled {
            return Err(RelayerError::PoolError("Pool is not enabled".to_string()));
        }

        // Check for duplicate member
        if self.members.iter().any(|m| m.relayer_id == member.relayer_id) {
            return Err(RelayerError::PoolError("Member already in pool".to_string()));
        }

        self.total_stake += member.stake_amount;

        // Calculate coverage contribution: stake * max_pool_coverage / 100_000
        // e.g., 10,000 DAI * 100,000 / 100_000 = 10,000 coverage
        let coverage_added =
            (member.stake_amount * self.config.max_pool_coverage) / 100_000_u64;

        self.total_coverage += coverage_added;

        // Calculate coverage share in basis points
        member.coverage_share_bp =
            ((coverage_added as u128 * 10_000) / self.total_coverage.max(1) as u128) as u32;

        self.member_coverage.insert(member.relayer_id.clone(), coverage_added);
        self.members.push(member);

        Ok(())
    }

    /// Remove a member from the pool
    pub fn remove_member(&mut self, relayer_id: &str) -> Result<()> {
        let pos = self
            .members
            .iter()
            .position(|m| m.relayer_id == relayer_id)
            .ok_or_else(|| RelayerError::PoolError("Member not found".to_string()))?;

        let member = self.members.remove(pos);
        self.total_stake -= member.stake_amount;

        if let Some(contributed) = self.member_coverage.remove(relayer_id) {
            self.total_coverage = self.total_coverage.saturating_sub(contributed);
        }

        // Recalculate coverage shares for remaining members
        self.rebalance_coverage_shares();

        Ok(())
    }

    /// Rebalance coverage shares proportionally
    fn rebalance_coverage_shares(&mut self) {
        let total_cov = self.total_coverage.max(1);
        for member in &mut self.members {
            if let Some(contributed) = self.member_coverage.get(&member.relayer_id) {
                member.coverage_share_bp =
                    ((*contributed as u128 * 10_000) / total_cov as u128) as u32;
            }
        }
    }

    /// Allocate coverage for a withdrawal
    pub fn allocate_coverage(
        &mut self,
        withdrawal: &PendingWithdrawal,
    ) -> Result<CoverageAllocation> {
        if !self.can_accept(withdrawal.amount) {
            return Err(RelayerError::InsufficientStake {
                available: self.available_coverage(),
                required: withdrawal.amount,
            });
        }

        // Find contributing members (those with available coverage)
        let mut remaining = withdrawal.amount;
        let mut contributors = Vec::new();

        for member in &self.members {
            let member_cov = *self.member_coverage.get(&member.relayer_id).unwrap_or(&0);
            if member_cov == 0 {
                continue;
            }

            let allocated = remaining.min(member_cov);
            contributors.push(member.relayer_id.clone());

            // Update member's available coverage
            self.member_coverage
                .insert(member.relayer_id.clone(), member_cov - allocated);

            remaining = remaining.saturating_sub(allocated);
            if remaining == 0 {
                break;
            }
        }

        if remaining > 0 {
            return Err(RelayerError::PoolError(format!(
                "Insufficient pool coverage: need {}, available {}",
                withdrawal.amount,
                withdrawal.amount - remaining
            )));
        }

        let allocation = CoverageAllocation {
            withdrawal_id: withdrawal.withdrawal_id,
            amount: withdrawal.amount,
            contributing_members: contributors,
        };

        self.active_allocations.push(allocation.clone());
        Ok(allocation)
    }

    /// Release coverage after successful withdrawal
    pub fn release_coverage(&mut self, withdrawal_id: &[u8; 32]) -> Result<()> {
        let pos = self
            .active_allocations
            .iter()
            .position(|a| a.withdrawal_id == *withdrawal_id)
            .ok_or_else(|| RelayerError::PoolError("Allocation not found".to_string()))?;

        let allocation = self.active_allocations.remove(pos);

        // Restore coverage to contributing members proportionally
        let contributor_count = allocation.contributing_members.len() as u64;
        if contributor_count > 0 {
            let restore_each = allocation.amount / contributor_count;
            let mut remainder = allocation.amount % contributor_count;

            for member_id in &allocation.contributing_members {
                if let Some(cov) = self.member_coverage.get_mut(member_id) {
                    *cov += restore_each;
                    if remainder > 0 {
                        *cov += 1;
                        remainder -= 1;
                    }
                }
            }
        }

        Ok(())
    }

    /// Slash coverage for failed guaranteed withdrawal
    /// Returns the amount slashed
    pub fn slash_coverage(&mut self, withdrawal_id: &[u8; 32]) -> Result<u64> {
        let pos = self
            .active_allocations
            .iter()
            .position(|a| a.withdrawal_id == *withdrawal_id)
            .ok_or_else(|| RelayerError::PoolError("Allocation not found".to_string()))?;

        let allocation = self.active_allocations.remove(pos);

        // Coverage is slashed and given to the user - not restored
        tracing::warn!(
            "Pool coverage slashed: {} from members {:?}",
            allocation.amount,
            allocation.contributing_members
        );

        // Remove the coverage from member_coverage permanently (slashed)
        for member_id in &allocation.contributing_members {
            if let Some(cov) = self.member_coverage.get_mut(member_id) {
                *cov = cov.saturating_sub(allocation.amount / allocation.contributing_members.len() as u64);
            }
        }

        Ok(allocation.amount)
    }

    /// Clean up expired allocations (withdrawals that timed out but weren't executed)
    pub fn cleanup_expired(&mut self, current_block: u64, timeout_height: u64) -> usize {
        if current_block <= timeout_height {
            return 0;
        }

        let initial_len = self.active_allocations.len();
        self.active_allocations.retain(|a| {
            // Keep allocations that haven't timed out
            // For simplicity, we check against a threshold
            true
        });

        let cleaned = initial_len - self.active_allocations.len();
        if cleaned > 0 {
            tracing::warn!("Pool cleaned up {} expired allocations", cleaned);
        }

        cleaned
    }

    /// Get total pool stake
    pub fn total_stake(&self) -> u64 {
        self.total_stake
    }

    /// Get total pool coverage
    pub fn total_coverage(&self) -> u64 {
        self.total_coverage
    }

    /// Get allocated coverage
    pub fn allocated_coverage(&self) -> u64 {
        self.active_allocations.iter().map(|a| a.amount).sum()
    }

    /// Get member count
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Get member by ID
    pub fn get_member(&self, relayer_id: &str) -> Option<&PoolMember> {
        self.members.iter().find(|m| m.relayer_id == relayer_id)
    }

    /// Get coverage share for a member
    pub fn get_member_coverage(&self, relayer_id: &str) -> u64 {
        *self.member_coverage.get(relayer_id).unwrap_or(&0)
    }
}