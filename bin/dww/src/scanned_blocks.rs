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
    pub fn get_scanned_block(&self, height: &u32) -> WalletDbResult<(String, String)> {
        self.wallet.get_scanned_block(height)
    }

    pub fn get_scanned_block_records(&self) -> WalletDbResult<Vec<(u32, String, String)>> {
        self.wallet.get_scanned_block_records()
    }

    pub fn get_last_scanned_block(&self) -> WalletDbResult<(u32, String)> {
        self.wallet.get_last_scanned_block()
    }

    pub fn reset_scanned_blocks(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting scanned blocks"));
        self.wallet.reset_scanned_blocks_table().map_err(|e| {
            output.push(format!("[reset_scanned_blocks] Resetting scanned blocks failed: {e:?}"));
            WalletDbError::GenericError
        })?;
        output.push(String::from("Successfully reset scanned blocks"));
        Ok(())
    }

    pub fn reset_to_height(
        &self,
        height: u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Resetting wallet state to block: {height}"));

        let anchor_height = *smol::block_on(self.verified_anchor_height.lock());
        if height < anchor_height {
            return Err(WalletDbError::GenericError);
        }

        if height == 0 {
            return self.reset(output)
        }

        let (last, _) = self.get_last_scanned_block()?;

        if last <= height {
            output.push(String::from(
                "Requested block height is greater or equal to last scanned block",
            ));
            return Ok(())
        }

        self.wallet.delete_scanned_blocks_above(height).map_err(|e| {
            output.push(format!("[reset_to_height] Removing scanned blocks failed: {e:?}"));
            WalletDbError::GenericError
        })?;

        self.wallet.remove_capabilities_after(height).map_err(|e| {
            output.push(format!("[reset_to_height] Removing capabilities failed: {e:?}"));
            e
        })?;

        self.wallet.retain_capabilities_after(height).map_err(|e| {
            output.push(format!("[reset_to_height] Retaining capabilities failed: {e:?}"));
            e
        })?;

        output.push(String::from("Successfully reset wallet state"));
        Ok(())
    }
}
