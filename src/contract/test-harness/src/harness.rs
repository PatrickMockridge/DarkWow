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

pub mod money_v3;
pub mod native_token;

// Re-export for convenience
pub use money_v3::{MoneyV3Harness, TokenCreationResult, MintResult};
pub use native_token::{NativeTokenHarness, PoWRewardResult, BurnResult, BurnCallInput};