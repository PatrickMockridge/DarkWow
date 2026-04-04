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

//! DarkFi Bridge Contract
//!
//! Smart contract implementing anonymous bridging between DarkFi and external
//! blockchains using Object Capability Security. Unlike VSS-based bridges,
//! this design uses deterministic address derivation - users control their
//! own funds via secrets, no threshold signing required.
//!
//! ## Architecture
//!
//! The bridge uses a modular, plugin-based architecture:
//!
//! - `chain_handler/`: Chain-specific handlers implementing `ChainHandler` trait
//! - `light_client/`: Light client verification implementing `LightClient` trait
//! - `capability/`: Object Capability derivation for authorization
//!
//! ## Security Model
//!
//! Object Capability model:
//! - Capabilities are derived, never assigned
//! - No VSS / threshold signing required
//! - User alone authorizes via secret knowledge
//! - Light client verification (no oracles)

/// Chain handler module - plugin architecture for external chains
pub mod chain_handler;
/// Light client module - trustless external chain verification
pub mod light_client;
/// Object Capability module - capability derivation and verification
pub mod capability;

use darkfi_sdk::define_contract_function;

/// Functions available in the contract
define_contract_function!(BridgeFunction {
    InitializeV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
    UpdateConfigV1 = 0x03,
    CancelWithdrawV1 = 0x04,  // Cancel timed-out withdrawal
    ExecuteGuaranteedWithdrawV1 = 0x05,  // Execute guaranteed withdrawal with pool stake
    // HTLC operations for cross-chain atomic swaps
    CreateHtlcV1 = 0x06,
    ClaimHtlcV1 = 0x07,
    RefundHtlcV1 = 0x08,
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

// These are the different sled trees that will be created
pub const BRIDGE_CONTRACT_INFO_TREE: &str = "info";
pub const BRIDGE_CONTRACT_DEPOSITS_TREE: &str = "deposits";
pub const BRIDGE_CONTRACT_WITHDRAWALS_TREE: &str = "withdrawals";
pub const BRIDGE_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const BRIDGE_CONTRACT_KEYS_TREE: &str = "keys";
pub const BRIDGE_CONTRACT_PENDING_WITHDRAWALS_TREE: &str = "pending_withdrawals";
// HTLC trees for cross-chain atomic swaps
pub const BRIDGE_CONTRACT_HTLCS_TREE: &str = "htlcs";
pub const BRIDGE_CONTRACT_HTLC_NULLIFIERS_TREE: &str = "htlc_nullifiers";

// These are keys inside the info tree
pub const BRIDGE_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const BRIDGE_CONTRACT_STATE: &[u8] = b"state";
pub const BRIDGE_CONTRACT_EXTERNAL_CHAIN: &[u8] = b"external_chain";

// zkas circuit namespaces
/// zkas deposit circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V1: &str = "Deposit_V1";
/// zkas withdrawal circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V1: &str = "Withdraw_V1";

// XMR (Monero) specific constants
/// Number of block confirmations required for XMR deposits
pub const BRIDGE_CONTRACT_XMR_CONFIRMATIONS: u64 = 10;
/// Hash function identifier for XMR (keccak256 = cn_fast_hash)
pub const BRIDGE_CONTRACT_XMR_HASH_FUNCTION: u8 = 3;

// ZEC (Zcash) specific constants
/// Number of block confirmations required for ZEC deposits (Sapling)
pub const BRIDGE_CONTRACT_ZEC_CONFIRMATIONS: u64 = 10;
/// Hash function identifier for ZEC (blake2s for Sapling)
pub const BRIDGE_CONTRACT_ZEC_HASH_FUNCTION: u8 = 4;

// AZT (Aztec) specific constants
/// Number of rollup confirmations required for Aztec deposits
pub const BRIDGE_CONTRACT_AZT_CONFIRMATIONS: u64 = 5;
/// Hash function identifier for Aztec (keccak256 for rollup)
pub const BRIDGE_CONTRACT_AZT_HASH_FUNCTION: u8 = 3;

/// Aztec asset IDs
pub const BRIDGE_CONTRACT_AZT_ASSET_ETH: u32 = 0;
pub const BRIDGE_CONTRACT_AZT_ASSET_DAI: u32 = 1;

// LTC (Litecoin) specific constants
/// Number of block confirmations required for LTC deposits
/// Litecoin faster block time (2.5 min vs Bitcoin 10 min) means faster confirmations
pub const BRIDGE_CONTRACT_LTC_CONFIRMATIONS: u64 = 6;
/// Hash function identifier for LTC (scrypt for PoW, same family as sha256)
pub const BRIDGE_CONTRACT_LTC_HASH_FUNCTION: u8 = 5;

/// Withdrawal timeout constants
/// Default number of blocks before a withdrawal can be cancelled
pub const BRIDGE_CONTRACT_WITHDRAWAL_TIMEOUT_BLOCKS: u64 = 100;
/// Slash amount for relayer timeout (in smallest unit)
pub const BRIDGE_CONTRACT_SLASH_AMOUNT: u64 = 1_000_000; // 0.001 XMR equivalent
