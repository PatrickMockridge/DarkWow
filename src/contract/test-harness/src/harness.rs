/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! DarkFi Contract Test Harness - Standard Interface
//!
//! This module provides a standardized testing interface for DarkFi contracts.
//!
//! ## Testing Layers
//!
//! | Layer | Purpose | Isolation |
//! |-------|---------|-----------|
//! | 1 | Circuit unit tests | Full |
//! | 2 | Contract isolation | Full |
//! | 3 | Composition | Partial |
//! | 4 | Integration | None |
//!
//! ## Usage
//!
//! ```rust
//! use darkfi_contract_test_harness::harness;
//!
//! // MoneyV3 (DeFi tokens)
//! let money_v3 = harness::money_v3::MoneyV3Harness::spawn();
//! let token = money_v3.create_token(auth_parent, user_data, blind, recipient, 1000)?;
//!
//! // NativeToken (Consensus)
//! let native_token = harness::native_token::NativeTokenHarness::spawn();
//! let reward = native_token.mint_pow_reward(keypair, block_height, fees)?;
//! ```

pub mod atomic_swap;
pub mod attestation;
pub mod auction;
pub mod bridge;
pub mod dex;
pub mod escrow;
pub mod identity;
pub mod labor_market;
pub mod money_v3;
pub mod native_token;
pub mod oracle;
pub mod stablecoin;
pub mod subscription;
pub mod tender;

// Re-export for convenience
pub use atomic_swap::AtomicSwapHarness;
pub use attestation::AttestationHarness;
pub use auction::AuctionHarness;
pub use bridge::BridgeHarness;
pub use dex::DexHarness;
pub use escrow::EscrowHarness;
pub use identity::IdentityHarness;
pub use labor_market::LaborMarketHarness;
pub use money_v3::{MoneyV3Harness, TokenCreationResult, MintResult};
pub use native_token::{NativeTokenHarness, PoWRewardResult, BurnResult, BurnCallInput};
pub use oracle::OracleHarness;
pub use stablecoin::StablecoinHarness;
pub use subscription::SubscriptionHarness;
pub use tender::TenderHarness;

use darkfi::{zk::ProvingKey, zkas::ZkBinary};

/// Trait for contract test harnesses providing ZK circuit access.
///
/// This trait enables the HeavyweightPipeline to work generically with
/// any contract harness by providing access to ZK binaries and proving keys.
pub trait ContractHarness {
    /// Returns the contract name (e.g., "dex", "money_v3", "native_token")
    fn name(&self) -> &str;

    /// Returns all circuit namespaces this contract uses
    fn circuits(&self) -> Vec<&'static str>;

    /// Get ZK binary for a circuit namespace
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary>;

    /// Get proving key for a circuit namespace
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey>;
}