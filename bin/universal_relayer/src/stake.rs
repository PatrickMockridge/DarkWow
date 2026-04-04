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
