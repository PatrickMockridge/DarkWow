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

//! DarkWow OTC Swap Contract
//!
//! Privacy-preserving peer-to-peer over-the-counter token swap contract.
//! Two parties atomically exchange tokens without a centralized exchange.
//!
//! Trust model: Two-phase commit with timeout
//! - Alice creates swap and locks her coins (Fund)
//! - Bob completes swap by locking his coins and releasing both (Execute)
//! - Alice can cancel before Bob commits, or after timeout
//!
//! Privacy properties:
//! - Amounts hidden in Pedersen commitments
//! - Parties hidden (public keys derived from secrets)
//! - Swap execution linkable only via nullifiers

use dwow_sdk::define_contract_function;

define_contract_function!(OtcSwapFunction {
    InitializeV1 = 0x00,
    CreateSwapV1 = 0x01,
    FundSwapV1 = 0x02,
    ExecuteSwapV1 = 0x03,
    CancelSwapV1 = 0x04,
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

/// Capability descriptor for wallet state machine
pub mod capability;

// These are the different sled trees that will be created
pub const OTC_SWAP_CONTRACT_INFO_TREE: &str = "info";
pub const OTC_SWAP_CONTRACT_SWAPS_TREE: &str = "swaps";
pub const OTC_SWAP_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";

// These are keys inside the info tree
pub const OTC_SWAP_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const OTC_SWAP_CONTRACT_STATE: &[u8] = b"state";
/// Promissory Note contract ID key (populated at runtime)
pub const OTC_SWAP_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID: &[u8] = b"promissory_note_cid";

// zkas circuit namespaces
pub const OTC_SWAP_CONTRACT_ZKAS_CREATE_NS_V1: &str = "CreateSwap";
pub const OTC_SWAP_CONTRACT_ZKAS_FUND_NS_V1: &str = "FundSwap";
pub const OTC_SWAP_CONTRACT_ZKAS_EXECUTE_NS_V1: &str = "ExecuteSwap";
pub const OTC_SWAP_CONTRACT_ZKAS_CANCEL_NS_V1: &str = "CancelSwap";
