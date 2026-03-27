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

//! DarkFi DAO-Escrow Contract
//!
//! Simplified MVP: Endowment pool governed by a DAO.
//!
//! ## Concept
//!
//! The DAO-Escrow is an endowment pool where:
//! - **Members pay premiums** → receive a time-limited membership note (annual)
//! - **Claims are handled by the DAO** → via existing treasury management (propose/vote/exec)
//! - **The DAO acts as escrow oracle** → votes to release funds when claims are approved
//!
//! This simplifies the original design by delegating voting to the existing DAO,
//! rather than building a parallel voting mechanism.
//!
//! ## Trust Model: DAO-Controlled Endowment
//!
//! - Membership notes are time-locked (annual expiry)
//! - Claims against the endowment are handled by DAO treasury vote
//! - The DAO's existing governance (propose/vote/exec) controls fund release
//! - No separate DAO-Escrow voting needed
//!
//! ## Use Cases
//!
//! - **Community Insurance**: Pool funds, DAO votes on claim payouts
//! - **Protocol-Owned Liquidity**: Endowment controlled by DAO
//! - **Treasury Management**: DAO manages a dedicated endowment pool

use darkfi_sdk::define_contract_function;

/// Functions available in the contract
define_contract_function!(DaoEscrowFunction {
    InitializeV1 = 0x00,
    UpdateV1 = 0x01,
    PayPremiumV1 = 0x02,
    WithdrawV1 = 0x03,  // Admin withdrawal of endowment fees
});

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

/// Info tree (version, config)
pub const DAO_ESCROW_CONTRACT_INFO_TREE: &str = "info";
/// Bullas tree (endowment instances)
pub const DAO_ESCROW_CONTRACT_BULLAS_TREE: &str = "bullas";
/// Membership notes tree (time-limited membership)
pub const DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE: &str = "membership";
/// Endowment pool tree (actual funds)
pub const DAO_ESCROW_CONTRACT_ENDOWMENT_TREE: &str = "endowment";

// ============================================================================
// KEYS
// ============================================================================

/// DB version key
pub const DAO_ESCROW_DB_VERSION: &[u8] = b"db_version";
/// Merkle tree key
pub const DAO_ESCROW_MERKLE_TREE: &[u8] = b"merkle_tree";
/// Latest root key
pub const DAO_ESCROW_LATEST_ROOT: &[u8] = b"last_root";

// ============================================================================
// ZKAS CIRCUIT NAMESPACES
// ============================================================================

/// ZKAS namespace for initialization
pub const DAO_ESCROW_ZKAS_INIT_NS: &str = "Init";
/// ZKAS namespace for premium payment
pub const DAO_ESCROW_ZKAS_PREMIUM_NS: &str = "PayPremium";
