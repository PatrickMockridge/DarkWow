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

use dwow_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::PrimeField, pallas};
use serde::{Deserialize, Serialize};

use super::{constants::DRK_POSEIDON_DOMAIN_NULLIFIER, poseidon_hash, SecretKey};
use crate::error::ContractError;

/// The `Nullifier` is represented as a base field element.
/// Used for double-spend prevention: `nf = poseidon_hash(secret, coin)`.
///
/// # Safety Invariants
/// - Zero is not a valid nullifier (Rule 3: "unclaimed reward" is `Option::None`)
/// - Canonical encoding only (non-canonical `from_bytes` returns error)
/// - `Ord`/`PartialOrd` for SMT key ordering and mempool `BTreeSet`
#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, SerialEncodable, SerialDecodable)]
pub struct Nullifier(pallas::Base);

// Manual serde impl using to_bytes/from_bytes (pallas::Base doesn't impl serde).
impl Serialize for Nullifier {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_bytes().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Nullifier {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 32]>::deserialize(d)?;
        Nullifier::from_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

impl Nullifier {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a new Nullifier from spending key and coin hash.
    /// `nf = poseidon_hash(DOMAIN_NULLIFIER, secret, coin)`.
    /// Domain-separated per type-system.md §8.1 — matches V2 circuit
    /// nullifier computation (witness_base(1) in burn_v2.zk et al.).
    pub fn new(secret: SecretKey, coin_hash: pallas::Base) -> Self {
        Self(poseidon_hash([DRK_POSEIDON_DOMAIN_NULLIFIER, *secret.inner(), coin_hash]))
    }

    /// Create a `Nullifier` from 32 bytes. Rejects non-canonical field elements
    /// AND zero (Rule 3: zero is not a valid nullifier).
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) if v != pallas::Base::zero() => Ok(Self(v)),
            Some(_) => Err(ContractError::IoError(
                "Nullifier is zero — not a valid nullifier (use Option<Nullifier> for unclaimed)"
                    .to_string(),
            )),
            None => Err(ContractError::IoError(
                "Non-canonical bytes for Nullifier".to_string(),
            )),
        }
    }

    /// Convert the `Nullifier` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    /// Returns `true` if this nullifier is zero (invalid).
    pub fn is_zero(&self) -> bool {
        self.0 == pallas::Base::zero()
    }

    /// Zero-value nullifier — only for pre-spend sentinel/placeholder.
    /// Per Rule 3: zero is not a valid nullifier for chain submission.
    /// Use `Option<Nullifier>` for unclaimed rewards; use `zero()` only
    /// where sentinel is required (e.g., subscription pre-spend state).
    pub const ZERO: Self = Self(pallas::Base::zero());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nullifier_rejects_zero() {
        assert!(Nullifier::from_bytes([0u8; 32]).is_err(),
            "Nullifier::from_bytes([0u8;32]) must reject zero per Rule 3");
    }

    #[test]
    fn test_nullifier_rejects_non_canonical() {
        assert!(Nullifier::from_bytes([0xFFu8; 32]).is_err(),
            "Nullifier::from_bytes(non-canonical) must reject");
    }

    #[test]
    fn test_nullifier_accepts_valid() {
        assert!(Nullifier::from_bytes([1u8; 32]).is_ok(),
            "Nullifier::from_bytes(valid) must succeed");
    }

    #[test]
    fn test_nullifier_roundtrip() {
        let nf = Nullifier::from_bytes([2u8; 32]).unwrap();
        let bytes = nf.to_bytes();
        let nf2 = Nullifier::from_bytes(bytes).unwrap();
        assert_eq!(nf, nf2, "to_bytes → from_bytes roundtrip must be identity");
    }

    #[test]
    fn test_nullifier_new_nonzero() {
        let sk = SecretKey::from_bytes([3u8; 32]).unwrap();
        let coin = pallas::Base::from(42u64);
        let nf = Nullifier::new(sk, coin);
        assert!(!nf.is_zero(), "Nullifier::new must produce non-zero value");
    }

    #[test]
    fn test_nullifier_zero_sentinel() {
        assert!(Nullifier::ZERO.is_zero(), "Nullifier::ZERO.is_zero() must be true");
        // But from_bytes still rejects
        assert!(Nullifier::from_bytes([0u8; 32]).is_err());
    }
}
