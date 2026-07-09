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

use dwow_core::blockchain::HeaderHash;
use dwow_sdk::crypto::{MerkleTree, SecretKey};
use dwow_serial::{deserialize, serialize};
use rusqlite::{
    params,
    types::ToSql,
    Connection,
};
use tracing::{debug, error};

use crate::error::{WalletDbError, WalletDbResult};

pub type WalletPtr = Arc<WalletDb>;

/// A held capability record — discovered via AEAD decryption, stored in the
/// `held_capabilities` table. Every spendable capability (PN note, native token,
/// bearer bond) is represented as a CapRecord.
/// Exercising a capability publishes a nullifier (tracked as `revoked`).
#[derive(Debug, Clone)]
pub struct CapRecord {
    pub cap_id: String,
    pub value: u64,
    pub token_id: String,
    pub spend_hook: Option<String>,
    pub user_data: Option<String>,
    pub leaf_position: u64,
    pub secret: String,
    pub cap_blind: String,
    pub value_blind: String,
    pub token_blind: String,
    pub revoked: bool,
    pub revoked_at_height: Option<u32>,
    pub created_at_height: u32,
}

/// Merkle proof for a capability commitment in the note tree.
#[derive(Debug, Clone)]
pub struct MerkleProof {
    pub siblings: Vec<String>,
    pub root: String,
}

// BondNoteRecord removed — dead struct, zero references. Bond tables also removed from wallet.sql.

/// Structure representing a discovered capability stored in the generic
/// capabilities table. Every AEAD-decrypted output is stored here regardless
/// of whether the note type is recognized. Structured decoders (NativeToken,
// CapabilityRecord (generic AEAD store) REMOVED — table + reader both dead.

/// Structure representing base wallet database operations.
pub struct WalletDb {
    /// Connection to the SQLite database. Shared via Arc for PnSmtStorage.
    pub conn: Arc<Mutex<Connection>>,
}

impl WalletDb {
    /// Create a new wallet database handler. If `path` is `None`, create it in memory.
    pub fn new(path: Option<PathBuf>, password: Option<&str>, production: bool) -> WalletDbResult<WalletPtr> {
        let Ok(conn) = (match path.clone() {
            Some(p) => Connection::open(p),
            None => Connection::open_in_memory(),
        }) else {
            return Err(WalletDbError::ConnectionFailed);
        };

        if let Some(password) = password {
            if !password.is_empty() {
                if production {
                    // SQLCipher KDF hardening for production mode
                    let _ = conn.pragma_update(None, "kdf_iter", 256_000);
                    let _ = conn.pragma_update(None, "cipher_hmac_algorithm", "HMAC_SHA512");
                }
                if let Err(e) = conn.pragma_update(None, "key", password) {
                    error!(target: "walletdb::new", "[WalletDb] Pragma update failed: {e}");
                    return Err(WalletDbError::PragmaUpdateError);
                };
                // Verify the password works in production mode
                if production {
                    let test: std::result::Result<i64, _> = conn.query_row(
                        "SELECT count(*) FROM sqlite_master", [], |row| row.get(0)
                    );
                    if test.is_err() {
                        error!(target: "walletdb::new", "[WalletDb] Password verification failed — wrong password or DB corruption");
                        return Err(WalletDbError::PragmaUpdateError);
                    }
                }
            }
        }
        if let Err(e) = conn.pragma_update(None, "foreign_keys", "ON") {
            error!(target: "walletdb::new", "[WalletDb] Pragma update failed: {e}");
            return Err(WalletDbError::PragmaUpdateError);
        };
        // Retry for up to 5s on SQLITE_BUSY — daemon may be mid-write
        if let Err(e) = conn.pragma_update(None, "busy_timeout", "5000") {
            error!(target: "walletdb::new", "[WalletDb] Pragma busy_timeout failed: {e}");
            return Err(WalletDbError::PragmaUpdateError);
        };
        // WAL mode — better crash recovery, concurrent reads, lower write amp
        if let Err(e) = conn.pragma_update(None, "journal_mode", "WAL") {
            error!(target: "walletdb::new", "[WalletDb] Pragma journal_mode failed: {e}");
            return Err(WalletDbError::PragmaUpdateError);
        };

        debug!(target: "walletdb::new", "[WalletDb] Opened Sqlite connection at \"{path:?}\"");
        Ok(Arc::new(Self { conn: Arc::new(Mutex::new(conn)) }))
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
    // create_prepared_statement / generate_select_query / query_single / query_multiple /
    // query_custom REMOVED — callerless (only referenced by dead txs_history readers).

    /// Get all held capabilities, optionally filtered by exercised status.
    pub fn get_held_capabilities(&self, revoked: Option<bool>) -> WalletDbResult<Vec<CapRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT cap_id, value, token_id, spend_hook, user_data,
                    leaf_position, secret, cap_blind, value_blind, token_blind,
                    revoked, revoked_at_height, created_at_height
             FROM held_capabilities WHERE (?1 IS NULL OR revoked = ?1)
             ORDER BY cap_id",
        )?;

        let revoked_param: Option<i64> = revoked.map(|r| if r { 1 } else { 0 });
        let mut rows = stmt.query(params![revoked_param])?;

        let mut caps = vec![];
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    let cap_id: String = row.get(0).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value: i64 = row.get(1).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_id: String = row.get(2).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spend_hook: Option<String> = row.get(3).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let user_data: Option<String> = row.get(4).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let leaf_position: i64 = row.get(5).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let secret: String = row.get(6).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let cap_blind: String = row.get(7).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value_blind: String = row.get(8).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_blind: String = row.get(9).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spent_val: i64 = row.get(10).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let revoked_at_height: Option<i64> = row.get(11).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let created_at_height: i64 = row.get(12).map_err(|_| WalletDbError::QueryExecutionFailed)?;

                    caps.push(CapRecord {
                        cap_id,
                        value: value as u64,
                        token_id,
                        spend_hook,
                        user_data,
                        leaf_position: leaf_position as u64,
                        secret,
                        cap_blind,
                        value_blind,
                        token_blind,
                        revoked: spent_val != 0,
                        revoked_at_height: revoked_at_height.map(|h| h as u32),
                        created_at_height: created_at_height as u32,
                    });
                }
                Ok(None) => break,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            }
        }

        Ok(caps)
    }

    /// Get held capabilities for a specific token ID.
    pub fn get_capabilities_for_token(&self, token_id: &str, revoked: Option<bool>) -> WalletDbResult<Vec<CapRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT cap_id, value, token_id, spend_hook, user_data,
                    leaf_position, secret, cap_blind, value_blind, token_blind,
                    revoked, revoked_at_height, created_at_height
             FROM held_capabilities WHERE token_id = ?1 AND revoked = ?2",
        )?;

        let revoked_param: Option<i64> = revoked.map(|r| if r { 1 } else { 0 });
        let mut rows = stmt.query(params![token_id, revoked_param])?;

        let mut caps = vec![];
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    let cap_id: String = row.get(0).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value: i64 = row.get(1).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_id: String = row.get(2).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spend_hook: Option<String> = row.get(3).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let user_data: Option<String> = row.get(4).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let leaf_position: i64 = row.get(5).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let secret: String = row.get(6).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let cap_blind: String = row.get(7).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value_blind: String = row.get(8).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let token_blind: String = row.get(9).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let spent_val: i64 = row.get(10).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let revoked_at_height: Option<i64> = row.get(11).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let created_at_height: i64 = row.get(12).map_err(|_| WalletDbError::QueryExecutionFailed)?;

                    caps.push(CapRecord {
                        cap_id,
                        value: value as u64,
                        token_id,
                        spend_hook,
                        user_data,
                        leaf_position: leaf_position as u64,
                        secret,
                        cap_blind,
                        value_blind,
                        token_blind,
                        revoked: spent_val != 0,
                        revoked_at_height: revoked_at_height.map(|h| h as u32),
                        created_at_height: created_at_height as u32,
                    });
                }
                Ok(None) => break,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            }
        }

        Ok(caps)
    }

    /// Mark a held capability as revoked (nullifier published on-chain).
    pub fn mark_revoked(&self, cap_id: &str, block_height: u32) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "UPDATE held_capabilities SET revoked = 1, revoked_at_height = ?1 WHERE cap_id = ?2",
            params![block_height as i64, cap_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    // mark_retained REMOVED — callerless dead method.

    /// Insert a held capability with Merkle proof.
    /// Wrapped in an explicit transaction so a crash between the two INSERTs
    /// does not leave an orphaned capability without a Merkle proof.
    pub fn insert_capability(&self, cap: &CapRecord, proof: &MerkleProof) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;

        let proof_json = serde_json::to_string(&proof.siblings)
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        conn.execute("BEGIN TRANSACTION", [])
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        let result = (|| -> WalletDbResult<()> {
            conn.execute(
                "INSERT OR IGNORE INTO held_capabilities (cap_id, value, token_id, spend_hook, user_data,
                    leaf_position, secret, cap_blind, value_blind, token_blind,
                    revoked, revoked_at_height, created_at_height)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    cap.cap_id,
                    cap.value as i64,
                    cap.token_id,
                    cap.spend_hook,
                    cap.user_data,
                    cap.leaf_position as i64,
                    cap.secret,
                    cap.cap_blind,
                    cap.value_blind,
                    cap.token_blind,
                    if cap.revoked { 1 } else { 0 },
                    cap.revoked_at_height.map(|h| h as i64),
                    cap.created_at_height as i64,
                ],
            )
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;

            conn.execute(
                "INSERT OR IGNORE INTO capability_proofs (cap_id, merkle_proof, merkle_root) VALUES (?1, ?2, ?3)",
                params![cap.cap_id, proof_json, proof.root],
            )
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;

            Ok(())
        })();

        if result.is_err() {
            conn.execute("ROLLBACK", []).ok();
            return result;
        }

        conn.execute("COMMIT", [])
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        Ok(())
    }

    /// Get Merkle proof for a cap.
    pub fn get_merkle_proof(&self, cap_id: &str) -> WalletDbResult<MerkleProof> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT merkle_proof, merkle_root FROM capability_proofs WHERE cap_id = ?1",
        )?;

        let mut rows = stmt.query(params![cap_id])?;
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

    // ── Key lifecycle persistence ───────────────────────────────────────

    /// Load the persisted lifecycle keys JSON blob (encrypted AES-256). Returns
    /// None if the table/row doesn't exist or is empty.
    pub fn load_key_lifecycle(&self) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        let mut stmt = conn.prepare("SELECT blob FROM key_lifecycle WHERE id = 1").ok()?;
        stmt.query_row([], |row| row.get(0)).ok()
    }

    /// Save the lifecycle keys JSON blob (from AccountManager::to_json_string).
    pub fn save_key_lifecycle(&self, blob: &str) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO key_lifecycle (id, blob) VALUES (1, ?1)",
            params![blob],
        ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Remove caps after a certain block height.
    pub fn remove_capabilities_after(&self, height: u32) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let height = height as i64;

        // Delete capability_proofs for caps being deleted (only those CREATED
        // above the reorg target — revocation is handled by retain_capabilities_after).
        conn.execute(
            "DELETE FROM capability_proofs WHERE cap_id IN
             (SELECT cap_id FROM held_capabilities WHERE created_at_height > ?1)",
            params![height],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        // F1-fix: only delete caps CREATED above the reorg target. The old
        // `OR revoked_at_height > ?1` clause deleted still-valid coins that
        // happened to be revoked above the target — before retain_capabilities_after
        // could un-revoke them. Revocation is handled separately by retain.
        conn.execute(
            "DELETE FROM held_capabilities WHERE created_at_height > ?1",
            params![height],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;

        Ok(())
    }

    /// Retain (un-revoke) capabilities revoked after a given block height.
    /// Used during reorg to restore capabilities that were marked as exercised
    /// on blocks that no longer exist.
    pub fn retain_capabilities_after(&self, height: u32) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let height = height as i64;
        conn.execute(
            "UPDATE held_capabilities SET revoked = 0, revoked_at_height = NULL
             WHERE revoked = 1 AND revoked_at_height > ?1",
            params![height],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    // import_secrets_batch REMOVED — the addresses table is gone; the wallet
    // derives its identity on boot via AccountManager (no key store).

    // insert_generic_capability REMOVED — the generic capabilities table is gone;
    // only held_capabilities (typed, with Merkle proofs) lives on the scan→balance path.
    // get_capabilities REMOVED — callerless; CapabilityRecord struct also removed.
    // get_token / get_all_tokens / get_aliases / insert_alias REMOVED — callerless dead.

    // get_addresses / insert_address REMOVED — the addresses table is gone; the
    // wallet derives its identity on boot via AccountManager (no key store).
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

// ContractInteractionRecord REMOVED — table + all readers dead.

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

    /// Atomic insert of contract metadata + manifest in a single transaction.
    /// Eliminates the race condition between insert_contract_metadata() and
    /// the subsequent UPDATE from store_manifest().
    pub fn insert_contract_metadata_with_manifest(
        &self,
        record: &ContractMetadataRecord,
        manifest_json: Option<&str>,
    ) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO contract_metadata (contract_id, name, symbol,
              category, description, public, deployer_pubkey, deploy_height,
              attestations_json, manifest_json, lock_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.contract_id, record.name, record.symbol, record.category,
                record.description, record.public, record.deployer_pubkey,
                record.deploy_height, record.attestations_json,
                manifest_json.unwrap_or(""),
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

    // get_transactions_history / insert_contract_interaction / get_contract_interactions /
    // get_contract_id_by_name REMOVED — callerless dead methods.

    /// Look up a contract name by its contract_id (forward lookup in contract_metadata).
    /// Returns the human-readable name if the contract was discovered during scan.
    pub fn get_contract_name_by_id(&self, contract_id: &str) -> WalletDbResult<Option<String>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT name FROM contract_metadata WHERE contract_id = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![contract_id])?;
        match rows.next() {
            Ok(Some(row)) => Ok(Some(row.get(0)?)),
            Ok(None) => Ok(None),
            Err(_) => Err(WalletDbError::QueryExecutionFailed),
        }
    }

    /// Store a contract manifest as JSON in the contract_metadata table.
    /// Uses dedicated `manifest_json` column — NOT attestations_json.
    /// Attestations are separate on-chain data from Identity+Attestation contracts.
    pub fn store_manifest(
        &self,
        contract_id: &str,
        manifest_json: &str,
    ) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "UPDATE contract_metadata SET manifest_json = ?1 WHERE contract_id = ?2",
            params![manifest_json, contract_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Retrieve a stored contract manifest.
    /// Reads from dedicated `manifest_json` column.
    /// Returns None if no manifest was stored for this contract.
    pub fn get_contract_manifest(
        &self,
        contract_id: &str,
    ) -> WalletDbResult<Option<dwow_sdk::manifest::ContractManifest>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT manifest_json FROM contract_metadata WHERE contract_id = ?1",
        )?;
        let mut rows = stmt.query(params![contract_id])?;
        match rows.next()? {
            Some(row) => {
                let json_str: String = row.get(0)?;
                if json_str.is_empty() || json_str == "[]" {
                    Ok(None)
                } else {
                    // Try TOML first (legacy), then JSON (current format)
                    dwow_sdk::manifest::ContractManifest::from_toml(&json_str)
                        .map(Some)
                        .or_else(|_| {
                            Ok(serde_json::from_str::<dwow_sdk::manifest::ContractManifest>(&json_str).ok())
                        })
                }
            }
            None => Ok(None),
        }
    }

    // ── Cache methods (merged from cache.rs) ──────────────────────

    pub fn insert_merkle_trees(&self, trees: &[(&[u8], &MerkleTree)]) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        for (key, tree) in trees {
            let raw = serialize(*tree);
            let checked = crate::sled_checksum::checksum_encode(&raw);
            conn.execute(
                "INSERT OR REPLACE INTO merkle_trees (name, tree_blob) VALUES (?1, ?2)",
                rusqlite::params![key, checked],
            ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        }
        Ok(())
    }

    pub fn get_merkle_tree(&self, name: &[u8]) -> Option<MerkleTree> {
        let conn = self.conn.lock().ok()?;
        let tree_bytes: Vec<u8> = conn.query_row(
            "SELECT tree_blob FROM merkle_trees WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        ).ok()?;
        let raw = crate::sled_checksum::checksum_decode(&tree_bytes).ok()?;
        deserialize(&raw).ok()
    }

    pub fn insert_scanned_block(
        &self, height: &u32, hash: &HeaderHash, signing_key: &Option<SecretKey>,
    ) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let hash_str = hash.to_string();
        let key_str = match signing_key {
            Some(key) => key.to_string(),
            None => String::from("-"),
        };
        conn.execute(
            "INSERT OR REPLACE INTO scanned_blocks (height, hash, signing_key) VALUES (?1, ?2, ?3)",
            rusqlite::params![height, hash_str, key_str],
        ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    pub fn get_scanned_block(&self, height: &u32) -> WalletDbResult<(String, String)> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.query_row(
            "SELECT hash, signing_key FROM scanned_blocks WHERE height = ?1",
            rusqlite::params![height],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => WalletDbError::RowNotFound,
            _ => WalletDbError::QueryExecutionFailed,
        })
    }

    pub fn get_scanned_block_records(&self) -> WalletDbResult<Vec<(u32, String, String)>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT height, hash, signing_key FROM scanned_blocks ORDER BY height ASC",
        ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        }).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        let mut scanned = vec![];
        for row in rows {
            scanned.push(row.map_err(|_| WalletDbError::ParseColumnValueError)?);
        }
        Ok(scanned)
    }

    pub fn get_last_scanned_block(&self) -> WalletDbResult<(u32, String)> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.query_row(
            "SELECT height, hash FROM scanned_blocks ORDER BY height DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok((0, String::from("-"))),
            _ => Err(WalletDbError::QueryExecutionFailed),
        })
    }

    pub fn reset_scanned_blocks_table(&self) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute("DELETE FROM scanned_blocks", [])
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    pub fn delete_scanned_blocks_above(&self, height: u32) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute("DELETE FROM scanned_blocks WHERE height > ?1", params![height])
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }


    // ── Chain block methods (replaces sled LinearStore) ──────────────

    pub fn insert_block(&self, height: u64, block: &dwow_chain::Block) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let block_json = serde_json::to_string(block)
            .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        conn.execute(
            "INSERT OR REPLACE INTO chain_blocks (height, block_json) VALUES (?1, ?2)",
            params![height as i64, block_json],
        ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    pub fn get_block(&self, height: u64) -> WalletDbResult<dwow_chain::Block> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT block_json FROM chain_blocks WHERE height = ?1",
        )?;
        stmt.query_row(params![height as i64], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => WalletDbError::RowNotFound,
            _ => WalletDbError::QueryExecutionFailed,
        })
        .and_then(|json| {
            serde_json::from_str(&json)
                .map_err(|_| WalletDbError::QueryExecutionFailed)
        })
    }

    pub fn chain_height(&self) -> WalletDbResult<u64> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let height: i64 = conn.query_row(
            "SELECT COALESCE(MAX(height), 0) FROM chain_blocks",
            [],
            |row| row.get(0),
        )?;
        Ok(height as u64)
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

    // test_mem_wallet REMOVED — tested query_single/query_custom (both dead).
    // test_query_single REMOVED — tested dead query_single/query_custom.
    // test_query_multi REMOVED — tested dead query_multiple/query_custom.

    // test_insert_and_get_capabilities REMOVED — tested get_capabilities (dead).

    // test_address_stores_secret REMOVED — the addresses table / key store is gone;
    // the wallet derives its identity on boot via AccountManager. Key-path coverage
    // now lives in the dwow-accounts determinism tests + the full-path integration test.
}
