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

//! DarkFi Subscription Contract
//!
//! Privacy-preserving member subscription service with:
//! - Block-based time locks (no oracle needed)
//! - DAO treasury for subscription fees
//! - Endowment fund for insurance/refunds
//! - Cross-chain atomic swap integration
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │              Subscription Service DAO                   │
//! │  ┌─────────────────────────────────────────────────┐   │
//! │  │  Treasury (subscription fees → governance)      │   │
//! │  └─────────────────────────────────────────────────┘   │
//! │  ┌─────────────────────────────────────────────────┐   │
//! │  │  Endowment Fund (insurance reserve)             │   │
//! │  │  - Covers refunds if service fails              │   │
//! │  │  - DAO-controlled drawdown                     │   │
//! │  └─────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## State Machine
//!
//! ```text
//! Active ──[Cancel]──> Cancelled ──[Expiry]──> Expired
//!    │                                          │
//!    └──[Renew]──> Active                       │
//!                                                     │
//! Cancelled: user cancelled, refund available         │
//! Expired: time lock expired, refund available        │
//! ```
//!
//! ## Trust Model
//!
//! - **Block-based locks**: Subscriptions expire at specific block heights
//! - **DAO governance**: Subscription terms, pricing, and endowment managed by DAO
//! - **Endowment insurance**: DAO can authorize refunds from endowment fund
//! - **Atomic swap**: Cross-chain payments via HTLC pattern

use darkfi_sdk::define_contract_function;

define_contract_function!(SubscriptionFunction {
    InitializeV1 = 0x00,
    SubscribeV1 = 0x01,
    CancelV1 = 0x02,
    RenewV1 = 0x03,
    VerifyAccessV1 = 0x04,
    DaoControlV1 = 0x05,
});

/// Call parameters definitions
pub mod model;

/// WASM entrypoint functions
#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Info tree: version, config, plan registry
pub const SUBSCRIPTION_CONTRACT_INFO_TREE: &str = "info";
/// Subscriptions tree: active/cancelled subscriptions
pub const SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE: &str = "subscriptions";
/// Nullifiers tree: prevents double-spend/cancel
pub const SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Plans tree: subscription plan definitions
pub const SUBSCRIPTION_CONTRACT_PLANS_TREE: &str = "plans";

// ============================================================================
// DATABASE KEYS
// ============================================================================

pub const SUBSCRIPTION_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const SUBSCRIPTION_CONTRACT_STATE: &[u8] = b"state";

// ============================================================================
// zkas CIRCUIT NAMESPACES
// ============================================================================

/// Subscribe circuit namespace
pub const SUBSCRIPTION_CONTRACT_ZKAS_SUBSCRIBE_NS_V1: &str = "Subscribe_V1";
/// Verify access circuit namespace
pub const SUBSCRIPTION_CONTRACT_ZKAS_VERIFY_NS_V1: &str = "VerifyAccess_V1";