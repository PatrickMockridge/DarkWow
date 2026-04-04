/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! DarkFi Plain Subscription Contract
//!
//! # DEPRECATED
//!
//! This contract is deprecated. Use `darkfi_subscription_contract` in `../../contract/subscription/` instead.
//!
//! ZK opcodes `base_div` and `less_than_or_equal` are now sound and implemented.
//! See `proofs/lean/src/Main.lean` for Lean 4 verification.
//!
//! # Overview
//!
//! This is a **"partial transparency"** alternative to the ZK `subscription` contract.
//! It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.
//!
//! # Key Differences from ZK Version
//!
//! | Feature | ZK Version | Plain Version |
//! |---------|-----------|---------------|
//! | Access control | Tiered linear | True bitmask |
//! | Rate limiting | Simple counter | Ratio-based |
//!
//! # Privacy Notice
//!
//! All state is PUBLIC in this contract. See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md)
//! for full documentation of privacy tradeoffs.
//!
//! # Opcode Dependencies
//!
//! This contract uses native Rust division which would require `base_div` in ZK.
//! Currently uses `checked_div` with error handling.

pub mod error;

pub mod model;

pub mod entrypoint;

use crate::error::SubscriptionPlainError;

/// Function enum for subscription plain contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionPlainFunction {
    SubscribeV1 = 0x00,
    VerifyAccessV1 = 0x01,
    CancelV1 = 0x02,
}

impl TryFrom<u8> for SubscriptionPlainFunction {
    type Error = crate::error::SubscriptionPlainError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::SubscribeV1),
            0x01 => Ok(Self::VerifyAccessV1),
            0x02 => Ok(Self::CancelV1),
            _ => Err(SubscriptionPlainError::InvalidFunction),
        }
    }
}
