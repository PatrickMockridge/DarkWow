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

//! DarkWow DEX Contract - Level 0 MVP: Atomic Swap DAO
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

use dwow_sdk::define_contract_function;

// Functions available in the DEX contract
define_contract_function!(DexFunction {
    InitializeV1 = 0x00,
    CreateSwapV1 = 0x01,
    AcceptSwapV1 = 0x02,
    ExecuteSwapV1 = 0x03,
    CancelSwapV1 = 0x04,
    UpdateConfigV1 = 0x05,
    SetTransparencyLevelV1 = 0x06,
    ExecuteSwapFeeV1 = 0x07,
    ExecuteSwapSlippageV1 = 0x08,
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
/// Trusted money contract merkle root key
pub const DEX_CONTRACT_TRUSTED_MONEY_MERKLE_ROOT_KEY: &[u8] = b"trusted_money_merkle_root";
/// Transparency level key
pub const DEX_CONTRACT_TRANSPARENCY_LEVEL_KEY: &[u8] = b"transparency_level";
/// Governance public key for authorization
pub const DEX_CONTRACT_GOVERNANCE_PUBKEY_KEY: &[u8] = b"governance_pubkey";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// Create swap circuit namespace
pub const DEX_CONTRACT_ZKAS_CREATE_SWAP_NS_V1: &str = "CreateSwapV1";
/// Accept swap circuit namespace
pub const DEX_CONTRACT_ZKAS_ACCEPT_SWAP_NS_V1: &str = "AcceptSwapV1";
/// Execute swap circuit namespace
pub const DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1: &str = "ExecuteSwapV1";
/// Cancel swap circuit namespace
pub const DEX_CONTRACT_ZKAS_CANCEL_SWAP_NS_V1: &str = "CancelSwapV1";
/// Execute swap with slippage tolerance circuit namespace
pub const DEX_CONTRACT_ZKAS_EXECUTE_SWAP_SLIPPAGE_NS_V1: &str = "ExecuteSwapSlippageV1";
/// Execute swap with fee circuit namespace
pub const DEX_CONTRACT_ZKAS_EXECUTE_SWAP_FEE_NS_V1: &str = "ExecuteSwapFeeV1";