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

pub mod attestation;
pub mod atomic_swap;
pub mod auction;
pub mod baccarat;
pub mod betting_stake;
pub mod bridge;
pub mod darkbet_exchange;
pub mod darktoshi_dice;
pub mod dao_escrow;
pub mod deployooor;
pub mod dex;
pub mod drain_protection;
pub mod escrow;
pub mod game_room;
pub mod identity;
pub mod insurance_market;
pub mod labor_market;
pub mod lottery;
pub mod money_v3;
pub mod native_token;
pub mod oracle;
pub mod pool_stake;
pub mod relayer_endowment;
pub mod roulette;
pub mod slot;
pub mod stablecoin;
pub mod subscription;
pub mod tender;

// Re-export for convenience
pub use attestation::AttestationHarness;
pub use atomic_swap::AtomicSwapHarness;
pub use auction::AuctionHarness;
pub use baccarat::BaccaratHarness;
pub use betting_stake::{BettingStakeHarness, ClaimStakeInfo, UnstakeStakeInfo};
pub use bridge::BridgeHarness;
pub use dao_escrow::DaoEscrowHarness;
pub use darkbet_exchange::DarkbetExchangeHarness;
pub use darktoshi_dice::DarkToshiDiceHarness;
pub use deployooor::DeployooorHarness;
pub use dex::DexHarness;
pub use drain_protection::DrainProtectionHarness;
pub use escrow::{EscrowHarness, CreateEscrowResult, FundEscrowResult, ClaimEscrowResult, RefundEscrowResult};
pub use game_room::GameRoomHarness;
pub use identity::IdentityHarness;
pub use insurance_market::InsuranceMarketHarness;
pub use labor_market::LaborMarketHarness;
pub use lottery::LotteryHarness;
pub use money_v3::{MoneyV3Harness, TokenCreationResult, MintResult, TransferResult};
pub use native_token::{NativeTokenHarness, PoWRewardResult, BurnResult, BurnCallInput};
pub use oracle::OracleHarness;
pub use pool_stake::PoolStakeHarness;
pub use relayer_endowment::RelayerEndowmentHarness;
pub use roulette::RouletteHarness;
pub use slot::SlotHarness;
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