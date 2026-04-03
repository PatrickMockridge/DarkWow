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

use darkfi_sdk::define_contract_function;

/// Functions available in the contract
define_contract_function!(BridgeFunction {
    InitializeV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
    UpdateConfigV1 = 0x03,
    CancelWithdrawV1 = 0x04,  // Cancel timed-out withdrawal
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

/// Withdrawal timeout constants
/// Default number of blocks before a withdrawal can be cancelled
pub const BRIDGE_CONTRACT_WITHDRAWAL_TIMEOUT_BLOCKS: u64 = 100;
/// Slash amount for relayer timeout (in smallest unit)
pub const BRIDGE_CONTRACT_SLASH_AMOUNT: u64 = 1_000_000; // 0.001 XMR equivalent
