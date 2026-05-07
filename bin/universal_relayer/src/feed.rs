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

//! Feed market for withdrawal pricing modes

use super::config::FeedMode;
use super::error::{PendingWithdrawal, Result};

/// Represents a withdrawal with feed pricing applied
#[derive(Debug, Clone)]
pub struct PricedWithdrawal {
    /// Original withdrawal data
    pub withdrawal: PendingWithdrawal,
    /// Feed mode used for pricing
    pub feed_mode: FeedMode,
    /// Total amount to lock from user
    pub lock_amount: u64,
    /// Relayer fee earned
    pub relayer_fee: u64,
    /// Guarantee premium (only for Guaranteed mode)
    pub guarantee_premium: u64,
}

/// Feed manager for withdrawal pricing
pub struct FeedManager {
    mode: FeedMode,
    /// Base fee percentage (in basis points, e.g., 100 = 1%)
    fee_percentage: u64,
}

impl FeedManager {
    /// Create a new feed manager
    pub fn new(mode: FeedMode, fee_percentage: u64) -> Self {
        Self { mode, fee_percentage }
    }

    /// Calculate price for a withdrawal
    pub fn price_withdrawal(&self, withdrawal: &PendingWithdrawal) -> Result<PricedWithdrawal> {
        // Calculate base relayer fee
        let relayer_fee = (withdrawal.amount * self.fee_percentage) / 10000;

        match self.mode {
            FeedMode::Standard => Ok(PricedWithdrawal {
                withdrawal: withdrawal.clone(),
                feed_mode: self.mode,
                lock_amount: withdrawal.amount + relayer_fee,
                relayer_fee,
                guarantee_premium: 0,
            }),
            FeedMode::Guaranteed { refund_premium_bp } => {
                let guarantee_premium =
                    (withdrawal.amount * refund_premium_bp as u64) / 10000;
                Ok(PricedWithdrawal {
                    withdrawal: withdrawal.clone(),
                    feed_mode: self.mode,
                    lock_amount: withdrawal.amount + relayer_fee + guarantee_premium,
                    relayer_fee,
                    guarantee_premium,
                })
            }
        }
    }

    /// Get the feed mode
    pub fn mode(&self) -> FeedMode {
        self.mode
    }

    /// Get the fee percentage
    pub fn fee_percentage(&self) -> u64 {
        self.fee_percentage
    }
}

impl PricedWithdrawal {
    /// Check if this is a guaranteed withdrawal
    pub fn is_guaranteed(&self) -> bool {
        matches!(self.feed_mode, FeedMode::Guaranteed { .. })
    }

    /// Get the refund amount for a failed guaranteed withdrawal
    /// This is the original amount + guarantee premium (not the fee)
    pub fn get_refund_amount(&self) -> u64 {
        self.withdrawal.amount + self.guarantee_premium
    }
}
