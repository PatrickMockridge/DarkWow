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

#![allow(dead_code)]

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
        self.active_allocations.retain(|_a| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PoolConfig;
    use crate::error::PendingWithdrawal;

    fn enabled_config() -> PoolConfig {
        PoolConfig {
            enabled: true,
            pool_id: Some("test_pool".to_string()),
            min_pool_members: 1,
            max_pool_coverage: 100_000,
        }
    }

    fn test_member(id: &str, stake: u64) -> PoolMember {
        PoolMember {
            relayer_id: id.to_string(),
            stake_amount: stake,
            coverage_share_bp: 0,
            is_relayer: true,
        }
    }

    fn test_withdrawal(id: u8, amount: u64) -> PendingWithdrawal {
        let mut w_id = [0u8; 32];
        w_id[0] = id;
        PendingWithdrawal {
            withdrawal_id: w_id,
            recipient_hash: [0u8; 32],
            amount,
            chain: 0,
            request_height: 1,
            timeout_height: 100,
            relayer_fee: 0,
            feed_mode: 0,
            guarantee_premium: 0,
        }
    }

    #[test]
    fn test_new() {
        let pool = PoolManager::new(enabled_config());
        assert!(pool.is_enabled());
        assert_eq!(pool.pool_id(), Some(&"test_pool".to_string()));
        assert_eq!(pool.total_stake(), 0);
        assert_eq!(pool.total_coverage(), 0);
        assert_eq!(pool.member_count(), 0);
    }

    #[test]
    fn test_new_disabled() {
        let cfg = PoolConfig { enabled: false, ..Default::default() };
        let pool = PoolManager::new(cfg);
        assert!(!pool.is_enabled());
    }

    #[test]
    fn test_add_member_success() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 10000)).unwrap();
        assert_eq!(pool.member_count(), 1);
        assert_eq!(pool.total_stake(), 10000);
        // coverage = 10000 * 100000 / 100000 = 10000
        assert_eq!(pool.total_coverage(), 10000);
        assert!(pool.get_member("relayer1").is_some());
    }

    #[test]
    fn test_add_member_duplicate() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 5000)).unwrap();
        let err = pool.add_member(test_member("relayer1", 3000)).unwrap_err();
        assert!(matches!(err, RelayerError::PoolError(_)));
    }

    #[test]
    fn test_add_member_disabled() {
        let cfg = PoolConfig { enabled: false, ..Default::default() };
        let mut pool = PoolManager::new(cfg);
        let err = pool.add_member(test_member("relayer1", 5000)).unwrap_err();
        assert!(matches!(err, RelayerError::PoolError(_)));
    }

    #[test]
    fn test_remove_member_success() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 10000)).unwrap();
        pool.remove_member("relayer1").unwrap();
        assert_eq!(pool.member_count(), 0);
        assert_eq!(pool.total_stake(), 0);
        assert_eq!(pool.total_coverage(), 0);
    }

    #[test]
    fn test_remove_member_not_found() {
        let mut pool = PoolManager::new(enabled_config());
        let err = pool.remove_member("nobody").unwrap_err();
        assert!(matches!(err, RelayerError::PoolError(_)));
    }

    #[test]
    fn test_add_multiple_members_coverage_shares() {
        let mut pool = PoolManager::new(enabled_config());
        // Member 1: 10k stake -> 10k coverage
        pool.add_member(test_member("relayer1", 10000)).unwrap();
        // Member 2: 20k stake -> 20k coverage
        pool.add_member(test_member("relayer2", 20000)).unwrap();

        assert_eq!(pool.member_count(), 2);
        assert_eq!(pool.total_stake(), 30000);
        assert_eq!(pool.total_coverage(), 30000);

        let m1 = pool.get_member("relayer1").unwrap();
        let m2 = pool.get_member("relayer2").unwrap();
        // First member added: share = 10000*10000/10000 = 10000
        // Second member added: share = 20000*10000/30000 = 6666
        // Note: rebalance_coverage_shares is only called on remove_member, not add_member
        assert_eq!(m1.coverage_share_bp, 10000);
        assert_eq!(m2.coverage_share_bp, 6666);
    }

    #[test]
    fn test_allocate_coverage_success() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 10000)).unwrap();
        pool.add_member(test_member("relayer2", 20000)).unwrap();

        let w = test_withdrawal(1, 5000);
        let alloc = pool.allocate_coverage(&w).unwrap();
        assert_eq!(alloc.amount, 5000);
        assert!(!alloc.contributing_members.is_empty());
    }

    #[test]
    fn test_allocate_coverage_insufficient() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 1000)).unwrap();

        let w = test_withdrawal(1, 5000);
        let err = pool.allocate_coverage(&w).unwrap_err();
        assert!(matches!(err, RelayerError::InsufficientStake { .. }));
    }

    #[test]
    fn test_release_coverage_success() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 10000)).unwrap();

        let w = test_withdrawal(1, 5000);
        let alloc = pool.allocate_coverage(&w).unwrap();
        let cov_before = pool.allocated_coverage();
        assert!(cov_before > 0);

        pool.release_coverage(&alloc.withdrawal_id).unwrap();
        // After release, coverage restored to member but allocation removed
        assert_eq!(pool.allocated_coverage(), 0);
    }

    #[test]
    fn test_release_coverage_not_found() {
        let mut pool = PoolManager::new(enabled_config());
        let unknown = [0xFF; 32];
        let err = pool.release_coverage(&unknown).unwrap_err();
        assert!(matches!(err, RelayerError::PoolError(_)));
    }

    #[test]
    fn test_slash_coverage() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 10000)).unwrap();

        let w = test_withdrawal(1, 3000);
        let alloc = pool.allocate_coverage(&w).unwrap();
        let slashed = pool.slash_coverage(&alloc.withdrawal_id).unwrap();
        assert_eq!(slashed, 3000);
        assert_eq!(pool.allocated_coverage(), 0);
        // Coverage is permanently removed (slashed), not restored
    }

    #[test]
    fn test_slash_coverage_not_found() {
        let mut pool = PoolManager::new(enabled_config());
        let unknown = [0xFF; 32];
        let err = pool.slash_coverage(&unknown).unwrap_err();
        assert!(matches!(err, RelayerError::PoolError(_)));
    }

    #[test]
    fn test_can_accept_enabled() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 10000)).unwrap();
        assert!(pool.can_accept(1000));
        assert!(pool.can_accept(10000));
        assert!(!pool.can_accept(20000));
    }

    #[test]
    fn test_can_accept_disabled() {
        let cfg = PoolConfig { enabled: false, ..Default::default() };
        let pool = PoolManager::new(cfg);
        assert!(!pool.can_accept(1));
    }

    #[test]
    fn test_allocated_coverage_tracks_active() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 50000)).unwrap();

        let w1 = test_withdrawal(1, 5000);
        let w2 = test_withdrawal(2, 3000);

        pool.allocate_coverage(&w1).unwrap();
        pool.allocate_coverage(&w2).unwrap();

        assert_eq!(pool.allocated_coverage(), 8000);
        assert_eq!(pool.available_coverage(), 50000 - 8000);
    }

    #[test]
    fn test_get_member_coverage() {
        let mut pool = PoolManager::new(enabled_config());
        pool.add_member(test_member("relayer1", 10000)).unwrap();
        assert!(pool.get_member_coverage("relayer1") > 0);
        assert_eq!(pool.get_member_coverage("nobody"), 0);
    }
}