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

//! Stake management for relayer coverage

use serde::{Deserialize, Serialize};

use super::config::StakeConfig;
use super::error::{PendingWithdrawal, RelayerError, Result};

/// Represents an in-flight withdrawal with locked stake
#[derive(Debug, Clone)]
pub struct ActiveWithdrawal {
    /// Unique withdrawal ID
    pub withdrawal_id: [u8; 32],
    /// Amount locked
    pub amount: u64,
    /// Block when timeout occurs
    pub locked_until_block: u64,
}

/// Proof of stake for user verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeProof {
    /// Total staked amount (DAI + NETHER equivalent)
    pub total_stake: u64,
    /// Currently available stake
    pub available_stake: u64,
    /// Currently locked stake for in-flight withdrawals
    pub locked_stake: u64,
    /// Number of active withdrawals
    pub active_withdrawal_count: usize,
}

/// Stake manager for relayer coverage
pub struct StakeManager {
    config: StakeConfig,
    /// Current available stake (total - locked)
    available_stake: u64,
    /// Currently locked stake for pending withdrawals
    locked_stake: u64,
    /// In-flight withdrawals
    active_withdrawals: Vec<ActiveWithdrawal>,
}

impl StakeManager {
    /// Create a new stake manager
    pub fn new(config: StakeConfig) -> Self {
        let total_stake = config.dai_amount + config.nether_amount;
        Self {
            config,
            available_stake: total_stake,
            locked_stake: 0,
            active_withdrawals: Vec::new(),
        }
    }

    /// Check if we can accept a withdrawal of given amount
    pub fn can_accept(&self, amount: u64) -> bool {
        self.config.enabled && self.available_stake >= amount
    }

    /// Lock stake for a new withdrawal
    pub fn lock_for_withdrawal(&mut self, withdrawal: &PendingWithdrawal) -> Result<()> {
        if !self.config.enabled {
            return Err(RelayerError::InsufficientStake {
                available: 0,
                required: withdrawal.amount,
            });
        }

        if !self.can_accept(withdrawal.amount) {
            return Err(RelayerError::InsufficientStake {
                available: self.available_stake,
                required: withdrawal.amount,
            });
        }

        self.available_stake -= withdrawal.amount;
        self.locked_stake += withdrawal.amount;
        self.active_withdrawals.push(ActiveWithdrawal {
            withdrawal_id: withdrawal.withdrawal_id,
            amount: withdrawal.amount,
            locked_until_block: withdrawal.timeout_height,
        });

        tracing::debug!(
            "Locked {} stake for withdrawal {}. Available: {}, Locked: {}",
            withdrawal.amount,
            hex::encode(&withdrawal.withdrawal_id),
            self.available_stake,
            self.locked_stake
        );

        Ok(())
    }

    /// Release stake after successful execution
    pub fn release(&mut self, withdrawal_id: &[u8; 32]) -> Result<u64> {
        let pos = self
            .active_withdrawals
            .iter()
            .position(|w| w.withdrawal_id == *withdrawal_id)
            .ok_or(RelayerError::StakeClaimFailed)?;

        let withdrawal = self.active_withdrawals.remove(pos);
        self.locked_stake -= withdrawal.amount;
        self.available_stake += withdrawal.amount;

        tracing::debug!(
            "Released {} stake for withdrawal {}. Available: {}, Locked: {}",
            withdrawal.amount,
            hex::encode(withdrawal_id),
            self.available_stake,
            self.locked_stake
        );

        Ok(withdrawal.amount)
    }

    /// Slash stake for failed withdrawal (gives to user as compensation)
    /// Returns the slashed amount
    pub fn slash_for_failure(&mut self, withdrawal_id: &[u8; 32]) -> Result<u64> {
        let pos = self
            .active_withdrawals
            .iter()
            .position(|w| w.withdrawal_id == *withdrawal_id)
            .ok_or(RelayerError::StakeClaimFailed)?;

        let withdrawal = self.active_withdrawals.remove(pos);
        self.locked_stake -= withdrawal.amount;
        // Note: slashed amount goes to user, not returned to available pool

        tracing::warn!(
            "Slashed {} stake for failed withdrawal {}",
            withdrawal.amount,
            hex::encode(withdrawal_id)
        );

        Ok(withdrawal.amount)
    }

    /// Get current stake proof for verification
    pub fn get_stake_proof(&self) -> StakeProof {
        StakeProof {
            total_stake: self.config.dai_amount + self.config.nether_amount,
            available_stake: self.available_stake,
            locked_stake: self.locked_stake,
            active_withdrawal_count: self.active_withdrawals.len(),
        }
    }

    /// Get the maximum withdrawal this relayer can accept
    pub fn max_withdrawal(&self) -> u64 {
        if !self.config.enabled {
            return 0;
        }
        self.available_stake.min(self.config.max_withdrawal)
    }

    /// Check if stake is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get total stake
    pub fn total_stake(&self) -> u64 {
        self.config.dai_amount + self.config.nether_amount
    }

    /// Get available stake
    pub fn available_stake(&self) -> u64 {
        self.available_stake
    }

    /// Get locked stake
    pub fn locked_stake(&self) -> u64 {
        self.locked_stake
    }

    /// Clean up expired withdrawals (that timed out but weren't claimed)
    /// Returns the number of cleaned up withdrawals
    pub fn cleanup_expired(&mut self, current_block: u64) -> usize {
        let initial_len = self.active_withdrawals.len();
        self.active_withdrawals.retain(|w| w.locked_until_block > current_block);
        let cleaned = initial_len - self.active_withdrawals.len();

        if cleaned > 0 {
            // Note: We don't release the stake here because they should be claimed by users
            tracing::warn!("Cleaned up {} expired active withdrawals", cleaned);
        }

        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StakeConfig;
    use crate::error::PendingWithdrawal;

    fn test_withdrawal(id: u8, amount: u64, timeout: u64) -> PendingWithdrawal {
        let mut w_id = [0u8; 32];
        w_id[0] = id;
        let mut r_hash = [0u8; 32];
        r_hash[0] = id;
        PendingWithdrawal {
            withdrawal_id: w_id,
            recipient_hash: r_hash,
            amount,
            chain: 0,
            request_height: 1,
            timeout_height: timeout,
            relayer_fee: 0,
            feed_mode: 0,
            guarantee_premium: 0,
        }
    }

    fn enabled_config() -> StakeConfig {
        StakeConfig { enabled: true, dai_amount: 5000, nether_amount: 3000, ..Default::default() }
    }

    #[test]
    fn test_new() {
        let mgr = StakeManager::new(enabled_config());
        assert!(mgr.is_enabled());
        assert_eq!(mgr.total_stake(), 8000);
        assert_eq!(mgr.available_stake(), 8000);
        assert_eq!(mgr.locked_stake(), 0);
    }

    #[test]
    fn test_new_disabled() {
        let cfg = StakeConfig { enabled: false, ..Default::default() };
        let mgr = StakeManager::new(cfg);
        assert!(!mgr.is_enabled());
        assert_eq!(mgr.total_stake(), 0);
        assert_eq!(mgr.available_stake(), 0);
    }

    #[test]
    fn test_can_accept_enabled_with_sufficient_stake() {
        let mgr = StakeManager::new(enabled_config());
        assert!(mgr.can_accept(100));
        assert!(mgr.can_accept(8000));
    }

    #[test]
    fn test_can_accept_disabled() {
        let cfg = StakeConfig { enabled: false, ..Default::default() };
        let mgr = StakeManager::new(cfg);
        assert!(!mgr.can_accept(1));
    }

    #[test]
    fn test_can_accept_insufficient() {
        let mgr = StakeManager::new(enabled_config());
        assert!(!mgr.can_accept(8001));
    }

    #[test]
    fn test_lock_for_withdrawal_success() {
        let mut mgr = StakeManager::new(enabled_config());
        let w = test_withdrawal(1, 500, 50);
        mgr.lock_for_withdrawal(&w).unwrap();
        assert_eq!(mgr.available_stake(), 7500);
        assert_eq!(mgr.locked_stake(), 500);
        assert_eq!(mgr.get_stake_proof().active_withdrawal_count, 1);
    }

    #[test]
    fn test_lock_for_withdrawal_insufficient() {
        let mut mgr = StakeManager::new(enabled_config());
        let w = test_withdrawal(1, 9000, 50);
        let err = mgr.lock_for_withdrawal(&w).unwrap_err();
        assert!(matches!(err, RelayerError::InsufficientStake { .. }));
    }

    #[test]
    fn test_lock_for_withdrawal_disabled() {
        let cfg = StakeConfig { enabled: false, ..Default::default() };
        let mut mgr = StakeManager::new(cfg);
        let w = test_withdrawal(1, 100, 50);
        let err = mgr.lock_for_withdrawal(&w).unwrap_err();
        assert!(matches!(err, RelayerError::InsufficientStake { .. }));
    }

    #[test]
    fn test_release_success() {
        let mut mgr = StakeManager::new(enabled_config());
        let w = test_withdrawal(1, 500, 50);
        mgr.lock_for_withdrawal(&w).unwrap();
        let released = mgr.release(&w.withdrawal_id).unwrap();
        assert_eq!(released, 500);
        assert_eq!(mgr.available_stake(), 8000);
        assert_eq!(mgr.locked_stake(), 0);
    }

    #[test]
    fn test_release_not_found() {
        let mut mgr = StakeManager::new(enabled_config());
        let unknown = [0xFF; 32];
        let err = mgr.release(&unknown).unwrap_err();
        assert!(matches!(err, RelayerError::StakeClaimFailed));
    }

    #[test]
    fn test_slash_for_failure() {
        let mut mgr = StakeManager::new(enabled_config());
        let w = test_withdrawal(1, 500, 50);
        mgr.lock_for_withdrawal(&w).unwrap();
        let slashed = mgr.slash_for_failure(&w.withdrawal_id).unwrap();
        assert_eq!(slashed, 500);
        assert_eq!(mgr.locked_stake(), 0);
        assert_eq!(mgr.available_stake(), 7500);
    }

    #[test]
    fn test_slash_not_found() {
        let mut mgr = StakeManager::new(enabled_config());
        let unknown = [0xFF; 32];
        let err = mgr.slash_for_failure(&unknown).unwrap_err();
        assert!(matches!(err, RelayerError::StakeClaimFailed));
    }

    #[test]
    fn test_get_stake_proof() {
        let mgr = StakeManager::new(enabled_config());
        let proof = mgr.get_stake_proof();
        assert_eq!(proof.total_stake, 8000);
        assert_eq!(proof.available_stake, 8000);
        assert_eq!(proof.locked_stake, 0);
        assert_eq!(proof.active_withdrawal_count, 0);
    }

    #[test]
    fn test_get_stake_proof_after_lock() {
        let mut mgr = StakeManager::new(enabled_config());
        let w = test_withdrawal(1, 500, 50);
        mgr.lock_for_withdrawal(&w).unwrap();
        let proof = mgr.get_stake_proof();
        assert_eq!(proof.total_stake, 8000);
        assert_eq!(proof.available_stake, 7500);
        assert_eq!(proof.locked_stake, 500);
        assert_eq!(proof.active_withdrawal_count, 1);
    }

    #[test]
    fn test_max_withdrawal_enabled() {
        let cfg = StakeConfig {
            enabled: true,
            dai_amount: 10000,
            nether_amount: 0,
            max_withdrawal: 5000,
            ..Default::default()
        };
        let mgr = StakeManager::new(cfg);
        assert_eq!(mgr.max_withdrawal(), 5000);
    }

    #[test]
    fn test_max_withdrawal_disabled() {
        let cfg = StakeConfig { enabled: false, ..Default::default() };
        let mgr = StakeManager::new(cfg);
        assert_eq!(mgr.max_withdrawal(), 0);
    }

    #[test]
    fn test_max_withdrawal_limited_by_available() {
        let cfg = StakeConfig {
            enabled: true,
            dai_amount: 100,
            nether_amount: 0,
            max_withdrawal: 5000,
            ..Default::default()
        };
        let mgr = StakeManager::new(cfg);
        assert_eq!(mgr.max_withdrawal(), 100);
    }

    #[test]
    fn test_cleanup_expired() {
        let mut mgr = StakeManager::new(enabled_config());
        let w1 = test_withdrawal(1, 100, 10);
        let w2 = test_withdrawal(2, 200, 20);
        mgr.lock_for_withdrawal(&w1).unwrap();
        mgr.lock_for_withdrawal(&w2).unwrap();

        let cleaned = mgr.cleanup_expired(15);
        assert_eq!(cleaned, 1);
        // NOTE: cleanup_expired removes from active_withdrawals but does NOT
        // update locked_stake — it stays at 300 (100+200). This is documented
        // in the source: "We don't release the stake here because they should
        // be claimed by users."
        assert_eq!(mgr.locked_stake(), 300);
    }

    #[test]
    fn test_cleanup_none_expired() {
        let mut mgr = StakeManager::new(enabled_config());
        let w = test_withdrawal(1, 100, 50);
        mgr.lock_for_withdrawal(&w).unwrap();
        let cleaned = mgr.cleanup_expired(10);
        assert_eq!(cleaned, 0);
    }
}
