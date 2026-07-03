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

//! Scanned block records — SQLite-backed (formerly sled _scanned_blocks tree).

use crate::{
    error::{WalletDbError, WalletDbResult},
    Dww,
};

impl Dww {
    /// Get a scanned block information record.
    pub fn get_scanned_block(&self, height: &u32) -> WalletDbResult<(String, String)> {
        let result: Result<(String, String), _> = self.cache.conn.lock().unwrap().query_row(
            "SELECT hash, signing_key FROM scanned_blocks WHERE height = ?1",
            rusqlite::params![height],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Err(WalletDbError::RowNotFound),
            Err(_) => Err(WalletDbError::QueryExecutionFailed),
        }
    }

    /// Fetch all scanned block information records.
    pub fn get_scanned_block_records(&self) -> WalletDbResult<Vec<(u32, String, String)>> {
        let conn = self.cache.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT height, hash, signing_key FROM scanned_blocks ORDER BY height ASC"
        ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        }).map_err(|_| WalletDbError::QueryExecutionFailed)?;

        let mut scanned_blocks = vec![];
        for row in rows {
            scanned_blocks.push(row.map_err(|_| WalletDbError::ParseColumnValueError)?);
        }
        Ok(scanned_blocks)
    }

    /// Get the last scanned block height and hash from the wallet.
    /// If database is empty default (0, '-') is returned.
    pub fn get_last_scanned_block(&self) -> WalletDbResult<(u32, String)> {
        let result: Result<(u32, String), _> = self.cache.conn.lock().unwrap().query_row(
            "SELECT height, hash FROM scanned_blocks ORDER BY height DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match result {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok((0, String::from("-"))),
            Err(_) => Err(WalletDbError::QueryExecutionFailed),
        }
    }

    /// Reset the scanned blocks information records in the cache.
    pub fn reset_scanned_blocks(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting scanned blocks"));
        if let Err(e) = self.cache.conn.lock().unwrap().execute("DELETE FROM scanned_blocks", []) {
            output.push(format!("[reset_scanned_blocks] Resetting scanned blocks failed: {e}"));
            return Err(WalletDbError::GenericError)
        }
        // Clear the nullifier SMT — all nullifiers are tied to scanned blocks
        if let Err(e) = self.cache.conn.lock().unwrap().execute("DELETE FROM nullifier_smt", []) {
            output.push(format!(
                "[reset_scanned_blocks] Resetting nullifier SMT failed: {e}"
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

        // Guard: refuse to roll back below verified anchor height.
        let anchor_height = *smol::block_on(self.verified_anchor_height.lock());
        if height < anchor_height {
            return Err(WalletDbError::GenericError);
        }

        // If genesis block height(0) was provided, perform a full reset.
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
        self.cache.conn.lock().unwrap().execute(
            "DELETE FROM scanned_blocks WHERE height > ?1",
            rusqlite::params![height],
        ).map_err(|e| {
            output.push(format!("[reset_to_height] Removing scanned blocks failed: {e}"));
            WalletDbError::GenericError
        })?;

        // Remove all wallet capabilities created after the reset height
        self.wallet.remove_capabilities_after(height)
            .map_err(|e| {
                output.push(format!("[reset_to_height] Removing capabilities failed: {e}"));
                e
            })?;

        // Unspent all wallet capabilities spent after the reset height
        self.wallet.retain_capabilities_after(height)
            .map_err(|e| {
                output.push(format!("[reset_to_height] Retaining capabilities failed: {e}"));
                e
            })?;

        // Set reverted status to all transactions executed after reset height.
        self.revert_transactions_after(&height, output)?;

        output.push(String::from("Successfully reset wallet state"));
        Ok(())
    }
}
