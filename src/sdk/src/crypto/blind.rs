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

use core::str::FromStr;

#[cfg(feature = "async")]
use dwow_serial::{AsyncDecodable, AsyncEncodable};
use dwow_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

use pasta_curves::{
    group::ff::{Field, PrimeField},
    pallas,
};
use rand_core::{CryptoRng, RngCore};

use crate::error::ContractError;

#[cfg(feature = "async")]
pub trait EncDecode: Encodable + Decodable + AsyncEncodable + AsyncDecodable {}
#[cfg(not(feature = "async"))]
pub trait EncDecode: Encodable + Decodable {}

impl EncDecode for pallas::Base {}
impl EncDecode for pallas::Scalar {}

/// Blinding factor used in bullas. Every bulla should contain one.
/// Copy and Clone removed per C3 — blinding factors SHALL NOT be
/// implicitly duplicated. Drop zeroizes the inner field element.
/// HAZOP C10 fix: Debug derive removed — manual impl renders `<redacted>`
/// to prevent leaking Pedersen commitment blinding factors via `{:?}`.
#[cfg(feature = "async")]
#[derive(Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Blind<F: Field + EncDecode + AsyncEncodable + AsyncDecodable>(pub F);

/// Blinding factor used in bullas. Every bulla should contain one.
/// Copy and Clone removed per C3 — blinding factors SHALL NOT be
/// implicitly duplicated. Drop zeroizes the inner field element.
/// HAZOP C10 fix: Debug derive removed — manual impl renders `<redacted>`.
#[cfg(not(feature = "async"))]
#[derive(Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Blind<F: Field + EncDecode>(pub F);

// Clone is explicit (not derived) — blinding factors SHALL NOT be
// implicitly duplicated. See C3 constraint (keypair.rs:71-76).
impl<F: Field + EncDecode> Clone for Blind<F> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}

// HAZOP C10 fix: manual Debug impl renders `<redacted>` — prevents
// accidental leakage of Pedersen commitment blinding factors via `{:?}`.
// Matches the SecretKey Debug pattern at keypair.rs:91-95.
impl<F: Field + EncDecode> core::fmt::Debug for Blind<F> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.write_str("<redacted>")
    }
}

// Zeroize the inner field element on drop — blinding factors are
// security-critical for Pedersen commitment hiding. Same pattern
// as SecretKey (keypair.rs:79-92).
impl<F: Field + EncDecode> Drop for Blind<F> {
    fn drop(&mut self) {
        unsafe {
            core::ptr::write_bytes(
                &mut self.0 as *mut F as *mut u8,
                0,
                core::mem::size_of::<F>(),
            );
        }
    }
}

impl<F: Field + EncDecode> Blind<F> {
    pub const ZERO: Self = Self(F::ZERO);

    pub fn random(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        Self(F::random(rng))
    }

    pub fn inner(&self) -> F {
        self.0
    }
}

impl<'a, F: Field + EncDecode> std::ops::Add<&'a Blind<F>> for &Blind<F> {
    type Output = Blind<F>;

    #[inline]
    fn add(self, rhs: &'a Blind<F>) -> Blind<F> {
        Blind(self.0.add(rhs.0))
    }
}

impl<F: Field + EncDecode> std::ops::AddAssign for Blind<F> {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0.add_assign(other.0)
    }
}

pub type BaseBlind = Blind<pallas::Base>;
pub type ScalarBlind = Blind<pallas::Scalar>;

impl BaseBlind {
    /// Named constructor — per type-system.md §8.5, `From<u64>` SHALL NOT
    /// be implemented for nominal cryptographic types. This constructor makes
    /// the domain transition explicit at every call site.
    pub fn from_u64(x: u64) -> Self {
        Self(pallas::Base::from(x))
    }
}

impl FromStr for BaseBlind {
    type Err = ContractError;

    /// Tries to create a `BaseBlind` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding BaseBlind from bytes, len is not 32".to_string(),
            ))
        }

        match pallas::Base::from_repr(decoded.try_into().unwrap()).into() {
            Some(k) => Ok(Self(k)),
            None => Err(ContractError::IoError("Could not convert bytes to BaseBlind".to_string())),
        }
    }
}

impl core::fmt::Display for BaseBlind {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{disp}")
    }
}

impl ScalarBlind {
    /// Named constructor — per type-system.md §8.5, `From<u64>` SHALL NOT
    /// be implemented for nominal cryptographic types. This constructor makes
    /// the domain transition explicit at every call site.
    pub fn from_u64(x: u64) -> Self {
        Self(pallas::Scalar::from(x))
    }
}

impl FromStr for ScalarBlind {
    type Err = ContractError;

    /// Tries to create a `ScalarBlind` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding ScalarBlind from bytes, len is not 32".to_string(),
            ))
        }

        match pallas::Scalar::from_repr(decoded.try_into().unwrap()).into() {
            Some(k) => Ok(Self(k)),
            None => {
                Err(ContractError::IoError("Could not convert bytes to ScalarBlind".to_string()))
            }
        }
    }
}

impl core::fmt::Display for ScalarBlind {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{disp}")
    }
}
