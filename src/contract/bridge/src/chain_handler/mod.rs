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

//! Chain Handler Module
//!
//! Provides the `ChainHandler` trait and `ChainRegistry` for
//! chain-specific bridge operations.
//!
//! ## Architecture
//!
//! The bridge uses a plugin architecture where each external chain
//! implements the `ChainHandler` trait. The core contract routes
//! to the appropriate handler based on `ChainId`.
//!
//! ## Adding a New Chain
//!
//! 1. Implement `ChainHandler` for your chain
//! 2. Add variant to `ChainData` enum in `handler.rs`
//! 3. Register handler in `ChainRegistry::new()`
//! 4. NO changes to bridge core contract needed

pub mod registry;
pub mod handler;

pub use registry::ChainRegistry;
pub use handler::{
    ChainData, ChainHandler, ChainId, ExternalDeposit, HtlcDeposit, HtlcState, HtlcSwap,
    TxHash, VerifiedDeposit, VerifiedWithdrawal, WithdrawalRequest,
};
