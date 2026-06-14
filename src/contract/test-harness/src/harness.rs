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

//! DarkWow Contract Test Harness - Standard Interface
//!
//! This module provides a standardized testing interface for DarkWow contracts.
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
//! use dwow_contract_test_harness::harness;
//!
//! // PromissoryNote (DeFi tokens)
//! let promissory_note = harness::promissory_note::PromissoryNoteHarness::spawn();
//! let token = promissory_note.create_token(auth_parent, user_data, blind, recipient, 1000)?;
//!
//! // NativeToken (Consensus)
//! let native_token = harness::native_token::NativeTokenHarness::spawn();
//! let reward = native_token.mint_pow_reward(keypair, block_height, fees)?;
//! ```

pub mod attestation;
pub mod auction;
pub mod baccarat;
pub mod bearer_bond;
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
pub mod promissory_note;
pub mod native_token;
pub mod oracle;
pub mod otc_swap;
pub mod pool_stake;
pub mod relayer_endowment;
pub mod roulette;
pub mod slot;
pub mod stablecoin;
pub mod subscription;
pub mod tender;

// Re-export for convenience
pub use attestation::AttestationHarness;
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
pub use promissory_note::{PromissoryNoteHarness, TokenCreationResult, MintResult, TransferResult};
pub use native_token::{NativeTokenHarness, PoWRewardResult, BurnResult, BurnCallInput};
pub use oracle::OracleHarness;
pub use pool_stake::PoolStakeHarness;
pub use relayer_endowment::RelayerEndowmentHarness;
pub use roulette::RouletteHarness;
pub use slot::SlotHarness;
pub use stablecoin::StablecoinHarness;
pub use subscription::SubscriptionHarness;
pub use tender::TenderHarness;

use dwow_core::{zk::ProvingKey, zkas::ZkBinary, Result};

/// Trait for contract test harnesses providing ZK circuit access.
///
/// This trait enables the HeavyweightPipeline to work generically with
/// any contract harness by providing access to ZK binaries and proving keys.
pub trait ContractHarness {
    /// Returns the contract name (e.g., "dex", "promissory_note", "native_token")
    fn name(&self) -> &str;

    /// Returns all circuit namespaces this contract uses
    fn circuits(&self) -> Vec<&'static str>;

    /// Get ZK binary for a circuit namespace
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary>;

    /// Get proving key for a circuit namespace
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey>;

    /// Verify ZK coverage: every circuit in `circuits()` has a valid ZK binary
    /// and proving key. Called as a pre-deploy gate by `HeavyweightPipeline`.
    ///
    /// Returns an error listing ALL missing circuits rather than failing on the
    /// first one — this gives developers a complete picture of what's wrong.
    fn verify_zk_coverage(&self) -> Result<()> {
        let mut missing_zkbin = Vec::new();
        let mut missing_pk = Vec::new();
        let mut total = 0u32;
        let mut covered = 0u32;

        for ns in self.circuits() {
            total += 1;
            let has_zkbin = self.get_zkbin(ns).is_some();
            let has_pk = self.get_pk(ns).is_some();

            if !has_zkbin {
                missing_zkbin.push(ns);
            }
            if !has_pk {
                missing_pk.push(ns);
            }
            if has_zkbin && has_pk {
                covered += 1;
            }
        }

        if missing_zkbin.is_empty() && missing_pk.is_empty() {
            return Ok(());
        }

        let mut msg = format!(
            "ZK coverage check FAILED for {}: {}/{} circuits covered. ",
            self.name(),
            covered,
            total
        );

        if !missing_zkbin.is_empty() {
            msg.push_str(&format!(
                "Missing ZkBinary for: [{}]. ",
                missing_zkbin.join(", ")
            ));
        }
        if !missing_pk.is_empty() {
            msg.push_str(&format!(
                "Missing ProvingKey for: [{}]. ",
                missing_pk.join(", ")
            ));
        }
        msg.push_str("Every circuit in circuits() must have a valid ZkBinary and ProvingKey.");

        Err(dwow_core::Error::Custom(msg))
    }
}