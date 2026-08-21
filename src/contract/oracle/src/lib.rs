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


//! DarkWow Oracle Contract
//!
//! Demonstrates the "push model" for oracles in DarkWow.
//!
//! ## Overview
//!
//! Oracles push data values (prices, scores, weather, etc.) that can be
//! attested for consumption by other contracts. This creates a trustless
//! bridge between external data and DarkWow contracts.
//!
//! ## Flow
//!
//! 1. Oracle operator registers a data feed
//! 2. Oracle pushes values to their feed
//! 3. Oracle creates attestations for specific values
//! 4. Other contracts consume attestations via attestation contract
//!
//! ## Use Cases
//!
//! - **Price feeds**: Oracle attests to token prices, DeFi contracts use for liquidation
//! - **Sports scores**: Oracle attests to game outcomes, prediction markets settle
//! - **Weather data**: Oracle attests to conditions, insurance contracts pay out
//! - **Randomness**: Oracle attests to random values, NFT/s game uses for fairness

use dwow_sdk::define_contract_function;

/// Deterministic ZK mode — TEST-ONLY.
///
/// `enable_deterministic_zk()` switches proof generation from `OsRng` to a
/// seeded RNG (`StdRng::seed_from_u64(0)`), making proofs byte-identical so the
/// PI-7 determinism check (heavyweight-spec.md §7.4) can compare block hashes
/// across two independent pipelines.
///
/// SECURITY: this DISABLES zero-knowledge — the blinding factors become
/// deterministic and publicly known, so an observer who knows the seed can
/// unblind the commitments and recover the witness (secret key, private value).
/// It MUST therefore never be reachable in production. It is gated behind the
/// `deterministic-zk` cargo feature, which ONLY the test harness enables. The
/// wallet and WASM builds never enable that feature, so `enable_deterministic_zk`
/// is compiled out and `deterministic_zk_enabled()` always returns `false`.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation (test-harness only).
#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled. Always `false` unless the
/// `deterministic-zk` feature is enabled (test builds only).
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

define_contract_function!(OracleFunction {
    RegisterOracleV1 = 0x00,
    PushValueV1 = 0x01,
    AttestValueV1 = 0x02,
    PushValueCommitmentV1 = 0x03,
    AggregateV1 = 0x04,
    SetOracleActiveV1 = 0x05,
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

// Database tree names
pub const ORACLE_CONTRACT_ORACLES_TREE: &str = "oracles";
pub const ORACLE_CONTRACT_ATTESTATIONS_TREE: &str = "attestations";
pub const ORACLE_CONTRACT_INFO_TREE: &str = "info";

// zkas circuit namespaces (V2 only)
pub const ORACLE_CONTRACT_ZKAS_REGISTER_ORACLE_NS_V2: &str = "RegisterOracleV2";
pub const ORACLE_CONTRACT_ZKAS_PUSH_VALUE_NS_V2: &str = "PushValueV2";
pub const ORACLE_CONTRACT_ZKAS_ATTEST_VALUE_NS_V2: &str = "AttestValueV2";
pub const ORACLE_CONTRACT_ZKAS_PUSH_VALUE_COMMITMENT_NS_V2: &str = "PushValueCommitmentV2";
pub const ORACLE_CONTRACT_ZKAS_AGGREGATE_NS_V2: &str = "AggregateV2";
