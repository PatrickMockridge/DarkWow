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
//! blockchains using verifiable secret sharing and threshold cryptography.

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum BridgeFunction {
    /// Initialize bridge state
    InitializeV1 = 0x00,
    /// Register a new bridge deposit
    DepositV1 = 0x01,
    /// Claim a bridged withdrawal
    WithdrawV1 = 0x02,
    /// Update bridge configuration
    UpdateConfigV1 = 0x03,
}

impl TryFrom<u8> for BridgeFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::DepositV1),
            0x02 => Ok(Self::WithdrawV1),
            0x03 => Ok(Self::UpdateConfigV1),
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

// These are the different sled trees that will be created
pub const BRIDGE_CONTRACT_INFO_TREE: &str = "info";
pub const BRIDGE_CONTRACT_DEPOSITS_TREE: &str = "deposits";
pub const BRIDGE_CONTRACT_WITHDRAWALS_TREE: &str = "withdrawals";
pub const BRIDGE_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const BRIDGE_CONTRACT_KEYS_TREE: &str = "keys";

// These are keys inside the info tree
pub const BRIDGE_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const BRIDGE_CONTRACT_STATE: &[u8] = b"state";
pub const BRIDGE_CONTRACT_EXTERNAL_CHAIN: &[u8] = b"external_chain";

// zkas circuit namespaces
/// zkas deposit circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V1: &str = "Deposit_V1";
/// zkas withdrawal circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V1: &str = "Withdraw_V1";
