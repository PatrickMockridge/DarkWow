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

//! Prediction Market Contract
//!
//! A modular prediction market contract that integrates with existing DarkFi
//! infrastructure for provably fair outcome resolution.
//!
//! ## Architecture
//!
//! This contract uses existing DarkFi primitives:
//! - **Money::Burn** for placing bets (value lock)
//! - **Oracle::AttestValue** for outcome resolution
//! - **DAO::AuthMoneyTransfer** for dispute resolution
//! - **DarkToshi Dice** primitives for random resolution fallback
//!
//! ## Key Concepts
//!
//! - **Markets**: Each prediction market has an outcome space (e.g., YES/NO, or multiple options)
//! - **Positions**: Tokens representing a share in an outcome
//! - **Liquidity**: Market makers provide liquidity, earn fees
//! - **Resolution**: Oracle attests the outcome, winners determined
//!
//! ## Integration Pattern
//!
//! A complete prediction market transaction:
//! 1. Money::Burn parent call locks bet value, sets spend_hook to CreatePositionV1
//! 2. CreatePositionV1 child call creates a position token representing the bet
//! 3. Oracle attests outcome (off-chain or via Oracle contract)
//! 4. ResolveMarketV1 verifies oracle and distributes winnings

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum PredictionMarketFunction {
    /// Initialize a new prediction market
    InitializeV1 = 0x00,
    /// Create a new market
    CreateMarketV1 = 0x01,
    /// Place a bet/position (called as child of Money::Burn)
    CreatePositionV1 = 0x02,
    /// Add liquidity to a market
    AddLiquidityV1 = 0x03,
    /// Remove liquidity from a market
    RemoveLiquidityV1 = 0x04,
    /// Resolve market with oracle attestation
    ResolveMarketV1 = 0x05,
    /// Cancel market (only before resolution)
    CancelMarketV1 = 0x06,
    /// Claim winnings after resolution
    ClaimWinningsV1 = 0x07,
    /// Withdraw liquidity provider fees
    WithdrawFeesV1 = 0x08,
}

impl TryFrom<u8> for PredictionMarketFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::CreateMarketV1),
            0x02 => Ok(Self::CreatePositionV1),
            0x03 => Ok(Self::AddLiquidityV1),
            0x04 => Ok(Self::RemoveLiquidityV1),
            0x05 => Ok(Self::ResolveMarketV1),
            0x06 => Ok(Self::CancelMarketV1),
            0x07 => Ok(Self::ClaimWinningsV1),
            0x08 => Ok(Self::WithdrawFeesV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Stores market configurations indexed by market_id
pub const PREDICTION_CONTRACT_MARKETS_TREE: &str = "markets";
/// Stores positions/bets indexed by position_id
pub const PREDICTION_CONTRACT_POSITIONS_TREE: &str = "positions";
/// Stores liquidity provider shares
pub const PREDICTION_CONTRACT_LIQUIDITY_TREE: &str = "liquidity";
/// Stores contract info (fees, config)
pub const PREDICTION_CONTRACT_INFO_TREE: &str = "info";
/// Stores resolved outcomes
pub const PREDICTION_CONTRACT_RESOLUTIONS_TREE: &str = "resolutions";
/// Stores pending oracle attestations
pub const PREDICTION_CONTRACT_PENDING_TREE: &str = "pending";
/// Stores claimed winnings to prevent double-claim
pub const PREDICTION_CONTRACT_CLAIMS_TREE: &str = "claims";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const PREDICTION_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Protocol fee in basis points (e.g., 100 = 1%)
pub const PREDICTION_CONTRACT_PROTOCOL_FEE: &[u8] = b"protocol_fee";
/// Oracle contract ID for resolution
pub const PREDICTION_CONTRACT_ORACLE_ID: &[u8] = b"oracle_id";
/// Default resolution timeout in blocks
pub const PREDICTION_CONTRACT_RESOLUTION_TIMEOUT: &[u8] = b"resolution_timeout";
/// Liquidity provider fee in basis points
pub const PREDICTION_CONTRACT_LP_FEE: &[u8] = b"lp_fee";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas create position circuit namespace
pub const PREDICTION_CONTRACT_ZKAS_POSITION_NS: &str = "Position_V1";
/// zkas resolve market circuit namespace
pub const PREDICTION_CONTRACT_ZKAS_RESOLVE_NS: &str = "ResolveMarket_V1";
/// zkas add liquidity circuit namespace
pub const PREDICTION_CONTRACT_ZKAS_LIQUIDITY_NS: &str = "AddLiquidity_V1";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default protocol fee in basis points (1%)
pub const DEFAULT_PROTOCOL_FEE: u32 = 100;
/// Minimum protocol fee (0.1%)
pub const MIN_PROTOCOL_FEE: u32 = 10;
/// Maximum protocol fee (10%)
pub const MAX_PROTOCOL_FEE: u32 = 1000;
/// Default liquidity provider fee in basis points (2%)
pub const DEFAULT_LP_FEE: u32 = 200;
/// Default resolution timeout in blocks (1000 ≈ 1 week)
pub const DEFAULT_RESOLUTION_TIMEOUT: u32 = 1000;
/// Maximum number of outcomes in a market
pub const MAX_OUTCOMES: u8 = 20;

/// Outcome types for markets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeType {
    Binary,     // YES/NO only
    Discrete,   // Multiple discrete outcomes
}

/// Market states in the state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketState {
    Active,       // Market is accepting bets
    Frozen,       // Market is frozen (oracle issues)
    Resolved,     // Oracle has attested outcome
    Cancelled,    // Market was cancelled before resolution
    Disputed,     // Outcome is being disputed via DAO
}