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

//! DarkFi DEX Contract - Level 0 MVP: Atomic Swap DAO
//!
//! This is the minimal viable DEX: a DAO that coordinates atomic swaps
//! between two parties without revealing amounts, identities, or trade data.
//!
//! ## How It Works
//!
//! 1. **Create Swap**: Proposer locks funds, specifies swap params
//! 2. **Accept Swap**: Acceptor locks matching funds
//! 3. **Execute**: Atomic swap executes (or timeout refund)
//!
//! ## Privacy
//!
//! - No order book (no one knows what swaps are possible)
//! - No price discovery (parties agree bilaterally)
//! - No information leakage (swap either happens or refunds)

use darkfi_sdk::error::ContractError;

/// Functions available in the DEX contract
#[repr(u8)]
#[derive(Debug)]
pub enum DexFunction {
    /// Initialize swap contract
    InitializeV1 = 0x00,
    /// Create atomic swap proposal
    CreateSwapV1 = 0x01,
    /// Accept and lock funds for swap
    AcceptSwapV1 = 0x02,
    /// Execute atomic swap
    ExecuteSwapV1 = 0x03,
    /// Cancel swap and refund
    CancelSwapV1 = 0x04,
    /// Update contract configuration
    UpdateConfigV1 = 0x05,
}

impl TryFrom<u8> for DexFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::CreateSwapV1),
            0x02 => Ok(Self::AcceptSwapV1),
            0x03 => Ok(Self::ExecuteSwapV1),
            0x04 => Ok(Self::CancelSwapV1),
            0x05 => Ok(Self::UpdateConfigV1),
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

/// Tree for active swaps
pub const DEX_CONTRACT_SWAPS_TREE: &str = "swaps";
/// Tree for swap participants (nullifiers)
pub const DEX_CONTRACT_PARTICIPANTS_TREE: &str = "participants";
/// Tree for configuration
pub const DEX_CONTRACT_CONFIG_TREE: &str = "config";
/// Tree for DEX info
pub const DEX_CONTRACT_INFO_TREE: &str = "info";

// ============================================================================
// KEYS
// ============================================================================

/// Database version key
pub const DEX_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Swap timeout (in blocks)
pub const DEX_CONTRACT_TIMEOUT: &[u8] = b"swap_timeout";
/// DEX fee parameter
pub const DEX_CONTRACT_FEE: &[u8] = b"dex_fee";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// Create swap circuit namespace
pub const DEX_CONTRACT_ZKAS_CREATE_SWAP_NS_V1: &str = "CreateSwap_V1";
/// Accept swap circuit namespace
pub const DEX_CONTRACT_ZKAS_ACCEPT_SWAP_NS_V1: &str = "AcceptSwap_V1";
/// Execute swap circuit namespace
pub const DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1: &str = "ExecuteSwap_V1";
/// Cancel swap circuit namespace
pub const DEX_CONTRACT_ZKAS_CANCEL_SWAP_NS_V1: &str = "CancelSwap_V1";