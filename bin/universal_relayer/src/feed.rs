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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FeedMode;
    use crate::error::PendingWithdrawal;

    fn test_withdrawal(amount: u64) -> PendingWithdrawal {
        let mut w_id = [0u8; 32];
        w_id[0] = 1;
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
    fn test_price_withdrawal_standard() {
        // fee_percentage = 100 = 1% (100/10000 = 1%)
        let mgr = FeedManager::new(FeedMode::Standard, 100);
        let w = test_withdrawal(1000);
        let priced = mgr.price_withdrawal(&w).unwrap();
        assert_eq!(priced.relayer_fee, 10);
        assert_eq!(priced.lock_amount, 1010);
        assert_eq!(priced.guarantee_premium, 0);
        assert!(!priced.is_guaranteed());
    }

    #[test]
    fn test_price_withdrawal_standard_zero_amount() {
        let mgr = FeedManager::new(FeedMode::Standard, 100);
        let w = test_withdrawal(0);
        let priced = mgr.price_withdrawal(&w).unwrap();
        assert_eq!(priced.relayer_fee, 0);
        assert_eq!(priced.lock_amount, 0);
        assert_eq!(priced.guarantee_premium, 0);
    }

    #[test]
    fn test_price_withdrawal_standard_zero_fee() {
        let mgr = FeedManager::new(FeedMode::Standard, 0);
        let w = test_withdrawal(1000);
        let priced = mgr.price_withdrawal(&w).unwrap();
        assert_eq!(priced.relayer_fee, 0);
        assert_eq!(priced.lock_amount, 1000);
    }

    #[test]
    fn test_price_withdrawal_guaranteed() {
        // fee_percentage = 200 = 2%, refund_premium_bp = 300 = 3%
        let mgr = FeedManager::new(FeedMode::Guaranteed { refund_premium_bp: 300 }, 200);
        let w = test_withdrawal(1000);
        let priced = mgr.price_withdrawal(&w).unwrap();
        assert_eq!(priced.relayer_fee, 20);
        assert_eq!(priced.guarantee_premium, 30);
        assert_eq!(priced.lock_amount, 1050);
        assert!(priced.is_guaranteed());
    }

    #[test]
    fn test_price_withdrawal_guaranteed_zero_premium() {
        let mgr = FeedManager::new(FeedMode::Guaranteed { refund_premium_bp: 0 }, 100);
        let w = test_withdrawal(1000);
        let priced = mgr.price_withdrawal(&w).unwrap();
        assert_eq!(priced.relayer_fee, 10);
        assert_eq!(priced.guarantee_premium, 0);
        assert_eq!(priced.lock_amount, 1010);
    }

    #[test]
    fn test_priced_withdrawal_get_refund_amount() {
        let mgr = FeedManager::new(FeedMode::Guaranteed { refund_premium_bp: 500 }, 100);
        let w = test_withdrawal(1000);
        let priced = mgr.price_withdrawal(&w).unwrap();
        // refund = amount + premium = 1000 + 50 = 1050
        assert_eq!(priced.get_refund_amount(), 1050);
    }

    #[test]
    fn test_priced_withdrawal_get_refund_amount_standard() {
        let mgr = FeedManager::new(FeedMode::Standard, 100);
        let w = test_withdrawal(1000);
        let priced = mgr.price_withdrawal(&w).unwrap();
        // refund = amount + 0 = 1000 (no premium in standard mode)
        assert_eq!(priced.get_refund_amount(), 1000);
    }

    #[test]
    fn test_fee_percentage_accessor() {
        let mgr = FeedManager::new(FeedMode::Standard, 150);
        assert_eq!(mgr.fee_percentage(), 150);
    }

    #[test]
    fn test_mode_accessor() {
        let mgr = FeedManager::new(FeedMode::Standard, 100);
        assert_eq!(mgr.mode(), FeedMode::Standard);

        let mgr2 = FeedManager::new(FeedMode::Guaranteed { refund_premium_bp: 200 }, 100);
        assert_eq!(mgr2.mode(), FeedMode::Guaranteed { refund_premium_bp: 200 });
    }

    #[test]
    fn test_price_withdrawal_large_amount() {
        let mgr = FeedManager::new(FeedMode::Standard, 100);
        let w = test_withdrawal(1_000_000_000);
        let priced = mgr.price_withdrawal(&w).unwrap();
        assert_eq!(priced.relayer_fee, 10_000_000);
        assert_eq!(priced.lock_amount, 1_010_000_000);
    }

    #[test]
    fn test_price_withdrawal_rounding_down() {
        // fee_percentage = 1 = 0.01% (1/10000)
        let mgr = FeedManager::new(FeedMode::Standard, 1);
        let w = test_withdrawal(500);
        // 500 * 1 / 10000 = 0 (integer division truncates)
        let priced = mgr.price_withdrawal(&w).unwrap();
        assert_eq!(priced.relayer_fee, 0);
        assert_eq!(priced.lock_amount, 500);
        assert!(!priced.is_guaranteed());
    }
}
