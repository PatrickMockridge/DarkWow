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

use rusqlite::types::Value;

use dwow_core::tx::Transaction;
use dwow_serial::{deserialize_async, serialize};

use crate::{
    error::{WalletDbError, WalletDbResult},
    wallet_error::Result,
    Dww,
};

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema.
const WALLET_TXS_HISTORY_TABLE: &str = "transactions_history";
const WALLET_TXS_HISTORY_COL_TX_HASH: &str = "transaction_hash";
const WALLET_TXS_HISTORY_COL_STATUS: &str = "status";
const WALLET_TXS_HISTORY_BLOCK_HEIGHT: &str = "block_height";
const WALLET_TXS_HISTORY_COL_TX: &str = "tx";

impl Dww {
    /// Insert or update a `Transaction` history record into the wallet,
    /// with the provided status, and store its inverse query into the cache.
    pub fn put_tx_history_record(
        &self,
        tx: &Transaction,
        status: &str,
        block_height: Option<u32>,
    ) -> WalletDbResult<String> {
        // Create an SQL `INSERT OR REPLACE` query
        let query = format!(
            "INSERT OR REPLACE INTO {WALLET_TXS_HISTORY_TABLE} ({WALLET_TXS_HISTORY_COL_TX_HASH}, {WALLET_TXS_HISTORY_COL_STATUS}, {WALLET_TXS_HISTORY_BLOCK_HEIGHT}, {WALLET_TXS_HISTORY_COL_TX}) VALUES (?1, ?2, ?3, ?4);"
        );

        // Execute the query
        let tx_hash = tx.hash().to_string();
        self.wallet
            .exec_sql(&query, rusqlite::params![tx_hash, status, block_height, &serialize(tx)])?;

        Ok(tx_hash)
    }

    // put_tx_history_records / get_tx_history_record REMOVED — callerless dead readers.

    // get_txs_history REMOVED — callerless dead reader.

    /// Reset the transaction history records in the wallet.
    pub fn reset_tx_history(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting transactions history"));
        let query = format!("DELETE FROM {WALLET_TXS_HISTORY_TABLE};");
        self.wallet.exec_sql(&query, &[])?;
        output.push(String::from("Successfully reset transactions history"));

        Ok(())
    }

    /// Set reverted status to the transaction history records in the
    /// wallet that where executed after provided height.
    pub fn revert_transactions_after(
        &self,
        height: &u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Reverting transactions history after: {height}"));
        let query = format!(
            "UPDATE {WALLET_TXS_HISTORY_TABLE} SET {WALLET_TXS_HISTORY_COL_STATUS} = 'Reverted', {WALLET_TXS_HISTORY_BLOCK_HEIGHT} = NULL WHERE {WALLET_TXS_HISTORY_BLOCK_HEIGHT} > ?1;"
        );
        self.wallet.exec_sql(&query, rusqlite::params![Some(*height)])?;
        output.push(String::from("Successfully reverted transactions history"));

        Ok(())
    }

    // remove_reverted_txs REMOVED — callerless dead method.
}
