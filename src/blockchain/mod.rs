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

use std::{fmt, str::FromStr};

use sled::IVec;

use dwow_serial::{deserialize, Decodable};

#[cfg(feature = "async-serial")]
use dwow_serial::{deserialize_async, AsyncDecodable};

/// Simple deterministic key-value store
pub mod simple_db;
pub use simple_db::SimpleDb;

/// Hash of a block header (32 bytes).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HeaderHash(pub [u8; 32]);

impl FromStr for HeaderHash {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = bs58::decode(s).into_vec().map_err(|_e| {
            crate::Error::DecodeError("Failed to decode base58 HeaderHash")
        })?;
        if bytes.len() != 32 {
            return Err(crate::Error::DecodeError(
                "Invalid HeaderHash length: expected 32"
            ))
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(HeaderHash(hash))
    }
}

impl fmt::Display for HeaderHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", bs58::encode(self.0).into_string())
    }
}

/// Parse a sled record in the form of a tuple (`key`, `value`).
pub fn parse_record<T1: Decodable, T2: Decodable>(record: (IVec, IVec)) -> crate::Result<(T1, T2)> {
    let key = deserialize(&record.0)?;
    let value = deserialize(&record.1)?;

    Ok((key, value))
}

/// Parse a sled record with a u32 key, encoded in Big Endian bytes,
/// in the form of a tuple (`key`, `value`).
pub fn parse_u32_key_record<T: Decodable>(record: (IVec, IVec)) -> crate::Result<(u32, T)> {
    let key_bytes: [u8; 4] = record.0.as_ref().try_into().unwrap();
    let key = u32::from_be_bytes(key_bytes);
    let value = deserialize(&record.1)?;

    Ok((key, value))
}

/// Parse a sled record with a u64 key, encoded in Big Endian bytes,
/// in the form of a tuple (`key`, `value`).
pub fn parse_u64_key_record<T: Decodable>(record: (IVec, IVec)) -> crate::Result<(u64, T)> {
    let key_bytes: [u8; 8] = record.0.as_ref().try_into().unwrap();
    let key = u64::from_be_bytes(key_bytes);
    let value = deserialize(&record.1)?;

    Ok((key, value))
}

#[cfg(feature = "async-serial")]
/// Parse a sled record in the form of a tuple (`key`, `value`).
pub async fn parse_record_async<T1: AsyncDecodable, T2: AsyncDecodable>(
    record: (IVec, IVec),
) -> crate::Result<(T1, T2)> {
    let key = deserialize_async(&record.0).await?;
    let value = deserialize_async(&record.1).await?;

    Ok((key, value))
}

#[cfg(feature = "async-serial")]
/// Parse a sled record with a u32 key, encoded in Big Endian bytes,
/// in the form of a tuple (`key`, `value`).
pub async fn parse_u32_key_record_async<T: AsyncDecodable>(
    record: (IVec, IVec),
) -> crate::Result<(u32, T)> {
    let key_bytes: [u8; 4] = record.0.as_ref().try_into().unwrap();
    let key = u32::from_be_bytes(key_bytes);
    let value = deserialize_async(&record.1).await?;

    Ok((key, value))
}

#[cfg(feature = "async-serial")]
/// Parse a sled record with a u64 key, encoded in Big Endian bytes,
/// in the form of a tuple (`key`, `value`).
pub async fn parse_u64_key_record_async<T: AsyncDecodable>(
    record: (IVec, IVec),
) -> crate::Result<(u64, T)> {
    let key_bytes: [u8; 8] = record.0.as_ref().try_into().unwrap();
    let key = u64::from_be_bytes(key_bytes);
    let value = deserialize_async(&record.1).await?;

    Ok((key, value))
}
