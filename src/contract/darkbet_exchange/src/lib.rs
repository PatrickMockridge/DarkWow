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

//! DarkBet Exchange Contract
//!
//! A decentralized betting exchange supporting two modes:
//!
//! ## Order-Book Mode (market_type = 0)
//!
//! Peer-to-peer betting via DEX-style order matching:
//! - **Back**: Bet that an outcome WILL happen (odds determine payout)
//! - **Lay**: Bet that an outcome will NOT happen (you become the bookie)
//! - **Matching**: DEX matches back orders with lay orders at agreed odds
//!
//! ## AMM Pool Mode (market_type = 1)
//!
//! Automated market making via constant-product formula:
//! - **Positions**: Buy shares in an outcome at AMM-calculated price
//! - **Liquidity Providers**: Supply liquidity, earn protocol + LP fees
//! - **Settlement**: Oracle resolves, winners claim from pool
//!
//! ## Composability
//!
//! Both modes compose with:
//! - BettingStake for liquidity provision
//! - Oracle for event resolution
//! - DAO-Escrow for governance and treasury

use dwow_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum DarkbetFunction {
    // ---- Market creation ----
    /// Create a new betting market (order-book or AMM)
    CreateMarketV1 = 0x00,

    // ---- Order-book mode ----
    /// Place a back order (bet for)
    PlaceBackV1 = 0x01,
    /// Place a lay order (bet against)
    PlaceLayV1 = 0x02,
    /// Match back and lay orders
    MatchOrdersV1 = 0x03,

    // ---- AMM mode ----
    /// Buy a position in an AMM pool market
    BuyPositionV1 = 0x07,
    /// Add liquidity to an AMM pool
    AddLiquidityV1 = 0x08,
    /// Remove liquidity from an AMM pool
    RemoveLiquidityV1 = 0x09,
    /// Claim winnings from a winning position
    ClaimWinningsV1 = 0x0A,

    // ---- Common ----
    /// Oracle resolves the market
    ResolveMarketV1 = 0x04,
    /// Distribute winnings to winners
    SettleMarketV1 = 0x05,
    /// Cancel an unmatched order (order-book mode)
    CancelOrderV1 = 0x06,
}

impl TryFrom<u8> for DarkbetFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::CreateMarketV1),
            0x01 => Ok(Self::PlaceBackV1),
            0x02 => Ok(Self::PlaceLayV1),
            0x03 => Ok(Self::MatchOrdersV1),
            0x04 => Ok(Self::ResolveMarketV1),
            0x05 => Ok(Self::SettleMarketV1),
            0x06 => Ok(Self::CancelOrderV1),
            0x07 => Ok(Self::BuyPositionV1),
            0x08 => Ok(Self::AddLiquidityV1),
            0x09 => Ok(Self::RemoveLiquidityV1),
            0x0A => Ok(Self::ClaimWinningsV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Internal contract errors
pub mod error;

/// Capability descriptor for wallet resolver
pub mod capability;
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

/// Stores market details indexed by market_id
pub const DARKBET_EXCHANGE_MARKETS_TREE: &str = "darkbet_markets";
/// Stores back orders indexed by order_id (order-book mode)
pub const DARKBET_EXCHANGE_BACK_ORDERS_TREE: &str = "darkbet_back_orders";
/// Stores lay orders indexed by order_id (order-book mode)
pub const DARKBET_EXCHANGE_LAY_ORDERS_TREE: &str = "darkbet_lay_orders";
/// Stores matched bets indexed by match_id (order-book mode)
pub const DARKBET_EXCHANGE_MATCHES_TREE: &str = "darkbet_matches";
/// Stores positions indexed by position_id (AMM mode)
pub const DARKBET_EXCHANGE_POSITIONS_TREE: &str = "darkbet_positions";
/// Stores LP shares indexed by lp_share_id (AMM mode)
pub const DARKBET_EXCHANGE_LP_SHARES_TREE: &str = "darkbet_lp_shares";
/// Stores nullifiers to prevent double-spending
pub const DARKBET_EXCHANGE_NULLIFIERS_TREE: &str = "darkbet_nullifiers";
/// Stores contract info (version, config)
pub const DARKBET_EXCHANGE_INFO_TREE: &str = "darkbet_info";

// Keys inside the info tree
pub const DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID: &[u8] = b"promissory_note_cid";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum commission rate in basis points (2%)
pub const DARKBET_EXCHANGE_COMMISSION_BP: u32 = 200;
/// Minimum order size
pub const DARKBET_EXCHANGE_MIN_ORDER_SIZE: u64 = 10;
/// Maximum market lifetime in blocks (~1 week at 5min blocks)
pub const DARKBET_EXCHANGE_MAX_MARKET_LIFETIME: u64 = 2016;

// Default fees for AMM mode
/// Default protocol fee in basis points (1%)
pub const DEFAULT_PROTOCOL_FEE: u32 = 100;
/// Default LP fee in basis points (2%)
pub const DEFAULT_LP_FEE: u32 = 200;
/// Minimum protocol fee (0.1%)
pub const MIN_PROTOCOL_FEE: u32 = 10;
/// Maximum protocol fee (10%)
pub const MAX_PROTOCOL_FEE: u32 = 1000;
/// Maximum match IDs per settle_market call
pub const DARKBET_EXCHANGE_MAX_SETTLE_MATCHES: usize = 100;

// =============================================================================
// ZK CIRCUIT NAMESPACES
// =============================================================================

/// ZK namespace for CreateMarket circuit
pub const DARKBET_EXCHANGE_ZKAS_CREATE_MARKET_NS: &str = "CreateMarket";
/// ZK namespace for BuyPosition circuit
pub const DARKBET_EXCHANGE_ZKAS_BUY_POSITION_NS: &str = "BuyPosition";
/// ZK namespace for ClaimWinnings circuit
pub const DARKBET_EXCHANGE_ZKAS_CLAIM_WINNINGS_NS: &str = "ClaimWinnings";
/// ZK namespace for AddLiquidity circuit
pub const DARKBET_EXCHANGE_ZKAS_ADD_LIQUIDITY_NS: &str = "AddLiquidity";

// V2 circuit namespaces (HAZOP RC3: domain separation)
pub const DARKBET_EXCHANGE_ZKAS_CREATE_MARKET_NS_V2: &str = "CreateMarketV2";
pub const DARKBET_EXCHANGE_ZKAS_BUY_POSITION_NS_V2: &str = "BuyPositionV2";
pub const DARKBET_EXCHANGE_ZKAS_CLAIM_WINNINGS_NS_V2: &str = "ClaimWinningsV2";
pub const DARKBET_EXCHANGE_ZKAS_ADD_LIQUIDITY_NS_V2: &str = "AddLiquidityV2";

// ============================================================================
// COMPOSED CONTRACTS
// ============================================================================
//
// Darkbet Exchange composes these existing contracts:
// - DEX: Matching engine for back/lay orders
// - BettingStake: Liquidity pool for settlement
// - Oracle: Event resolution
// - DAO-Escrow: Commission treasury, governance
//
// Cross-contract call IDs (placeholder - would be actual contract IDs in production):
pub const DEX_CONTRACT_ID: &[u8] = b"dwow_dex";
pub const BETTING_STAKE_CONTRACT_ID: &[u8] = b"dwow_betting_stake";
pub const ORACLE_CONTRACT_ID: &[u8] = b"dwow_oracle";
pub const DAO_ESCROW_CONTRACT_ID: &[u8] = b"dwow_dao_escrow";