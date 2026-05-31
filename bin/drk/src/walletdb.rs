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

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::{
    params,
    types::{ToSql, Value},
    Connection,
};
use tracing::{debug, error};

use crate::error::{WalletDbError, WalletDbResult};

pub type WalletPtr = Arc<WalletDb>;

/// Structure representing a coin record from the database.
/// This includes all data needed to spend a coin, including
/// Merkle proof information.
#[derive(Debug, Clone)]
pub struct CoinRecord {
    pub coin_id: String,
    pub value: u64,
    pub token_id: String,
    pub spend_hook: Option<String>,
    pub user_data: Option<String>,
    pub leaf_position: u64,
    pub secret: String,
    pub coin_blind: String,
    pub value_blind: String,
    pub token_blind: String,
    pub spent: bool,
    pub spent_at_height: Option<u32>,
    pub created_at_height: u32,
}

/// Structure representing a Merkle proof for a coin.
#[derive(Debug, Clone)]
pub struct MerkleProof {
    pub siblings: Vec<String>,
    pub root: String,
}

/// Structure representing a bearer bond coin record for wallet storage.
#[derive(Debug, Clone)]
pub struct BondCoinRecord {
    pub coin_id: String,
    pub value_commit_x: String,
    pub value_commit_y: String,
    pub token_commit: String,
    pub spend_hook: String,
    pub user_data: String,
    pub leaf_position: u64,
    pub secret: String,
    pub coin_blind: String,
    pub value_blind: String,
    pub token_blind: String,
    pub last_claim_block: u64,
    pub maturity_block: u64,
    pub issuer_contract: String,
    pub interest_rate_bps: u64,
    pub spent: bool,
    pub spent_at_height: Option<u32>,
    pub created_at_height: u32,
}

/// Structure representing base wallet database operations.
pub struct WalletDb {
    /// Connection to the SQLite database.
    pub conn: Mutex<Connection>,
}

impl WalletDb {
    /// Create a new wallet database handler. If `path` is `None`, create it in memory.
    pub fn new(path: Option<PathBuf>, password: Option<&str>) -> WalletDbResult<WalletPtr> {
        let Ok(conn) = (match path.clone() {
            Some(p) => Connection::open(p),
            None => Connection::open_in_memory(),
        }) else {
            return Err(WalletDbError::ConnectionFailed);
        };

        if let Some(password) = password {
            if !password.is_empty() {
                if let Err(e) = conn.pragma_update(None, "key", password) {
                    error!(target: "walletdb::new", "[WalletDb] Pragma update failed: {e}");
                    return Err(WalletDbError::PragmaUpdateError);
                };
            }
        }
        if let Err(e) = conn.pragma_update(None, "foreign_keys", "ON") {
            error!(target: "walletdb::new", "[WalletDb] Pragma update failed: {e}");
            return Err(WalletDbError::PragmaUpdateError);
        };

        debug!(target: "walletdb::new", "[WalletDb] Opened Sqlite connection at \"{path:?}\"");
        Ok(Arc::new(Self { conn: Mutex::new(conn) }))
    }

    /// This function executes a given SQL query that contains multiple SQL statements,
    /// that don't contain any parameters.
    pub fn exec_batch_sql(&self, query: &str) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_batch_sql", "[WalletDb] Executing batch SQL query:\n{query}");
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };
        if let Err(e) = conn.execute_batch(query) {
            error!(target: "walletdb::exec_batch_sql", "[WalletDb] Query failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed)
        };

        Ok(())
    }

    /// This function executes a given SQL query, but isn't able to return anything.
    /// Therefore it's best to use it for initializing a table or similar things.
    pub fn exec_sql(&self, query: &str, params: &[&dyn ToSql]) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_sql", "[WalletDb] Executing SQL query:\n{query}");
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };

        // If no params are provided, execute directly
        if params.is_empty() {
            if let Err(e) = conn.execute(query, ()) {
                error!(target: "walletdb::exec_sql", "[WalletDb] Query failed: {e}");
                return Err(WalletDbError::QueryExecutionFailed)
            };
            return Ok(())
        }

        // First we prepare the query
        let Ok(mut stmt) = conn.prepare(query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided params
        if let Err(e) = stmt.execute(params) {
            error!(target: "walletdb::exec_sql", "[WalletDb] Query failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Finalize query and drop connection lock
        if let Err(e) = stmt.finalize() {
            error!(target: "walletdb::exec_sql", "[WalletDb] Query finalization failed: {e}");
            return Err(WalletDbError::QueryFinalizationFailed)
        };
        drop(conn);

        Ok(())
    }

    /// Generate a new statement for provided query and bind the provided params,
    /// returning the raw SQL query as a string.
    pub fn create_prepared_statement(
        &self,
        query: &str,
        params: &[&dyn ToSql],
    ) -> WalletDbResult<String> {
        debug!(target: "walletdb::create_prepared_statement", "[WalletDb] Preparing statement for SQL query:\n{query}");
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };

        // First we prepare the query
        let Ok(mut stmt) = conn.prepare(query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Bind all provided params
        for (index, param) in params.iter().enumerate() {
            if stmt.raw_bind_parameter(index + 1, param).is_err() {
                return Err(WalletDbError::QueryPreparationFailed)
            };
        }

        // Grab the raw SQL
        let query = stmt.expanded_sql().unwrap();

        // Drop statement and the connection lock
        drop(stmt);
        drop(conn);

        Ok(query)
    }

    /// Generate a `SELECT` query for provided table from selected column names and
    /// provided `WHERE` clauses. Named parameters are supported in the `WHERE` clauses,
    /// assuming they follow the normal formatting ":{column_name}".
    fn generate_select_query(
        &self,
        table: &str,
        col_names: &[&str],
        params: &[(&str, &dyn ToSql)],
    ) -> String {
        let mut query = if col_names.is_empty() {
            format!("SELECT * FROM {table}")
        } else {
            format!("SELECT {} FROM {table}", col_names.join(", "))
        };
        if params.is_empty() {
            return query
        }

        let mut where_str = Vec::with_capacity(params.len());
        for (k, _) in params {
            let col = &k[1..];
            where_str.push(format!("{col} = {k}"));
        }
        query.push_str(&format!(" WHERE {}", where_str.join(" AND ")));

        query
    }

    /// Query provided table from selected column names and provided `WHERE` clauses,
    /// for a single row.
    pub fn query_single(
        &self,
        table: &str,
        col_names: &[&str],
        params: &[(&str, &dyn ToSql)],
    ) -> WalletDbResult<Vec<Value>> {
        // Generate `SELECT` query
        let query = self.generate_select_query(table, col_names, params);
        debug!(target: "walletdb::query_single", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };

        let Ok(mut stmt) = conn.prepare(&query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided params
        let Ok(mut rows) = stmt.query(params) else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Check if row exists
        let Ok(next) = rows.next() else { return Err(WalletDbError::QueryExecutionFailed) };
        let row = match next {
            Some(row_result) => row_result,
            None => return Err(WalletDbError::RowNotFound),
        };

        // Grab returned values
        let mut result = vec![];
        if col_names.is_empty() {
            let mut idx = 0;
            loop {
                let Ok(value) = row.get(idx) else { break };
                result.push(value);
                idx += 1;
            }
        } else {
            for col in col_names {
                let Ok(value) = row.get(*col) else {
                    return Err(WalletDbError::ParseColumnValueError)
                };
                result.push(value);
            }
        }

        Ok(result)
    }

    /// Query provided table from selected column names and provided `WHERE` clauses,
    /// for multiple rows.
    pub fn query_multiple(
        &self,
        table: &str,
        col_names: &[&str],
        params: &[(&str, &dyn ToSql)],
    ) -> WalletDbResult<Vec<Vec<Value>>> {
        // Generate `SELECT` query
        let query = self.generate_select_query(table, col_names, params);
        debug!(target: "walletdb::query_multiple", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };
        let Ok(mut stmt) = conn.prepare(&query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided converted params
        let Ok(mut rows) = stmt.query(params) else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Loop over returned rows and parse them
        let mut result = vec![];
        loop {
            // Check if an error occured
            let row = match rows.next() {
                Ok(r) => r,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            };

            // Check if no row was returned
            let row = match row {
                Some(r) => r,
                None => break,
            };

            // Grab row returned values
            let mut row_values = vec![];
            if col_names.is_empty() {
                let mut idx = 0;
                loop {
                    let Ok(value) = row.get(idx) else { break };
                    row_values.push(value);
                    idx += 1;
                }
            } else {
                for col in col_names {
                    let Ok(value) = row.get(*col) else {
                        return Err(WalletDbError::ParseColumnValueError)
                    };
                    row_values.push(value);
                }
            }
            result.push(row_values);
        }

        Ok(result)
    }

    /// Query provided table using provided query for multiple rows.
    pub fn query_custom(
        &self,
        query: &str,
        params: &[&dyn ToSql],
    ) -> WalletDbResult<Vec<Vec<Value>>> {
        debug!(target: "walletdb::query_custom", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };
        let Ok(mut stmt) = conn.prepare(query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided converted params
        let Ok(mut rows) = stmt.query(params) else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Loop over returned rows and parse them
        let mut result = vec![];
        loop {
            // Check if an error occured
            let row = match rows.next() {
                Ok(r) => r,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            };

            // Check if no row was returned
            let row = match row {
                Some(r) => r,
                None => break,
            };

            // Grab row returned values
            let mut row_values = vec![];
            let mut idx = 0;
            loop {
                let Ok(value) = row.get(idx) else { break };
                row_values.push(value);
                idx += 1;
            }
            result.push(row_values);
        }

        Ok(result)
    }

    /// Get all coins, optionally filtered by spent status.
    pub fn get_coins(&self, spent: bool) -> WalletDbResult<Vec<CoinRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT coin_id, value, token_id, spend_hook, user_data,
                    leaf_position, secret, coin_blind, value_blind, token_blind,
                    spent, spent_at_height, created_at_height
             FROM coins WHERE spent = ?1",
        )?;

        let spent_int = if spent { 1 } else { 0 };
        let mut rows = stmt.query(params![spent_int])?;

        let mut coins = vec![];
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    let coin_id: String = row.get(0).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value: i64 = row.get(1).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_id: String = row.get(2).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spend_hook: Option<String> = row.get(3).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let user_data: Option<String> = row.get(4).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let leaf_position: i64 = row.get(5).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let secret: String = row.get(6).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let coin_blind: String = row.get(7).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value_blind: String = row.get(8).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_blind: String = row.get(9).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spent_int: i64 = row.get(10).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spent_at_height: Option<i64> = row.get(11).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let created_at_height: i64 = row.get(12).map_err(|_| WalletDbError::QueryExecutionFailed)?;

                    coins.push(CoinRecord {
                        coin_id,
                        value: value as u64,
                        token_id,
                        spend_hook,
                        user_data,
                        leaf_position: leaf_position as u64,
                        secret,
                        coin_blind,
                        value_blind,
                        token_blind,
                        spent: spent_int != 0,
                        spent_at_height: spent_at_height.map(|h| h as u32),
                        created_at_height: created_at_height as u32,
                    });
                }
                Ok(None) => break,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            }
        }

        Ok(coins)
    }

    /// Get coins for a specific token ID.
    pub fn get_token_coins(&self, token_id: &str, spent: bool) -> WalletDbResult<Vec<CoinRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT coin_id, value, token_id, spend_hook, user_data,
                    leaf_position, secret, coin_blind, value_blind, token_blind,
                    spent, spent_at_height, created_at_height
             FROM coins WHERE token_id = ?1 AND spent = ?2",
        )?;

        let spent_int = if spent { 1 } else { 0 };
        let mut rows = stmt.query(params![token_id, spent_int])?;

        let mut coins = vec![];
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    let coin_id: String = row.get(0).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value: i64 = row.get(1).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_id: String = row.get(2).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spend_hook: Option<String> = row.get(3).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let user_data: Option<String> = row.get(4).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let leaf_position: i64 = row.get(5).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let secret: String = row.get(6).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let coin_blind: String = row.get(7).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value_blind: String = row.get(8).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_blind: String = row.get(9).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spent_int: i64 = row.get(10).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spent_at_height: Option<i64> = row.get(11).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let created_at_height: i64 = row.get(12).map_err(|_| WalletDbError::QueryExecutionFailed)?;

                    coins.push(CoinRecord {
                        coin_id,
                        value: value as u64,
                        token_id,
                        spend_hook,
                        user_data,
                        leaf_position: leaf_position as u64,
                        secret,
                        coin_blind,
                        value_blind,
                        token_blind,
                        spent: spent_int != 0,
                        spent_at_height: spent_at_height.map(|h| h as u32),
                        created_at_height: created_at_height as u32,
                    });
                }
                Ok(None) => break,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            }
        }

        Ok(coins)
    }

    /// Mark a coin as spent.
    pub fn mark_coin_spent(&self, coin_id: &str, block_height: u32) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "UPDATE coins SET spent = 1, spent_at_height = ?1 WHERE coin_id = ?2",
            params![block_height as i64, coin_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Mark a coin as unspent.
    pub fn mark_coin_unspent(&self, coin_id: &str) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "UPDATE coins SET spent = 0, spent_at_height = NULL WHERE coin_id = ?1",
            params![coin_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Insert a coin with Merkle proof.
    pub fn insert_coin(&self, coin: &CoinRecord, proof: &MerkleProof) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;

        conn.execute(
            "INSERT INTO coins (coin_id, value, token_id, spend_hook, user_data,
                leaf_position, secret, coin_blind, value_blind, token_blind,
                spent, spent_at_height, created_at_height)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                coin.coin_id,
                coin.value as i64,
                coin.token_id,
                coin.spend_hook,
                coin.user_data,
                coin.leaf_position as i64,
                coin.secret,
                coin.coin_blind,
                coin.value_blind,
                coin.token_blind,
                if coin.spent { 1 } else { 0 },
                coin.spent_at_height.map(|h| h as i64),
                coin.created_at_height as i64,
            ],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        // Insert Merkle proof
        let proof_json = serde_json::to_string(&proof.siblings)
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        conn.execute(
            "INSERT INTO coin_merkle_proofs (coin_id, merkle_proof, merkle_root) VALUES (?1, ?2, ?3)",
            params![coin.coin_id, proof_json, proof.root],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        Ok(())
    }

    /// Insert a bearer bond coin record into the wallet database.
    pub fn insert_bond_coin(&self, coin: &BondCoinRecord, proof: &MerkleProof) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;

        conn.execute(
            "INSERT INTO bond_coins (coin_id, value_commit_x, value_commit_y, token_commit,
                spend_hook, user_data, leaf_position, secret, coin_blind, value_blind, token_blind,
                last_claim_block, maturity_block, issuer_contract, interest_rate_bps,
                spent, spent_at_height, created_at_height)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                coin.coin_id,
                coin.value_commit_x,
                coin.value_commit_y,
                coin.token_commit,
                coin.spend_hook,
                coin.user_data,
                coin.leaf_position as i64,
                coin.secret,
                coin.coin_blind,
                coin.value_blind,
                coin.token_blind,
                coin.last_claim_block as i64,
                coin.maturity_block as i64,
                coin.issuer_contract,
                coin.interest_rate_bps as i64,
                if coin.spent { 1 } else { 0 },
                coin.spent_at_height.map(|h| h as i64),
                coin.created_at_height as i64,
            ],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        // Insert Merkle proof
        let proof_json = serde_json::to_string(&proof.siblings)
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        conn.execute(
            "INSERT INTO coin_merkle_proofs (coin_id, merkle_proof, merkle_root) VALUES (?1, ?2, ?3)",
            params![coin.coin_id, proof_json, proof.root],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        Ok(())
    }

    /// Get Merkle proof for a coin.
    pub fn get_merkle_proof(&self, coin_id: &str) -> WalletDbResult<MerkleProof> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT merkle_proof, merkle_root FROM coin_merkle_proofs WHERE coin_id = ?1",
        )?;

        let mut rows = stmt.query(params![coin_id])?;
        let row = rows
            .next()
            .map_err(|_| WalletDbError::QueryExecutionFailed)?
            .ok_or(WalletDbError::RowNotFound)?;

        let proof_json: String = row.get(0)?;
        let root: String = row.get(1)?;

        let siblings: Vec<String> =
            serde_json::from_str(&proof_json).map_err(|_| WalletDbError::QueryExecutionFailed)?;

        Ok(MerkleProof { siblings, root })
    }

    /// Remove coins after a certain block height.
    pub fn remove_coins_after(&self, height: u32) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let height = height as i64;

        // Delete coin_merkle_proofs for coins being deleted
        conn.execute(
            "DELETE FROM coin_merkle_proofs WHERE coin_id IN
             (SELECT coin_id FROM coins WHERE spent_at_height > ?1 OR created_at_height > ?1)",
            params![height],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        // Delete coins
        conn.execute(
            "DELETE FROM coins WHERE spent_at_height > ?1 OR created_at_height > ?1",
            params![height],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        Ok(())
    }

    /// Get secrets for all coins.
    pub fn get_secrets(&self) -> WalletDbResult<Vec<String>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare("SELECT secret FROM coin_secrets")?;
        let mut rows = stmt.query([])?;

        let mut secrets = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            let secret: String = row.get(0)?;
            secrets.push(secret);
        }

        Ok(secrets)
    }

    /// Insert a secret.
    pub fn insert_secret(&self, secret: &str, coin_id: &str) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR IGNORE INTO coin_secrets (secret, coin_id) VALUES (?1, ?2)",
            params![secret, coin_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Insert a deploy authority.
    pub fn insert_deploy_auth(&self, contract_id: &str, secret: &str) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO deploy_authorities (contract_id, secret, is_locked, created_at)
             VALUES (?1, ?2, 0, ?3)",
            params![contract_id, secret, now],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Get all deploy authorities.
    pub fn get_deploy_authorities(&self) -> WalletDbResult<Vec<(String, String, bool, Option<u32>)>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT contract_id, secret, is_locked, created_at_height
             FROM deploy_authorities ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        let mut result = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            let contract_id: String = row.get(0)?;
            let secret: String = row.get(1)?;
            let is_locked: i64 = row.get(2)?;
            let created_at_height: Option<i64> = row.get(3)?;
            result.push((contract_id, secret, is_locked != 0, created_at_height.map(|h| h as u32)));
        }
        Ok(result)
    }

    /// Remove all deploy authorities (for reset).
    pub fn remove_deploy_authorities(&self) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute("DELETE FROM deploy_authorities", [])
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Register a contract ID in the persistent registry.
    pub fn register_contract(&self, name: &str, contract_id: &str) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO contract_registry (contract_name, contract_id) VALUES (?1, ?2)",
            params![name, contract_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Load all registered contract IDs from the persistent registry.
    pub fn get_contract_registry(&self) -> WalletDbResult<Vec<(String, String)>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT contract_name, contract_id FROM contract_registry",
        )?;
        let mut rows = stmt.query([])?;
        let mut result = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            let name: String = row.get(0)?;
            let cid: String = row.get(1)?;
            result.push((name, cid));
        }
        Ok(result)
    }

    /// Get a token by its token_id or name/alias.
    pub fn get_token(&self, identifier: &str) -> WalletDbResult<Option<TokenInfo>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;

        // Try to find by token_id first (exact match)
        let mut stmt = conn.prepare(
            "SELECT token_id, name, symbol, decimals, mint_authority, token_blind,
                    is_frozen, freeze_height, created_at_height
             FROM tokens WHERE token_id = ?1",
        )?;

        let mut rows = stmt.query(params![identifier])?;
        if let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            return Ok(Some(TokenInfo {
                token_id: row.get(0)?,
                name: row.get(1)?,
                symbol: row.get(2)?,
                decimals: row.get::<_, i64>(3)? as u8,
                mint_authority: row.get(4)?,
                token_blind: row.get(5)?,
                is_frozen: row.get::<_, i64>(6)? != 0,
                freeze_height: row.get(7)?,
                created_at_height: row.get::<_, i64>(8)? as u32,
            }));
        }

        // Try to find by name/alias
        let mut stmt = conn.prepare(
            "SELECT token_id, name, symbol, decimals, mint_authority, token_blind,
                    is_frozen, freeze_height, created_at_height
             FROM tokens WHERE name = ?1",
        )?;

        let mut rows = stmt.query(params![identifier])?;
        if let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            return Ok(Some(TokenInfo {
                token_id: row.get(0)?,
                name: row.get(1)?,
                symbol: row.get(2)?,
                decimals: row.get::<_, i64>(3)? as u8,
                mint_authority: row.get(4)?,
                token_blind: row.get(5)?,
                is_frozen: row.get::<_, i64>(6)? != 0,
                freeze_height: row.get(7)?,
                created_at_height: row.get::<_, i64>(8)? as u32,
            }));
        }

        Ok(None)
    }

    /// Insert a token into the database.
    pub fn insert_token(&self, token: &TokenInfo) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO tokens (token_id, name, symbol, decimals,
             mint_authority, token_blind, is_frozen, freeze_height, created_at_height)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                token.token_id,
                token.name,
                token.symbol,
                token.decimals as i64,
                token.mint_authority,
                token.token_blind,
                token.is_frozen as i64,
                token.freeze_height,
                token.created_at_height as i64,
            ],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Get all tokens from the database.
    pub fn get_all_tokens(&self) -> WalletDbResult<Vec<TokenInfo>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT token_id, name, symbol, decimals, mint_authority, token_blind,
                    is_frozen, freeze_height, created_at_height
             FROM tokens ORDER BY created_at_height DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut tokens = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            tokens.push(TokenInfo {
                token_id: row.get(0)?,
                name: row.get(1)?,
                symbol: row.get(2)?,
                decimals: row.get::<_, i64>(3)? as u8,
                mint_authority: row.get(4)?,
                token_blind: row.get(5)?,
                is_frozen: row.get::<_, i64>(6)? != 0,
                freeze_height: row.get(7)?,
                created_at_height: row.get::<_, i64>(8)? as u32,
            });
        }
        Ok(tokens)
    }

    /// Get all aliases from the database.
    pub fn get_aliases(&self) -> WalletDbResult<Vec<AliasRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT token_id, alias FROM aliases ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut aliases = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            aliases.push(AliasRecord {
                token_id: row.get(0)?,
                alias: row.get(1)?,
            });
        }
        Ok(aliases)
    }

    /// Insert a new alias into the database.
    pub fn insert_alias(&self, alias: &str, token_id: &str, _is_default: i64) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO aliases (alias, token_id) VALUES (?1, ?2)",
            params![alias, token_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Get all addresses from the database.
    pub fn get_addresses(&self) -> WalletDbResult<Vec<AddressRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT id, public_key, secret, is_default, created_at, created_at_height FROM addresses ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        let mut addresses = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            addresses.push(AddressRecord {
                id: row.get(0)?,
                public_key: row.get(1)?,
                secret: row.get(2)?,
                is_default: row.get::<_, i64>(3)? != 0,
                created_at: row.get(4)?,
                created_at_height: row.get::<_, i64>(5)? as u32,
            });
        }
        Ok(addresses)
    }

    /// Insert a new address into the database.
    pub fn insert_address(
        &self,
        public_key: &str,
        secret: &str,
        is_default: bool,
        _created_at: i64,
    ) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let result = conn.execute(
            "INSERT INTO addresses (public_key, secret, is_default, created_at, created_at_height) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![public_key, secret, is_default as i64, _created_at, 0],
        );
        if let Err(e) = result {
            error!(target: "walletdb::insert_address", "[WalletDb] Insert address failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed);
        }
        Ok(())
    }
}

/// Alias record from the database.
#[derive(Debug, Clone)]
pub struct AliasRecord {
    pub token_id: String,
    pub alias: String,
}

/// Address record from the database.
#[derive(Debug, Clone)]
pub struct AddressRecord {
    pub id: i64,
    pub public_key: String,
    pub secret: String,
    pub is_default: bool,
    pub created_at: i64,
    pub created_at_height: u32,
}

/// Structure representing on-chain contract metadata.
#[derive(Debug, Clone)]
pub struct ContractMetadataRecord {
    pub contract_id: String,
    pub name: String,
    pub symbol: Option<String>,
    pub category: String,
    pub description: Option<String>,
    pub public: bool,
    pub deployer_pubkey: String,
    pub deploy_height: u32,
    pub attestations_json: String,
    pub lock_status: String,
}

/// Structure representing a contract interaction record.
#[derive(Debug, Clone)]
pub struct ContractInteractionRecord {
    pub contract_id: String,
    pub function_name: String,
    pub tx_hash: String,
    pub block_height: Option<u32>,
    pub timestamp: i64,
}

impl WalletDb {
    /// Insert or update on-chain contract metadata discovered during scan.
    pub fn insert_contract_metadata(&self, record: &ContractMetadataRecord) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO contract_metadata
             (contract_id, name, symbol, category, description, public,
              deployer_pubkey, deploy_height, attestations_json, lock_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.contract_id,
                record.name,
                record.symbol,
                record.category,
                record.description,
                if record.public { 1 } else { 0 },
                record.deployer_pubkey,
                record.deploy_height as i64,
                record.attestations_json,
                record.lock_status,
            ],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Get metadata for a single contract by its ContractId.
    pub fn get_contract_metadata(&self, contract_id: &str) -> WalletDbResult<ContractMetadataRecord> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT contract_id, name, symbol, category, description, public,
                    deployer_pubkey, deploy_height, attestations_json, lock_status
             FROM contract_metadata WHERE contract_id = ?1",
        )?;
        let mut rows = stmt.query(params![contract_id])?;
        let row = rows.next()
            .map_err(|_| WalletDbError::QueryExecutionFailed)?
            .ok_or(WalletDbError::RowNotFound)?;
        Ok(ContractMetadataRecord {
            contract_id: row.get(0)?,
            name: row.get(1)?,
            symbol: row.get(2)?,
            category: row.get(3)?,
            description: row.get(4)?,
            public: row.get::<_, i64>(5)? != 0,
            deployer_pubkey: row.get(6)?,
            deploy_height: row.get::<_, i64>(7)? as u32,
            attestations_json: row.get(8)?,
            lock_status: row.get(9)?,
        })
    }

    /// Get all contract metadata, optionally filtered by public visibility.
    pub fn get_contract_metadata_list(&self, public_only: bool) -> WalletDbResult<Vec<ContractMetadataRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let query = if public_only {
            "SELECT contract_id, name, symbol, category, description, public,
                    deployer_pubkey, deploy_height, attestations_json, lock_status
             FROM contract_metadata WHERE public = 1 ORDER BY deploy_height DESC"
        } else {
            "SELECT contract_id, name, symbol, category, description, public,
                    deployer_pubkey, deploy_height, attestations_json, lock_status
             FROM contract_metadata ORDER BY deploy_height DESC"
        };
        let mut stmt = conn.prepare(query)?;
        let mut rows = stmt.query([])?;
        let mut records = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            records.push(ContractMetadataRecord {
                contract_id: row.get(0)?,
                name: row.get(1)?,
                symbol: row.get(2)?,
                category: row.get(3)?,
                description: row.get(4)?,
                public: row.get::<_, i64>(5)? != 0,
                deployer_pubkey: row.get(6)?,
                deploy_height: row.get::<_, i64>(7)? as u32,
                attestations_json: row.get(8)?,
                lock_status: row.get(9)?,
            });
        }
        Ok(records)
    }

    /// Get contract metadata filtered by category.
    pub fn get_contract_metadata_by_category(&self, category: &str) -> WalletDbResult<Vec<ContractMetadataRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT contract_id, name, symbol, category, description, public,
                    deployer_pubkey, deploy_height, attestations_json, lock_status
             FROM contract_metadata WHERE category = ?1 AND public = 1 ORDER BY deploy_height DESC",
        )?;
        let mut rows = stmt.query(params![category])?;
        let mut records = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            records.push(ContractMetadataRecord {
                contract_id: row.get(0)?,
                name: row.get(1)?,
                symbol: row.get(2)?,
                category: row.get(3)?,
                description: row.get(4)?,
                public: row.get::<_, i64>(5)? != 0,
                deployer_pubkey: row.get(6)?,
                deploy_height: row.get::<_, i64>(7)? as u32,
                attestations_json: row.get(8)?,
                lock_status: row.get(9)?,
            });
        }
        Ok(records)
    }

    /// Insert a transaction history record.
    pub fn insert_transaction_history(
        &self,
        tx_hash: &str,
        status: &str,
        block_height: Option<u32>,
        tx_blob: &[u8],
    ) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO transactions_history (transaction_hash, status, block_height, tx)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                tx_hash,
                status,
                block_height.map(|h| h as i64),
                tx_blob,
            ],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Get transaction history records.
    pub fn get_transactions_history(&self) -> WalletDbResult<Vec<(String, String, Option<u32>)>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT transaction_hash, status, block_height FROM transactions_history ORDER BY block_height DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut result = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            let tx_hash: String = row.get(0)?;
            let status: String = row.get(1)?;
            let block_height: Option<i64> = row.get(2)?;
            result.push((tx_hash, status, block_height.map(|h| h as u32)));
        }
        Ok(result)
    }

    /// Insert a contract interaction record.
    pub fn insert_contract_interaction(
        &self,
        contract_id: &str,
        function_name: &str,
        tx_hash: &str,
        block_height: Option<u32>,
        timestamp: i64,
    ) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT INTO contract_interactions (contract_id, function_name, tx_hash, block_height, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                contract_id,
                function_name,
                tx_hash,
                block_height.map(|h| h as i64),
                timestamp,
            ],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Get contract interactions for a given contract.
    pub fn get_contract_interactions(&self, contract_id: &str) -> WalletDbResult<Vec<ContractInteractionRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT contract_id, function_name, tx_hash, block_height, timestamp
             FROM contract_interactions WHERE contract_id = ?1 ORDER BY timestamp DESC",
        )?;
        let mut rows = stmt.query(params![contract_id])?;
        let mut result = vec![];
        while let Some(row) = rows.next().map_err(|_| WalletDbError::QueryExecutionFailed)? {
            result.push(ContractInteractionRecord {
                contract_id: row.get(0)?,
                function_name: row.get(1)?,
                tx_hash: row.get(2)?,
                block_height: row.get::<_, Option<i64>>(3)?.map(|h| h as u32),
                timestamp: row.get(4)?,
            });
        }
        Ok(result)
    }

    /// Look up a contract_id by name (reverse lookup in contract_metadata).
    pub fn get_contract_id_by_name(&self, name: &str) -> WalletDbResult<Option<String>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT contract_id FROM contract_metadata WHERE name = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![name])?;
        match rows.next() {
            Ok(Some(row)) => Ok(Some(row.get(0)?)),
            Ok(None) => Ok(None),
            Err(_) => Err(WalletDbError::QueryExecutionFailed),
        }
    }
}

/// Token information stored in wallet database.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token_id: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: u8,
    pub mint_authority: Option<String>,
    pub token_blind: String,
    pub is_frozen: bool,
    pub freeze_height: Option<u32>,
    pub created_at_height: u32,
}

/// Custom implementation of rusqlite::named_params! to use `expr` instead of `literal` as `$param_name`,
/// and append the ":" named parameters prefix.
#[macro_export]
macro_rules! convert_named_params {
    () => {
        &[] as &[(&str, &dyn rusqlite::types::ToSql)]
    };
    ($(($param_name:expr, $param_val:expr)),+ $(,)?) => {
        &[$((format!(":{}", $param_name).as_str(), &$param_val as &dyn rusqlite::types::ToSql)),+] as &[(&str, &dyn rusqlite::types::ToSql)]
    };
}

#[cfg(test)]
mod tests {
    use rusqlite::types::Value;

    use crate::walletdb::WalletDb;

    #[test]
    fn test_mem_wallet() {
        let wallet = WalletDb::new(None, Some("foobar")).unwrap();
        wallet
            .exec_batch_sql(
                "CREATE TABLE mista ( numba INTEGER ); INSERT INTO mista ( numba ) VALUES ( 42 );",
            )
            .unwrap();

        let ret = wallet.query_single("mista", &["numba"], &[]).unwrap();
        assert_eq!(ret.len(), 1);
        let numba: i64 = if let Value::Integer(numba) = ret[0] { numba } else { -1 };
        assert_eq!(numba, 42);

        let ret = wallet.query_custom("SELECT numba FROM mista;", &[]).unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0].len(), 1);
        let numba: i64 = if let Value::Integer(numba) = ret[0][0] { numba } else { -1 };
        assert_eq!(numba, 42);
    }

    #[test]
    fn test_query_single() {
        let wallet = WalletDb::new(None, None).unwrap();
        wallet
            .exec_batch_sql("CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );")
            .unwrap();

        let why = 42;
        let are = "are".to_string();
        let you = 69;
        let gae = vec![42u8; 32];

        wallet
            .exec_sql(
                "INSERT INTO mista ( why, are, you, gae ) VALUES (?1, ?2, ?3, ?4);",
                rusqlite::params![why, are, you, gae],
            )
            .unwrap();

        let ret = wallet.query_single("mista", &["why", "are", "you", "gae"], &[]).unwrap();
        assert_eq!(ret.len(), 4);
        assert_eq!(ret[0], Value::Integer(why));
        assert_eq!(ret[1], Value::Text(are.clone()));
        assert_eq!(ret[2], Value::Integer(you));
        assert_eq!(ret[3], Value::Blob(gae.clone()));
        let ret = wallet.query_custom("SELECT why, are, you, gae FROM mista;", &[]).unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0].len(), 4);
        assert_eq!(ret[0][0], Value::Integer(why));
        assert_eq!(ret[0][1], Value::Text(are.clone()));
        assert_eq!(ret[0][2], Value::Integer(you));
        assert_eq!(ret[0][3], Value::Blob(gae.clone()));

        let ret = wallet
            .query_single(
                "mista",
                &["gae"],
                rusqlite::named_params! {":why": why, ":are": are, ":you": you},
            )
            .unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0], Value::Blob(gae.clone()));
        let ret = wallet
            .query_custom(
                "SELECT gae FROM mista WHERE why = ?1 AND are = ?2 AND you = ?3;",
                rusqlite::params![why, are, you],
            )
            .unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0].len(), 1);
        assert_eq!(ret[0][0], Value::Blob(gae));
    }

    #[test]
    fn test_query_multi() {
        let wallet = WalletDb::new(None, None).unwrap();
        wallet
            .exec_batch_sql("CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );")
            .unwrap();

        let why = 42;
        let are = "are".to_string();
        let you = 69;
        let gae = vec![42u8; 32];

        wallet
            .exec_sql(
                "INSERT INTO mista ( why, are, you, gae ) VALUES (?1, ?2, ?3, ?4);",
                rusqlite::params![why, are, you, gae],
            )
            .unwrap();
        wallet
            .exec_sql(
                "INSERT INTO mista ( why, are, you, gae ) VALUES (?1, ?2, ?3, ?4);",
                rusqlite::params![why, are, you, gae],
            )
            .unwrap();

        let ret = wallet.query_multiple("mista", &[], &[]).unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 4);
            assert_eq!(row[0], Value::Integer(why));
            assert_eq!(row[1], Value::Text(are.clone()));
            assert_eq!(row[2], Value::Integer(you));
            assert_eq!(row[3], Value::Blob(gae.clone()));
        }
        let ret = wallet.query_custom("SELECT * FROM mista;", &[]).unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 4);
            assert_eq!(row[0], Value::Integer(why));
            assert_eq!(row[1], Value::Text(are.clone()));
            assert_eq!(row[2], Value::Integer(you));
            assert_eq!(row[3], Value::Blob(gae.clone()));
        }

        let ret = wallet
            .query_multiple(
                "mista",
                &["gae"],
                convert_named_params! {("why", why), ("are", are), ("you", you)},
            )
            .unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 1);
            assert_eq!(row[0], Value::Blob(gae.clone()));
        }
        let ret = wallet
            .query_custom(
                "SELECT gae FROM mista WHERE why = ?1 AND are = ?2 AND you = ?3;",
                rusqlite::params![why, are, you],
            )
            .unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 1);
            assert_eq!(row[0], Value::Blob(gae.clone()));
        }
    }
}
