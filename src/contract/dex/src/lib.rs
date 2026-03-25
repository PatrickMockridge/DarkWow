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

//! DarkFi DEX Contract
//!
//! Anonymous decentralized exchange for privacy-preserving token swaps.
//! Built on DarkFi's ZK and object capability principles.
//!
//! ## Core Design
//!
//! Unlike traditional AMMs (UniSwap, Curve) that reveal everything,
//! the DarkFi DEX keeps orders, amounts, and identities hidden using:
//! - Sparse Merkle Tree for order book commitments
//! - Pedersen commitments for hidden values
//! - ZK proofs for order matching

use darkfi_sdk::error::ContractError;

/// Functions available in the DEX contract
#[repr(u8)]
#[derive(Debug)]
pub enum DexFunction {
    /// Initialize DEX state
    InitializeV1 = 0x00,
    /// Place an order (add to order book)
    PlaceOrderV1 = 0x01,
    /// Cancel an order
    CancelOrderV1 = 0x02,
    /// Match two orders (swap)
    MatchOrdersV1 = 0x03,
    /// Add liquidity to pool
    AddLiquidityV1 = 0x04,
    /// Remove liquidity from pool
    RemoveLiquidityV1 = 0x05,
    /// Update DEX configuration
    UpdateConfigV1 = 0x06,
}

impl TryFrom<u8> for DexFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::PlaceOrderV1),
            0x02 => Ok(Self::CancelOrderV1),
            0x03 => Ok(Self::MatchOrdersV1),
            0x04 => Ok(Self::AddLiquidityV1),
            0x05 => Ok(Self::RemoveLiquidityV1),
            0x06 => Ok(Self::UpdateConfigV1),
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

/// Tree for order book (SMT of order commitments)
pub const DEX_CONTRACT_ORDERBOOK_TREE: &str = "orderbook";
/// Tree for spent order nullifiers
pub const DEX_CONTRACT_NULLIFIERS_TREE: &str = "order_nullifiers";
/// Tree for liquidity positions
pub const DEX_CONTRACT_LIQUIDITY_TREE: &str = "liquidity";
/// Tree for DEX configuration
pub const DEX_CONTRACT_CONFIG_TREE: &str = "config";
/// Tree for DEX info
pub const DEX_CONTRACT_INFO_TREE: &str = "info";

// ============================================================================
// KEYS
// ============================================================================

/// Database version key
pub const DEX_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Current order book root
pub const DEX_CONTRACT_ORDERBOOK_ROOT: &[u8] = b"orderbook_root";
/// DEX fee parameter
pub const DEX_CONTRACT_FEE: &[u8] = b"dex_fee";
/// Minimum order size
pub const DEX_CONTRACT_MIN_ORDER: &[u8] = b"min_order";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// Place order circuit namespace
pub const DEX_CONTRACT_ZKAS_PLACE_ORDER_NS_V1: &str = "PlaceOrder_V1";
/// Cancel order circuit namespace
pub const DEX_CONTRACT_ZKAS_CANCEL_ORDER_NS_V1: &str = "CancelOrder_V1";
/// Match orders circuit namespace
pub const DEX_CONTRACT_ZKAS_MATCH_ORDERS_NS_V1: &str = "MatchOrders_V1";
/// Add liquidity circuit namespace
pub const DEX_CONTRACT_ZKAS_ADD_LIQUIDITY_NS_V1: &str = "AddLiquidity_V1";
/// Remove liquidity circuit namespace
pub const DEX_CONTRACT_ZKAS_REMOVE_LIQUIDITY_NS_V1: &str = "RemoveLiquidity_V1";