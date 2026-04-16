/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * either version 3 of the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Betting Stake Contract
//!
//! A composable contract that allows capital providers to stake against
//! betting contracts (Dice, Baccarat, Lottery) in exchange for a share
//! of the house edge over time.
//!
//! ## Overview
//!
//! This contract solves the capital requirements problem for betting games:
//!
//! 1. **Betting contracts** (Dice, Baccarat, Lottery) need capital to pay winners
//! 2. **Capital providers** want yield for bearing payout risk
//! 3. **This contract** matches capital supply with capital demand
//!
//! ## How It Works
//!
//! 1. **Stake**: Capital provider stakes funds against a specific betting table
//! 2. **Earn**: Provider earns a share of the house edge from that table's bets
//! 3. **Risk**: Provider absorbs losses when bets pay out (up to stake amount)
//! 4. **Withdraw**: Provider can withdraw stake + accumulated earnings
//!
//! ## Risk/Reward Profile
//!
//! | Scenario | Outcome |
//! |----------|---------|
//! | Table loses money | Staker absorbs loss, stake decreases |
//! | Table breaks even | Staker earns nothing |
//! | Table wins (house wins) | Staker earns house edge share |
//!
//! Over time, with many bets, the law of large numbers means stakers
//! should earn the positive expected value of the house edge.

pub mod error;
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

#[cfg(feature = "client")]
pub mod client;

pub use model::*;

// =============================================================================
// CONTRACT FUNCTIONS
// =============================================================================

/// Betting Stake function enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BettingStakeFunction {
    /// Initialize staking for a betting table
    InitializeV1 = 0x00,
    /// Stake capital against a table
    StakeV1 = 0x01,
    /// Withdraw stake + earnings
    UnstakeV1 = 0x02,
    /// Claim accumulated earnings
    ClaimEarningsV1 = 0x03,
    /// Update stake's risk exposure after a payout
    UpdateRiskV1 = 0x04,
}

impl TryFrom<u8> for BettingStakeFunction {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::StakeV1),
            0x02 => Ok(Self::UnstakeV1),
            0x03 => Ok(Self::ClaimEarningsV1),
            0x04 => Ok(Self::UpdateRiskV1),
            _ => Err(()),
        }
    }
}

// =============================================================================
// CONSTANTS
// =============================================================================

/// Name of the stake registry tree
pub const BETTING_STAKE_REGISTRY_TREE: &str = "staking_registry";
/// Name of the stakes tree
pub const BETTING_STAKE_STAKES_TREE: &str = "staking_stakes";
/// Name of the earnings tree
pub const BETTING_STAKE_EARNINGS_TREE: &str = "staking_earnings";

/// Minimum stake amount
pub const MIN_STAKE_AMOUNT: u64 = 100;
/// Maximum stake per table (as multiple of table's max bet)
pub const MAX_STAKE_RATIO: u64 = 100;
/// Basis points precision for earnings calculations
pub const EARNINGS_BP: u32 = 10000;

// =============================================================================
// STANDARD CONFIGURATIONS
// =============================================================================

/// Risk profiles for different betting types
#[derive(Debug, Clone, Copy)]
pub enum RiskProfile {
    /// Low volatility: Dice (2 outcomes, small variance)
    Low,
    /// Medium volatility: Baccarat (3 outcomes, moderate variance)
    Medium,
    /// High volatility: Lottery (jackpot potential, large variance)
    High,
}

impl RiskProfile {
    /// Returns the risk premium in basis points (additional yield for bearing risk)
    pub fn risk_premium_bp(&self) -> u32 {
        match self {
            Self::Low => 100,      // 1% extra
            Self::Medium => 250,   // 2.5% extra
            Self::High => 500,     // 5% extra
        }
    }
}

// =============================================================================
// ZK CIRCUIT NAMESPACES
// =============================================================================

/// ZK namespace for Init circuit
pub const BETTING_STAKE_ZKAS_INIT_NS: &str = "Init";
/// ZK namespace for Stake circuit
pub const BETTING_STAKE_ZKAS_STAKE_NS: &str = "Stake";
/// ZK namespace for Unstake circuit
pub const BETTING_STAKE_ZKAS_UNSTAKE_NS: &str = "Unstake";
/// ZK namespace for Claim circuit
pub const BETTING_STAKE_ZKAS_CLAIM_NS: &str = "Claim";
/// ZK namespace for UpdateRisk circuit
pub const BETTING_STAKE_ZKAS_UPDATE_RISK_NS: &str = "UpdateRisk";
