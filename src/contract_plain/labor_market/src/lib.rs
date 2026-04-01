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

//! DarkFi Plain Labor Market Contract
//!
//! # Overview
//!
//! This is a **"partial transparency"** alternative to the ZK `labor_market` contract.
//! It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.
//!
//! # Key Differences from ZK Version
//!
//! | Feature | ZK Version | Plain Version | Privacy Impact |
//! |---------|-----------|---------------|----------------|
//! | Payment tracking | Hidden in commitments | Public on-chain | All amounts visible |
//! | Time-weighted release | Not available | Native division | Timing ratios visible |
//! | Milestone chains | Limited | Full support | All milestones public |
//!
//! # Privacy Notice
//!
//! Most state is PUBLIC in this contract. Actual work content is NOT stored on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation.
//!
//! # Opcode Dependencies
//!
//! This contract uses native Rust division which would require `base_div` in ZK.

pub mod error;

pub mod model;

pub mod entrypoint;

use crate::error::LaborMarketPlainError;

/// Function enum for labor market plain contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaborMarketPlainFunction {
    CreateJobV1 = 0x00,
    AcceptJobV1 = 0x01,
    SubmitDeliverableV1 = 0x02,
    ConfirmDeliverableV1 = 0x03,
    DisputeV1 = 0x04,
    CancelV1 = 0x05,
    RefundV1 = 0x06,
}

impl TryFrom<u8> for LaborMarketPlainFunction {
    type Error = LaborMarketPlainError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::CreateJobV1),
            0x01 => Ok(Self::AcceptJobV1),
            0x02 => Ok(Self::SubmitDeliverableV1),
            0x03 => Ok(Self::ConfirmDeliverableV1),
            0x04 => Ok(Self::DisputeV1),
            0x05 => Ok(Self::CancelV1),
            0x06 => Ok(Self::RefundV1),
            _ => Err(LaborMarketPlainError::InvalidFunction),
        }
    }
}