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

//! Roulette Contract
//!
//! A privacy-preserving roulette game with fixed odds betting.
//! Unlike lottery (parimutuel), roulette has fixed maximum payouts,
//! making it ideal for capital staking via BettingStake contract.
//!
//! ## Roulette Rules
//!
//! European Roulette (37 numbers: 0-36):
//! - House edge: 2.7% (from single zero)
//! - En prison rule can reduce to 1.35% for even-money bets
//!
//! American Roulette (38 numbers: 0, 00, 1-36):
//! - House edge: 5.26% (from double zero)
//!
//! ## Bet Types and Odds
//!
//! | Bet Type | Numbers | Payout | European HE | American HE |
//! |----------|---------|--------|-------------|--------------|
//! | Straight | 1 | 35:1 | 2.7% | 5.26% |
//! | Split | 2 | 17:1 | 2.7% | 5.26% |
//! | Street | 3 | 11:1 | 2.7% | 5.26% |
//! | Corner | 4 | 8:1 | 2.7% | 5.26% |
//! | Six Line | 6 | 5:1 | 2.7% | 5.26% |
//! | Dozen | 12 | 2:1 | 2.7% | 5.26% |
//! | Column | 12 | 2:1 | 2.7% | 5.26% |
//! | Even Money | 18 | 1:1 | 2.7% | 5.26% |
//!
//! ## Capital Requirements (vs Lottery)
//!
//! Unlike lottery where jackpot scales with pool, roulette has FIXED maximum payouts:
//! - Maximum straight bet × 35 = max straight payout
//! - Table capital only needs to cover max single spin loss
//! - BettingStake contract works perfectly for this use case
//!
//! This is fundamentally different from lottery where:
//! - Jackpot can exceed collected pool
//! - Parimutuel requires pool = payouts
//! - External capital needed for "fixed" jackpots

pub mod error;
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

#[cfg(feature = "client")]
pub mod client;

/// Capability descriptor for wallet resolver
pub mod capability;

pub use model::*;

// =============================================================================
// CONTRACT FUNCTIONS
// =============================================================================

/// Roulette function enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RouletteFunction {
    /// Initialize roulette table
    InitializeV1 = 0x00,
    /// Player places bet
    PlaceBetV1 = 0x01,
    /// Spin wheel and determine outcome
    SpinWheelV1 = 0x02,
    /// Settle bets and pay winners
    SettleBetsV1 = 0x03,
    /// House closes table
    HouseCloseV1 = 0x04,
}

impl TryFrom<u8> for RouletteFunction {
    type Error = ();

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::PlaceBetV1),
            0x02 => Ok(Self::SpinWheelV1),
            0x03 => Ok(Self::SettleBetsV1),
            0x04 => Ok(Self::HouseCloseV1),
            _ => Err(()),
        }
    }
}

// =============================================================================
// CONSTANTS
// =============================================================================

/// European wheel: 37 numbers (0-36)
pub const EUROPEAN_WHEEL_SIZE: u8 = 37;
/// American wheel: 38 numbers (0, 00, 1-36)
pub const AMERICAN_WHEEL_SIZE: u8 = 38;

/// House edge for European roulette (2.7%)
pub const EUROPEAN_HOUSE_EDGE_BP: u32 = 270;
/// House edge for American roulette (5.26%)
pub const AMERICAN_HOUSE_EDGE_BP: u32 = 526;

/// Database tree names
pub const ROULETTE_CONTRACT_TABLES_TREE: &str = "roulette_tables";
pub const ROULETTE_CONTRACT_BETS_TREE: &str = "roulette_bets";
pub const ROULETTE_CONTRACT_NULLIFIERS_TREE: &str = "roulette_nullifiers";
pub const ROULETTE_CONTRACT_BETS_HISTORY_TREE: &str = "roulette_history";
/// Maximum bet IDs per settle call
pub const ROULETTE_CONTRACT_MAX_SETTLE_BETS: usize = 100;

// zkas circuit namespaces
pub const ROULETTE_CONTRACT_ZKAS_PLACE_BET_NS_V1: &str = "PlaceBet_V1";
pub const ROULETTE_CONTRACT_ZKAS_SETTLE_BET_NS_V1: &str = "SettleBet_V1";
