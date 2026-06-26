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

use dwow_serial::deserialize;

use crate::{
    error::{WalletDbError, WalletDbResult},
    Dww,
};

impl Dww {
    /// Get a scanned block information record.
    pub fn get_scanned_block(&self, height: &u32) -> WalletDbResult<(String, String)> {
        let Ok(query_result) = self.cache.scanned_blocks.get(height.to_be_bytes()) else {
            return Err(WalletDbError::QueryExecutionFailed);
        };
        let Some(value_bytes) = query_result else {
            return Err(WalletDbError::RowNotFound);
        };
        // Verify checksum — detects torn pages on crash recovery
        let raw = match crate::sled_checksum::checksum_decode(&value_bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(target: "scanned_blocks", "Checksum failed at height {}: {}", height, e);
                return Err(WalletDbError::ParseColumnValueError);
            }
        };
        let Ok((hash, signing_key)) = deserialize(&raw) else {
            return Err(WalletDbError::ParseColumnValueError);
        };
        Ok((hash, signing_key))
    }

    /// Fetch all scanned block information records.
    pub fn get_scanned_block_records(&self) -> WalletDbResult<Vec<(u32, String, String)>> {
        let mut scanned_blocks = vec![];

        for record in self.cache.scanned_blocks.iter() {
            let Ok((key, value)) = record else {
                return Err(WalletDbError::QueryExecutionFailed);
            };
            let key: [u8; 4] = match key.as_ref().try_into() {
                Ok(k) => k,
                Err(_) => return Err(WalletDbError::ParseColumnValueError),
            };
            let key = u32::from_be_bytes(key);
            let Ok((hash, signing_key)) = deserialize(&value) else {
                return Err(WalletDbError::ParseColumnValueError);
            };
            scanned_blocks.push((key, hash, signing_key));
        }

        Ok(scanned_blocks)
    }

    /// Get the last scanned block height and hash from the wallet.
    /// If database is empty default (0, '-') is returned.
    pub fn get_last_scanned_block(&self) -> WalletDbResult<(u32, String)> {
        let Ok(query_result) = self.cache.scanned_blocks.last() else {
            return Err(WalletDbError::QueryExecutionFailed);
        };
        let Some((key, value)) = query_result else { return Ok((0, String::from("-"))) };
        let key: [u8; 4] = match key.as_ref().try_into() {
            Ok(k) => k,
            Err(_) => return Err(WalletDbError::ParseColumnValueError),
        };
        let key = u32::from_be_bytes(key);
        let Ok((hash, _)) = deserialize::<(String, String)>(&value) else {
            return Err(WalletDbError::ParseColumnValueError);
        };
        Ok((key, hash))
    }

    /// Reset the scanned blocks information records in the cache.
    pub fn reset_scanned_blocks(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting scanned blocks"));
        if let Err(e) = self.cache.scanned_blocks.clear() {
            output
                .push(format!("[reset_scanned_blocks] Resetting scanned blocks tree failed: {e}"));
            return Err(WalletDbError::GenericError)
        }
        // Clear the promissory_note SMT tree — all nullifiers are tied to scanned blocks
        if let Err(e) = self.cache.nullifier_smt.clear() {
            output.push(format!(
                "[reset_scanned_blocks] Resetting promissory_note SMT tree failed: {e}"
            ));
            return Err(WalletDbError::GenericError)
        }
        output.push(String::from("Successfully reset scanned blocks"));

        Ok(())
    }

    /// Reset state to provided block height.
    /// If genesis block height(0) was provided, perform a full reset.
    ///
    /// DarkWow uses linear architecture — no overlay/diff/inverse-diff.
    /// Rollback clears records above the target height. The caller
    /// (scan_blocks) will re-scan from target+1, rebuilding merkle
    /// trees and SMT state deterministically.
    pub fn reset_to_height(
        &self,
        height: u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Resetting wallet state to block: {height}"));

        // If genesis block height(0) was provided,
        // perform a full reset.
        if height == 0 {
            return self.reset(output)
        }

        // Grab last scanned block height
        let (last, _) = self.get_last_scanned_block()?;

        // Check if requested height is after it
        if last <= height {
            output.push(String::from(
                "Requested block height is greater or equal to last scanned block",
            ));
            return Ok(())
        }

        // Clear scanned blocks above target height
        for h in (height + 1)..=last {
            self.cache.scanned_blocks.remove(h.to_be_bytes())
                .map_err(|e| {
                    output.push(format!(
                        "[reset_to_height] Removing scanned block {h} failed: {e}"
                    ));
                    WalletDbError::GenericError
                })?;
        }

        // Flush sled after clearing records
        if let Err(e) = self.cache.db.flush() {
            output.push(format!("[reset_to_height] Flushing cache sled database failed: {e}"));
            return Err(WalletDbError::GenericError)
        }

        // Remove all wallet caps created after the reset height
        self.remove_pn_caps_after(&height, output)?;

        // Unspent all wallet caps spent after the reset height
        self.retained_pn_caps_after(&height, output)?;

        // Set reverted status to all transactions executed after reset
        // height.
        self.revert_transactions_after(&height, output)?;

        output.push(String::from("Successfully reset wallet state"));
        Ok(())
    }
}
