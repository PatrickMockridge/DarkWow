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

//! Wallet-owned utility functions — replaces dwow_core::util imports.
//!
//! These are hand-rolled implementations that match the dwow_core originals
//! in behavior but use the wallet's own error type and minimal dependencies.

use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::wallet_error::{Result, Error};

// ============================================================================
// expand_path — tilde expansion (~/ to $HOME)
// ============================================================================

/// Expands `~/` prefix to the user's home directory.
/// Matches `dwow_core::util::path::expand_path` behavior.
pub fn expand_path(path: &str) -> Result<PathBuf> {
    if path.starts_with("~/") {
        let homedir = home_dir()?;
        let remains = PathBuf::from(path.strip_prefix("~/").unwrap());
        Ok([homedir, remains].iter().collect())
    } else if path.starts_with('~') {
        home_dir()
    } else {
        Ok(PathBuf::from(path))
    }
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::NotFound("HOME environment variable not set".into()))
}

// ============================================================================
// encode_base10 / decode_base10 — decimal encoding for token amounts
// ============================================================================

/// Encodes a u64 integer into a base-10 string with `decimal_places` digits
/// after the decimal point. The integer must be ≤ 10^(decimal_places+15).
/// Matches `dwow_core::util::parse::encode_base10`.
pub fn encode_base10(amount: u64, decimal_places: usize) -> String {
    let mut s = amount.to_string();
    while s.len() <= decimal_places {
        s = format!("0{s}");
    }
    let point = s.len() - decimal_places;
    let whole = &s[0..point];
    let frac = &s[point..];
    // Trim trailing zeros
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{frac}")
    }
}

/// Decodes a base-10 decimal string into a u64 integer.
/// Matches `dwow_core::util::parse::decode_base10`.
pub fn decode_base10(amount: &str, decimal_places: usize, strict: bool) -> Result<u64> {
    let mut s: Vec<char> = amount.to_string().chars().collect();

    // Find and remove the decimal point
    let point: usize = if let Some(p) = amount.find('.') {
        s.remove(p);
        p
    } else {
        s.len()
    };

    // Only digits should remain
    for c in &s {
        if !c.is_ascii_digit() {
            return Err(Error::ParseFailed("Found non-digits".into()));
        }
    }

    // Pad with zeros if too few decimal places
    let actual_places = s.len() - point;
    if actual_places < decimal_places {
        s.extend(vec!['0'; decimal_places - actual_places]);
    }

    // Truncate and check rounding if too many decimal places
    let mut round = false;
    if actual_places > decimal_places {
        let end = point + decimal_places;
        for c in &s[end..s.len()] {
            if *c != '0' {
                round = true;
                break;
            }
        }
        s.truncate(end);
    }

    if strict && round {
        return Err(Error::ParseFailed("Would end up rounding while strict".into()));
    }

    let number = u64::from_str(&String::from_iter(&s))
        .map_err(|e| Error::Custom(format!("Parse int: {e}")))?;

    Ok(number)
}

// ============================================================================
// NanoTimestamp — nanosecond-precision timestamp
// ============================================================================

/// Nanosecond-precision timestamp. Matches `dwow_core::util::time::NanoTimestamp`.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq)]
pub struct NanoTimestamp(pub u128);

impl NanoTimestamp {
    pub fn inner(&self) -> u128 {
        self.0
    }

    pub const fn from_secs(secs: u128) -> Self {
        Self(secs * 1_000_000_000)
    }

    pub fn current_time() -> Self {
        let dur = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Self(dur.as_nanos())
    }

    pub fn elapsed(&self) -> Result<Self> {
        Self::current_time().checked_sub(*self)
    }

    pub fn checked_sub(&self, ts: Self) -> Result<Self> {
        self.inner()
            .checked_sub(ts.inner())
            .map(Self)
            .ok_or_else(|| Error::InvalidInput("timestamp underflow".into()))
    }

    pub fn checked_add(&self, ts: Self) -> Result<Self> {
        self.inner()
            .checked_add(ts.inner())
            .map(Self)
            .ok_or_else(|| Error::InvalidInput("timestamp overflow".into()))
    }
}

// ============================================================================
// base64 — encoding for wallet secrets display
// ============================================================================

/// Base64 encoding lookup tables — matches dwow_core::util::encoding::base64.
const E0: [i8; 256] = [
    0i8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 0, 0, 0, 63,
    52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 0, 0, 0, -1, 0, 0,
    0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
    15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 0, 0, 0, 0, 0,
    0, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const E1: [i8; 256] = [
    0i8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 0, 0, 0, 0, 0, 0,
    0, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const E2: [i8; 256] = [
    0i8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 0, 0, 0, 0, 0, 0,
    0, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Encode a byte slice into a base64 string.
/// Matches `dwow_core::util::encoding::base64::encode`.
pub fn base64_encode(data: &[u8]) -> String {
    let len = data.len();
    let mut dest = vec![0u8; ((4 * len / 3) + 3) & !3];
    let mut i = 0;
    let mut j = 0;

    if len > 2 {
        while i < len - 2 {
            let t1 = data[i];
            let t2 = data[i + 1];
            let t3 = data[i + 2];

            dest[j] = E0[t1 as usize] as u8;
            dest[j + 1] = E1[(((t1 & 0x03) << 4) | ((t2 >> 4) & 0x0F)) as usize] as u8;
            dest[j + 2] = E1[(((t2 & 0x0F) << 2) | ((t3 >> 6) & 0x03)) as usize] as u8;
            dest[j + 3] = E2[t3 as usize] as u8;

            i += 3;
            j += 4;
        }
    }
    match len - i {
        0 => {}
        1 => {
            let t1 = data[i];
            dest[j] = E0[t1 as usize] as u8;
            dest[j + 1] = E1[((t1 & 0x03) << 4) as usize] as u8;
            dest[j + 2] = b'=';
            dest[j + 3] = b'=';
        }
        _ => {
            let t1 = data[i] as usize;
            let t2 = data[i + 1] as usize;
            dest[j] = E0[t1] as u8;
            dest[j + 1] = E1[((t1 & 0x03) << 4) | ((t2 >> 4) & 0x0F)] as u8;
            dest[j + 2] = E2[(t2 & 0x0F) << 2] as u8;
            dest[j + 3] = b'=';
        }
    }

    String::from_utf8(dest).unwrap()
}

/// Tries to decode a base64 string into a byte vector.
/// Returns `None` if the input is invalid.
/// Matches `dwow_core::util::encoding::base64::decode`.
pub fn base64_decode(data: &str) -> Option<Vec<u8>> {
    if !data.is_ascii() || data.is_empty() {
        return None;
    }

    let data = match data.len() % 4 {
        1 => return None,
        2 => format!("{data}=="),
        3 => format!("{data}="),
        _ => data.to_string(),
    };

    let bytes = data.as_bytes();
    let mut out = Vec::with_capacity((data.len() / 4) * 3);
    let mut i = 0;

    while i < data.len() {
        let c1 = E0[bytes[i] as usize] as u32;
        let c2 = E0[bytes[i + 1] as usize] as u32;
        let c3 = E0[bytes[i + 2] as usize] as u32;
        let c4 = E0[bytes[i + 3] as usize] as u32;

        let b1 = bytes[i] as u32;
        let b2 = bytes[i + 1] as u32;
        let b3 = bytes[i + 2] as u32;
        let b4 = bytes[i + 3] as u32;

        if c1 == 255 || c2 == 255 || (c3 == 255 && b3 != b'=' as u32) || (c4 == 255 && b4 != b'=' as u32) {
            return None;
        }

        let n = (c1 << 18) | (c2 << 12) | (c3 << 6) | c4;
        out.push(((n >> 16) & 0xFF) as u8);
        if b3 != b'=' as u32 {
            out.push(((n >> 8) & 0xFF) as u8);
        }
        if b4 != b'=' as u32 {
            out.push((n & 0xFF) as u8);
        }
        i += 4;
    }

    Some(out)
}
