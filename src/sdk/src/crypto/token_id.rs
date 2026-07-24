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

/// TokenId: asset identifier (↓denominate). Per spec §8.1.
///
/// Identifies which token/asset a coin represents. The native DRKW token has
/// `TokenId::DRKW` = zero. Other tokens (bridged assets, stablecoins, etc.)
/// have non-zero token IDs derived from their contract IDs.
use dwow_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::PrimeField, pallas};
use serde::{Deserialize, Serialize};

use crate::error::ContractError;

#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct TokenId(pallas::Base);

impl TokenId {
    /// Native DRKW token identifier = zero.
    pub const DRKW: Self = Self(pallas::Base::zero());

    /// Construct a TokenId from a pallas::Base field element.
    /// Named constructor per §8.5 — no From<pallas::Base> impl.
    pub fn from_base(x: pallas::Base) -> Self {
        Self(x)
    }

    /// Reference the raw inner base field element.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Convert to 32 raw bytes (canonical little-endian encoding).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    /// Create a `TokenId` from 32 bytes. Rejects non-canonical field elements.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError(
                "Non-canonical bytes for TokenId".to_string(),
            )),
        }
    }
}

// Manual serde impl using to_bytes/from_bytes (pallas::Base doesn't impl serde).
impl Serialize for TokenId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_bytes().serialize(s)
    }
}

impl<'de> Deserialize<'de> for TokenId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 32]>::deserialize(d)?;
        TokenId::from_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_id_roundtrip() {
        let tid = TokenId::from_bytes([1u8; 32]).unwrap();
        let bytes = tid.to_bytes();
        let tid2 = TokenId::from_bytes(bytes).unwrap();
        assert_eq!(tid, tid2);
    }

    #[test]
    fn test_token_id_zero_valid() {
        // TokenId::DRKW is zero — must be valid (unlike Nullifier)
        assert!(TokenId::from_bytes([0u8; 32]).is_ok());
    }
}
