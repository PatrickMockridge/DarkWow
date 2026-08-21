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

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]


//! DarkWow Subscription Contract
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

use dwow_sdk::define_contract_function;

define_contract_function!(SubscriptionFunction {
    InitializeV1 = 0x00,
    SubscribeV1 = 0x01,
    CancelV1 = 0x02,
    RenewV1 = 0x03,
    VerifyAccessV1 = 0x04,
    DaoControlV1 = 0x05,
    UpdateUsageV1 = 0x06,
});

/// Call parameters definitions
pub mod model;

/// WASM entrypoint functions
#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

/// Capability descriptor for wallet resolver
pub mod capability;

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
/// Promissory Note contract ID key (populated at runtime)
pub const SUBSCRIPTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID: &[u8] = b"promissory_note_cid";
pub const SUBSCRIPTION_CONTRACT_PURSE_CONTRACT_ID: &[u8] = b"purse_cid";
pub const SUBSCRIPTION_CONTRACT_BOX_CONTRACT_ID: &[u8] = b"box_cid";

// ============================================================================
// zkas CIRCUIT NAMESPACES
// ============================================================================

/// Subscribe circuit namespace
pub const SUBSCRIPTION_CONTRACT_ZKAS_SUBSCRIBE_NS_V1: &str = "Subscribe";
/// Verify access circuit namespace
pub const SUBSCRIPTION_CONTRACT_ZKAS_VERIFY_NS_V1: &str = "VerifyAccess";
/// Update usage circuit namespace
pub const SUBSCRIPTION_CONTRACT_ZKAS_UPDATE_NS_V1: &str = "UpdateUsage";
pub const SUBSCRIPTION_CONTRACT_ZKAS_SUBSCRIBE_NS_V2: &str = "SubscribeV2";
pub const SUBSCRIPTION_CONTRACT_ZKAS_RENEW_NS_V2: &str = "RenewV2";
pub const SUBSCRIPTION_CONTRACT_ZKAS_CANCEL_NS_V2: &str = "CancelV2";
pub const SUBSCRIPTION_CONTRACT_ZKAS_VERIFY_ACCESS_NS_V2: &str = "VerifyAccessV2";
pub const SUBSCRIPTION_CONTRACT_ZKAS_VERIFY_NS_V2: &str = "VerifyAccessV2";
pub const SUBSCRIPTION_CONTRACT_ZKAS_UPDATE_NS_V2: &str = "UpdateUsageV2";

/// Thread-safe flag for deterministic ZK proof generation.
/// Set by tests before endpoint exercise to eliminate OsRng from collateral/debt
/// blinds, note encryption, and proof generation, so a chain-replay determinism
/// check (PI-7) produces identical bytes on both chains.
/// Must be set BEFORE any ZK proof is created.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation for testing.
/// Replaces OsRng with StdRng::seed_from_u64(0).
#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled. Always `false` unless the
/// `deterministic-zk` feature is enabled (test builds only — heavyweight-spec.md §7.4 DZ-4).
pub fn deterministic_zk_enabled() -> bool {
    #[cfg(feature = "deterministic-zk")]
    {
        DETERMINISTIC_ZK.load(Ordering::SeqCst)
    }
    #[cfg(not(feature = "deterministic-zk"))]
    {
        false
    }
}