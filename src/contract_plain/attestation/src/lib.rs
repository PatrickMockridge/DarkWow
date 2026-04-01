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

//! DarkFi Plain Attestation Contract
//!
//! # Overview
//!
//! This is a **"partial transparency"** alternative to the ZK `attestation` contract.
//! It prioritizes **expressivity over privacy** to overcome current ZK circuit limitations.
//!
//! # Key Differences from ZK Version
//!
//! | Feature | ZK Version | Plain Version | Privacy Impact |
//! |---------|-----------|---------------|----------------|
//! | Credential chains | Limited | Full delegation support | Chain data visible |
//! | Delegation ratios | Circuit-limited | Full expression | Ratios visible |
//! | Cross-references | Simple | Full graph support | Reference chains visible |
//! | Expiry verification | Basic | Time-bounded with ratios | Expiry visible |
//!
//! # Privacy Notice
//!
//! Most state is PUBLIC in this contract. Actual credential content is NOT stored on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full documentation.
//!
//! # Opcode Dependencies
//!
//! This contract uses native Rust division which would require `base_div` in ZK.

pub mod error;

pub mod model;

pub mod entrypoint;

use crate::error::AttestationPlainError;

/// Function enum for attestation plain contract
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttestationPlainFunction {
    RegisterAttestorV1 = 0x00,
    CreateAttestationV1 = 0x01,
    DelegateAttestationV1 = 0x02,
    RevokeAttestationV1 = 0x03,
    VerifyAttestationV1 = 0x04,
}

impl TryFrom<u8> for AttestationPlainFunction {
    type Error = AttestationPlainError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::RegisterAttestorV1),
            0x01 => Ok(Self::CreateAttestationV1),
            0x02 => Ok(Self::DelegateAttestationV1),
            0x03 => Ok(Self::RevokeAttestationV1),
            0x04 => Ok(Self::VerifyAttestationV1),
            _ => Err(AttestationPlainError::InvalidFunction),
        }
    }
}