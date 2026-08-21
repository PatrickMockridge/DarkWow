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


//! DarkWow Bridge Contract (bridge-core)
//!
//! Smart contract implementing anonymous bridging between DarkWow and external
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
/// Cross-chain cryptographic verification (gated behind bridge-verify feature)
#[cfg(feature = "bridge-verify")]
pub mod verify;

use dwow_sdk::define_contract_function;

// Functions available in the contract (bridge-core: bridging + lock + PN issuance)
define_contract_function!(BridgeFunction {
    InitializeV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
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

// These are keys inside the info tree
pub const BRIDGE_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Promissory Note contract ID for cross-contract routing validation
pub const PROMISSORY_NOTE_CONTRACT_ID_KEY: &[u8] = b"promissory_note_cid";
pub const BRIDGE_CONTRACT_STATE: &[u8] = b"state";

// zkas circuit namespaces
/// zkas deposit circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V1: &str = "DepositV1";
/// zkas withdrawal circuit namespace
pub const BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V1: &str = "WithdrawV1";

// V2 circuit namespaces (HAZOP RC3: domain separation)
/// zkas deposit circuit namespace V2 (domain-separated)
pub const BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V2: &str = "DepositV2";
/// zkas withdrawal circuit namespace V2 (domain-separated)
pub const BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V2: &str = "WithdrawV2";

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
pub const BRIDGE_CONTRACT_LTC_CONFIRMATIONS: u64 = 6;
/// Hash function identifier for LTC (scrypt for PoW, same family as sha256)
pub const BRIDGE_CONTRACT_LTC_HASH_FUNCTION: u8 = 5;

/// Thread-safe flag for deterministic ZK proof generation.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation for testing.
#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled.
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
