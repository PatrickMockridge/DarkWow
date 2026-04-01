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

//! DarkFi Plain Insurance Contract
//!
//! # Overview
//!
//! This is a **"partial transparency"** alternative to a hypothetical ZK insurance contract.
//! It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.
//!
//! # Key Differences from ZK Version (Hypothetical)
//!
//! | Feature | ZK Version | Plain Version | Privacy Impact |
//! |---------|-----------|---------------|----------------|
//! | Premium calculation | Limited | Full actuarial | Premium ratios visible |
//! | Coverage verification | Circuit-limited | Full expression | Verification visible |
//! | Claims processing | Simple | Arbitrary logic | Claims data visible |
//!
//! # Privacy Notice
//!
//! Most state is PUBLIC in this contract. Actual personal details are NOT stored on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation.
//!
//! # Opcode Dependencies
//!
//! This contract uses native Rust division which would require `base_div` in ZK.

pub mod error;

pub mod model;

pub mod entrypoint;

use crate::error::InsurancePlainError;

/// Function enum for insurance plain contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsurancePlainFunction {
    CreatePolicyV1 = 0x00,
    ActivatePolicyV1 = 0x01,
    FileClaimV1 = 0x02,
    ApproveClaimV1 = 0x03,
    RejectClaimV1 = 0x04,
    PayClaimV1 = 0x05,
    CancelPolicyV1 = 0x06,
}

impl TryFrom<u8> for InsurancePlainFunction {
    type Error = InsurancePlainError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::CreatePolicyV1),
            0x01 => Ok(Self::ActivatePolicyV1),
            0x02 => Ok(Self::FileClaimV1),
            0x03 => Ok(Self::ApproveClaimV1),
            0x04 => Ok(Self::RejectClaimV1),
            0x05 => Ok(Self::PayClaimV1),
            0x06 => Ok(Self::CancelPolicyV1),
            _ => Err(InsurancePlainError::InvalidFunction),
        }
    }
}