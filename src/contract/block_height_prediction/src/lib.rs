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

//! Block Height Prediction Market Contract
//!
//! A prediction market for betting on the canonical block height at a specific time.
//!
//! ## Concept
//!
//! Participants bet on what the "official" block height will be at a specific
//! Unix timestamp. The resolution uses DarkFi's PoW blockchain (RandomX) to
//! provide trustless, verifiable randomness.
//!
//! ## How It Works
//!
//! 1. **Create Market**: Creator sets a target timestamp for resolution
//! 2. **Place Bets**: Participants bet on specific block heights (below/exact/above)
//! 3. **Resolution**: After target time + confirmation depth, PoW hash determines
//!    the resolved block height with cryptographic certainty
//! 4. **Claim**: Winners claim proportional payouts from the pool
//!
//! ## Security Model
//!
//! Unlike oracle-based prediction markets, this contract uses DarkFi's
//! proof-of-work as a source of trustless randomness:
//!
//! - **RandomX**: CPU-intensive, memory-hard PoW - hash output is unpredictable
//! - **Confirmation Depth**: K blocks accumulated = attacker needs K consecutive
//! - **Cumulative Entropy**: Combining multiple blocks exponentially increases
//!   manipulation difficulty
//!
//! ## Resolution Algorithm
//!
//! ```rust
//! // After target_time + confirmation_depth blocks:
//! let entropy = cumulative_pow_hash(block_N, ..., block_N+K)
//! let resolved_height = base_height + (entropy % expected_range)
//! ```
//!
//! ## Integration
//!
//! Uses existing DarkFi primitives:
//! - **Money::Burn** for value lock (spend_hook to CreatePosition)
//! - **PoW** via wasm::util for block hash access

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum BlockHeightPredictionFunction {
    /// Initialize contract settings
    InitializeV1 = 0x00,
    /// Create a new prediction market
    CreateMarketV1 = 0x01,
    /// Place a bet/position
    CreatePositionV1 = 0x02,
    /// Resolve market using PoW
    ResolveMarketV1 = 0x03,
    /// Claim winnings after resolution
    ClaimWinningsV1 = 0x04,
    /// Cancel market and refund (before resolution)
    CancelMarketV1 = 0x05,
}

impl TryFrom<u8> for BlockHeightPredictionFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::CreateMarketV1),
            0x02 => Ok(Self::CreatePositionV1),
            0x03 => Ok(Self::ResolveMarketV1),
            0x04 => Ok(Self::ClaimWinningsV1),
            0x05 => Ok(Self::CancelMarketV1),
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
pub const BLOCK_HEIGHT_PREDICTION_MARKETS_TREE: &str = "markets";
/// Stores positions/bets indexed by position_id
pub const BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE: &str = "positions";
/// Stores contract info (fees, config)
pub const BLOCK_HEIGHT_PREDICTION_INFO_TREE: &str = "info";
/// Stores claimed winnings to prevent double-claim
pub const BLOCK_HEIGHT_PREDICTION_CLAIMS_TREE: &str = "claims";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const BLOCK_HEIGHT_PREDICTION_DB_VERSION: &[u8] = b"db_version";
/// Protocol fee in basis points (e.g., 100 = 1%)
pub const BLOCK_HEIGHT_PREDICTION_PROTOCOL_FEE: &[u8] = b"protocol_fee";
/// Default resolution timeout in blocks
pub const BLOCK_HEIGHT_PREDICTION_RESOLUTION_TIMEOUT: &[u8] = b"resolution_timeout";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas create position circuit namespace
pub const BLOCK_HEIGHT_PREDICTION_ZKAS_POSITION_NS: &str = "Position_V1";
/// zkas resolve market circuit namespace
pub const BLOCK_HEIGHT_PREDICTION_ZKAS_RESOLVE_NS: &str = "ResolveMarket_V1";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default protocol fee in basis points (1%)
pub const DEFAULT_PROTOCOL_FEE: u32 = 100;
/// Minimum protocol fee (0.1%)
pub const MIN_PROTOCOL_FEE: u32 = 10;
/// Maximum protocol fee (10%)
pub const MAX_PROTOCOL_FEE: u32 = 1000;
/// Default resolution timeout in blocks (~30 minutes with 120s block time)
pub const DEFAULT_RESOLUTION_TIMEOUT: u32 = 15;
/// Default PoW confirmation depth (6 = high security)
pub const DEFAULT_CONFIRMATION_DEPTH: u8 = 6;
/// Maximum confirmation depth (10 = institutional)
pub const MAX_CONFIRMATION_DEPTH: u8 = 10;
/// Maximum tolerance range (+/- blocks for "close" payout)
pub const MAX_TOLERANCE: u8 = 50;
/// Expected block time in seconds
pub const EXPECTED_BLOCK_TIME: u64 = 120;
