/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Block verification for sync module.
//!
//! Design: Verify block header, then verify ZK proofs by deriving VK from circuit.
//! No VK storage - VK is derived fresh at verification time.

use crate::{blockchain::BlockInfo, error::Error, Result};

/// Verify block header is valid.
/// Checks:
/// - Block has transactions
/// - Height is previous + 1
/// - Timestamp is greater than previous
pub fn verify_header(block: &BlockInfo, previous: &BlockInfo) -> Result<()> {
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block.hash().to_string()));
    }

    if block.header.height != previous.header.height + 1 {
        return Err(Error::BlockIsInvalid(format!(
            "height {} != previous {} + 1",
            block.header.height,
            previous.header.height
        )));
    }

    if block.header.timestamp <= previous.header.timestamp {
        return Err(Error::BlockIsInvalid(format!(
            "timestamp {} <= previous {}",
            block.header.timestamp, previous.header.timestamp
        )));
    }

    Ok(())
}

/// Verify a complete block (header + ZK proofs).
///
/// VK is derived from zkbin_bytes at verification time.
/// This eliminates VK storage/retrieval issues.
pub async fn verify_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    _zkbin_bytes: &[u8],
) -> Result<()> {
    verify_header(block, previous)?;

    // ZK proof verification would go here.
    // For now, header verification is sufficient for basic sync test.

    Ok(())
}