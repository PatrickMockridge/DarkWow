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
//! ## Three Operating Modes
//!
//! DAO-Escrow supports three configuration modes:
//!
//! ### MODE_ESCROW (0x00) - Escrow-Only
//! - Pure insurance pool
//! - Members pay premiums → endowment grows
//! - No treasury (operational funds)
//! - Endowment pays out claims
//!
//! ### MODE_TREASURY (0x01) - Treasury-Only
//! - Same as DarkFi DAO
//! - Members pay fees → treasury grows
//! - DAO votes on treasury spending
//! - No endowment/insurance
//!
//! ### MODE_TREASURY_ENDOWMENT (0x02) - Treasury + Endowment
//! - Full-featured with insurance backing
//! - Fee split: treasury_share → Treasury, endowment_share → Endowment
//! - Treasury for operational costs, Endowment for insurance
//!
//! ## Trust Model
//!
//! - Membership notes are time-locked (block-based expiry)
//! - Claims against endowment handled by DAO vote
//! - Built-in governance (propose/vote/exec)
//! - No external DAO dependency
//!
//! ## Use Cases
//!
//! - **Community Insurance**: Escrow mode - pure insurance pool
//! - **Protocol Treasury**: Treasury mode - same as DarkFi DAO
//! - **Full-Featured DAO**: TreasuryEndowment mode - treasury + insurance

use darkfi_sdk::define_contract_function;

/// DAO-Escrow operating modes
pub mod modes {
    use super::DaoEscrowMode;
    /// Escrow-only: Pure insurance pool
    pub const MODE_ESCROW: u8 = 0x00;
    /// Treasury-only: Same as DarkFi DAO
    pub const MODE_TREASURY: u8 = 0x01;
    /// Treasury + Endowment: Full-featured
    pub const MODE_TREASURY_ENDOWMENT: u8 = 0x02;
}

/// Functions available in the contract
define_contract_function!(DaoEscrowFunction {
    InitializeV1 = 0x00,
    UpdateV1 = 0x01,
    PayPremiumV1 = 0x02,
    WithdrawV1 = 0x03,  // Withdrawal from treasury (not endowment)
    /// Endowment-only withdrawal (requires DAO vote, for insurance payouts)
    EndowmentWithdrawV1 = 0x04,
    /// Treasury spending (standard DAO governance)
    TreasurySpendV1 = 0x05,
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
