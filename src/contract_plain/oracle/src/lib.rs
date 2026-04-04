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

//! DarkFi Plain Oracle Contract
//!
//! # DEPRECATED
//!
//! This contract is deprecated. Use `darkfi_oracle_contract` in `../../contract/oracle/` instead.
//!
//! ZK opcodes `base_div` and `less_than_or_equal` are now sound and implemented.
//! See `proofs/lean/src/Main.lean` for Lean 4 verification.
//!
//! # Overview
//!
//! This is a **"partial transparency"** alternative to a hypothetical ZK oracle contract.
//! It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.
//!
//! # Key Differences from ZK Version (Hypothetical)
//!
//! | Feature | ZK Version | Plain Version | Privacy Impact |
//! |---------|-----------|---------------|----------------|
//! | Data aggregation | Limited | Full expression | All data visible |
//! | Weighted averages | Circuit-limited | Full support | Weights visible |
//! | Slash verification | Simple | Arbitrary logic | Slash data visible |
//!
//! # Privacy Notice
//!
//! Most state is PUBLIC in this contract. See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation.
//!
//! # Opcode Dependencies
//!
//! This contract uses native Rust division which would require `base_div` in ZK.

pub mod error;

pub mod model;

pub mod entrypoint;

use crate::error::OraclePlainError;

/// Function enum for oracle plain contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OraclePlainFunction {
    CreateFeedV1 = 0x00,
    RegisterStakerV1 = 0x01,
    SubmitDataPointV1 = 0x02,
    SlashStakerV1 = 0x03,
    UnregisterStakerV1 = 0x04,
}

impl TryFrom<u8> for OraclePlainFunction {
    type Error = OraclePlainError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::CreateFeedV1),
            0x01 => Ok(Self::RegisterStakerV1),
            0x02 => Ok(Self::SubmitDataPointV1),
            0x03 => Ok(Self::SlashStakerV1),
            0x04 => Ok(Self::UnregisterStakerV1),
            _ => Err(OraclePlainError::InvalidFunction),
        }
    }
}