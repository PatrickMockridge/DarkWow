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
use dwow_chain::CoinCommitment;
use dwow_sdk::crypto::{
    BaseBlind, Blind, ContractId, FuncId, MerkleTree, ScalarBlind, SecretKey, TokenId,
};
use dwow_sdk::pasta::{group::ff::PrimeField, pallas};
use dwow_serial::{deserialize, serialize};
use serde::Serialize;
use rusqlite::{
    params,
    types::ToSql,
    Connection,
};
use tracing::{debug, error};

use crate::error::{WalletDbError, WalletDbResult};
use crate::contract_imports::NATIVE_TOKEN_CONTRACT_ID;
use dwow_sdk::capability::{
    barbs_from_csv, barbs_to_csv, primitives_from_csv, primitives_to_csv, Barb, Primitive,
};

pub type WalletPtr = Arc<WalletDb>;

/// A held capability record — discovered via AEAD decryption, stored in the
/// `held_capabilities` table. Every spendable capability (PN note, native token,
/// bearer bond) is represented as a CapRecord.
/// Exercising a capability publishes a nullifier (tracked as `revoked`).
#[derive(Debug, Clone)]
pub struct CapRecord {
    pub cap_id: String,
    pub value: u64,
    /// AssetId (↓denominate) — typed per type-system.md §8.1
    pub asset_id: TokenId,
    /// Spend hook (↓gate) — spending condition
    pub spend_hook: Option<FuncId>,
    /// Raw user data field element
    pub user_data: Option<[u8; 32]>,
    pub leaf_position: u64,
    /// Capability commitment (↓commit) — poseidon_hash(cap_attrs)
    pub commitment: CoinCommitment,
    /// ContractId (↓dispatch) — the contract this capability routes to
    pub contract_id: ContractId,
    /// FuncId (↓gate) — the function this capability exercises
    pub func_id: Option<FuncId>,
    /// Capability discriminant from the contract manifest (u8).
    /// None for Path 1 (native token) capabilities or pre-manifest discoveries.
    pub capability_discriminant: Option<u8>,
    /// BaseBlind — capability commitment blinding factor
    pub cap_blind: BaseBlind,
    /// ScalarBlind — value blinding factor
    pub value_blind: ScalarBlind,
    /// BaseBlind — asset blinding factor
    pub asset_blind: BaseBlind,
    /// HAZOP WP-7: capability lifecycle status. NULL = unspent (spendable).
    /// Pending = broadcast but unmined. Processing = mined, immature.
    /// Spent = fully confirmed (>= CONFIRMATION_DEPTH blocks).
    pub status: Option<crate::capability::CapStatus>,
    /// Block height at which the current status was set.
    /// For Pending: broadcast height. For Processing/Spent: mined height.
    pub status_height: Option<u64>,
    /// HAZOP WP-7: derived from status — true if Processing or Spent.
    /// Kept for backward compatibility with existing code; set during
    /// CapRecord construction from SQL rows or constructors.
    pub revoked: bool,
    /// Height at which the nullifier was seen on-chain. Set alongside status.
    /// Kept for backward compatibility; mirrors status_height for spent states.
    pub revoked_at_height: Option<u64>,
    pub created_at_height: u64,
    /// Identification record (account index + derivation) that lets the
    /// spend path re-derive the owning secret via AccountManager::resolve_key.
    /// NOT key material — safe to store at rest. None for pre-upgrade caps.
    pub key_coords: Option<dwow_accounts::KeyCoordinates>,
    /// Manifest capability name (None for native Path 1 / pre-manifest).
    pub capability_name: Option<String>,
    /// TypedCapability resource / action identity (ocap.md §3).
    pub resource: Option<String>,
    pub action: Option<String>,
    /// Composed primitive types + covered barbs (canonical, sorted). Empty for
    /// native/pre-manifest capabilities.
    pub primitives: Vec<Primitive>,
    pub barbs: Vec<Barb>,
}

/// Merkle proof for a capability commitment in the note tree.
#[derive(Debug, Clone, Serialize)]
pub struct MerkleProof {
    pub siblings: Vec<String>,
    pub root: String,
}

// BondNoteRecord removed — dead struct, zero references. Bond tables also removed from wallet.sql.

/// Structure representing a discovered capability stored in the generic
/// capabilities table. Every AEAD-decrypted output is stored here regardless
/// of whether the note type is recognized. Structured decoders (NativeToken,
// CapabilityRecord (generic AEAD store) REMOVED — table + reader both dead.

/// Helper: convert a 32-byte array into a typed crypto wrapper.
/// Used by SELECT readers to reconstruct CapRecord from stored BLOBs.
fn bytes_to_asset_id(bytes: [u8; 32]) -> WalletDbResult<TokenId> {
    TokenId::from_bytes(bytes).map_err(|_e| WalletDbError::QueryExecutionFailed)
}
fn bytes_to_contract_id(bytes: [u8; 32]) -> WalletDbResult<ContractId> {
    ContractId::from_bytes(bytes).map_err(|_e| WalletDbError::QueryExecutionFailed)
}
fn bytes_to_func_id(bytes: [u8; 32]) -> WalletDbResult<FuncId> {
    FuncId::from_bytes(bytes).map_err(|_| WalletDbError::QueryExecutionFailed)
}
fn bytes_to_commitment(bytes: [u8; 32]) -> WalletDbResult<CoinCommitment> {
    CoinCommitment::from_bytes(bytes).map_err(|_| WalletDbError::QueryExecutionFailed)
}
fn bytes_to_base_blind(bytes: [u8; 32]) -> WalletDbResult<BaseBlind> {
    match pallas::Base::from_repr(bytes).into() {
        Some(v) => Ok(Blind(v)),
        None => Err(WalletDbError::QueryExecutionFailed),
    }
}
fn bytes_to_scalar_blind(bytes: [u8; 32]) -> WalletDbResult<ScalarBlind> {
    match pallas::Scalar::from_repr(bytes).into() {
        Some(v) => Ok(Blind(v)),
        None => Err(WalletDbError::QueryExecutionFailed),
    }
}

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
            "SELECT cap_id, value, asset_id_blob, asset_id, spend_hook_blob, spend_hook,
                    user_data_blob, user_data,
                    leaf_position, commitment_blob, commitment, cap_blind_blob, cap_blind,
                    value_blind_blob, value_blind, asset_blind_blob, asset_blind,
                    contract_id_blob, func_id_blob, capability_discriminant,
                    revoked, revoked_at_height, created_at_height,
                    capability_name, resource, action, primitives_csv, barbs_csv, key_coords_blob,
                    status, status_height
             FROM held_capabilities WHERE (?1 IS NULL OR revoked = ?1)
             ORDER BY cap_id",
        )?;

        let revoked_param: Option<i64> = revoked.map(|r| if r { 1 } else { 0 });
        let mut rows = stmt.query(params![revoked_param])?;

        // Column index constants for the SELECT above
        const C_ASSET_BLOB: usize = 2;
        const C_ASSET_TEXT: usize = 3;
        const C_SPEND_BLOB: usize = 4;
        const C_SPEND_TEXT: usize = 5;
        const C_USER_BLOB: usize = 6;
        const C_USER_TEXT: usize = 7;
        const C_COMMIT_BLOB: usize = 9;
        const C_COMMIT_TEXT: usize = 10;
        const C_CAPBLIND_BLOB: usize = 11;
        const C_CAPBLIND_TEXT: usize = 12;
        const C_VALBLIND_BLOB: usize = 13;
        const C_VALBLIND_TEXT: usize = 14;
        const C_ASSETBLIND_BLOB: usize = 15;
        const C_ASSETBLIND_TEXT: usize = 16;
        const C_CONTRACT_BLOB: usize = 17;
        const C_FUNC_BLOB: usize = 18;
        // Capability discriminant — referenced by external tooling
        #[allow(dead_code)]
        const C_DISCRIMINANT: usize = 19;

        /// Read a [u8; 32] from a BLOB column, falling back to bs58 decode from a TEXT column.
        fn read_blob32_text_fallback(
            row: &rusqlite::Row,
            idx_blob: usize,
            idx_text: usize,
        ) -> std::result::Result<[u8; 32], rusqlite::Error> {
            match row.get::<_, Vec<u8>>(idx_blob) {
                Ok(v) if v.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&v);
                    Ok(arr)
                }
                _ => {
                    let text: String = row.get(idx_text)?;
                    let bytes = bs58::decode(&text).into_vec()
                        .map_err(|_| rusqlite::Error::InvalidColumnName("bs58 decode failed".into()))?;
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    Ok(arr)
                }
            }
        }

        /// Read an Option<[u8; 32]> from nullable BLOB/TEXT columns.
        fn read_opt_blob32_text_fallback(
            row: &rusqlite::Row,
            idx_blob: usize,
            idx_text: usize,
        ) -> std::result::Result<Option<[u8; 32]>, rusqlite::Error> {
            match row.get::<_, Option<Vec<u8>>>(idx_blob) {
                Ok(Some(v)) if v.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&v);
                    Ok(Some(arr))
                }
                Ok(Some(_)) => {
                    // Non-32-byte BLOB — fall through to TEXT
                    let text: Option<String> = row.get(idx_text)?;
                    match text {
                        Some(t) if !t.is_empty() => {
                            let bytes = bs58::decode(&t).into_vec()
                                .map_err(|_| rusqlite::Error::InvalidColumnName("bs58 decode failed".into()))?;
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Ok(Some(arr))
                        }
                        _ => Ok(None),
                    }
                }
                _ => {
                    let text: Option<String> = row.get(idx_text)?;
                    match text {
                        Some(t) if !t.is_empty() => {
                            let bytes = bs58::decode(&t).into_vec()
                                .map_err(|_| rusqlite::Error::InvalidColumnName("bs58 decode failed".into()))?;
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Ok(Some(arr))
                        }
                        _ => Ok(None),
                    }
                }
            }
        }

        let mut caps = vec![];
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    let cap_id: String = row.get(0).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value: i64 = row.get(1).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let asset_id = bytes_to_asset_id(
                        read_blob32_text_fallback(&row, C_ASSET_BLOB, C_ASSET_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let spend_hook: Option<FuncId> = match read_opt_blob32_text_fallback(&row, C_SPEND_BLOB, C_SPEND_TEXT)
                        .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    {
                        Some(bytes) => Some(bytes_to_func_id(bytes)?),
                        None => None,
                    };
                    let user_data: Option<[u8; 32]> = read_opt_blob32_text_fallback(&row, C_USER_BLOB, C_USER_TEXT)
                        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let leaf_position: i64 = row.get(8).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let commitment = bytes_to_commitment(
                        read_blob32_text_fallback(&row, C_COMMIT_BLOB, C_COMMIT_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let cap_blind = bytes_to_base_blind(
                        read_blob32_text_fallback(&row, C_CAPBLIND_BLOB, C_CAPBLIND_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let value_blind = bytes_to_scalar_blind(
                        read_blob32_text_fallback(&row, C_VALBLIND_BLOB, C_VALBLIND_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let asset_blind = bytes_to_base_blind(
                        read_blob32_text_fallback(&row, C_ASSETBLIND_BLOB, C_ASSETBLIND_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let contract_id = match row.get::<_, Option<Vec<u8>>>(C_CONTRACT_BLOB) {
                        Ok(Some(v)) if v.len() == 32 => {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&v);
                            bytes_to_contract_id(arr)?
                        }
                        _ => {
                            tracing::warn!(
                                "CapRecord missing/invalid contract_id_blob — using ZERO sentinel. \
                                 Legacy rows must be re-scanned to populate contract_id."
                            );
                            ContractId::ZERO
                        }
                    };
                    let func_id: Option<FuncId> = match row.get::<_, Option<Vec<u8>>>(C_FUNC_BLOB) {
                        Ok(Some(v)) if v.len() == 32 => {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&v);
                            Some(bytes_to_func_id(arr)?)
                        }
                        _ => None,
                    };
                    let capability_discriminant: Option<i64> = row.get(19).ok().flatten();
                    let spent_val: i64 = row.get(20).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let revoked_at_height: Option<i64> = row.get(21).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let created_at_height: i64 = row.get(22).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let capability_name: Option<String> = row.get(23).ok().flatten();
                    let resource: Option<String> = row.get(24).ok().flatten();
                    let action: Option<String> = row.get(25).ok().flatten();
                    // A corrupt/unknown CSV degrades to an empty vec: typed
                    // composition metadata is non-load-bearing (display/typing
                    // only), so it is not surfaced as a read error.
                    let primitives = row.get::<_, Option<String>>(26).ok().flatten()
                        .and_then(|s| primitives_from_csv(&s)).unwrap_or_default();
                    let barbs = row.get::<_, Option<String>>(27).ok().flatten()
                        .and_then(|s| barbs_from_csv(&s)).unwrap_or_default();
                    // Column 28: key_coords_blob (nullable, not key material)
                    let key_coords: Option<dwow_accounts::KeyCoordinates> =
                        row.get::<_, Option<Vec<u8>>>(28).ok().flatten()
                            .and_then(|v| dwow_serial::deserialize(&v).ok());
                    // HAZOP WP-7: status columns (29=status TEXT, 30=status_height INTEGER)
                    let status_str: Option<String> = row.get(29).ok().flatten();
                    let status = status_str.as_deref()
                        .and_then(|s| crate::capability::CapStatus::from_str(s));
                    let status_height: Option<i64> = row.get(30).ok().flatten();

                    caps.push(CapRecord {
                        cap_id,
                        key_coords,
                        value: u64::try_from(value).unwrap_or(0),
                        asset_id,
                        spend_hook,
                        user_data,
                        leaf_position: u64::try_from(leaf_position).unwrap_or(0),
                        commitment,
                        contract_id,
                        func_id,
                        cap_blind,
                        value_blind,
                        asset_blind,
                        capability_discriminant: capability_discriminant.map(|d| d as u8),
                        revoked: spent_val != 0,
                        revoked_at_height: revoked_at_height.map(|h| u64::try_from(h).unwrap_or(0)),
                        status,
                        status_height: status_height.map(|h| u64::try_from(h).unwrap_or(0)),
                        created_at_height: u64::try_from(created_at_height).unwrap_or(0),
                        capability_name,
                        resource,
                        action,
                        primitives,
                        barbs,
                    });
                }
                Ok(None) => break,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            }
        }

        Ok(caps)
    }

    /// Get held capabilities for a specific asset ID (32-byte field element repr).
    pub fn get_capabilities_by_asset(&self, asset_id: &TokenId, revoked: Option<bool>) -> WalletDbResult<Vec<CapRecord>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT cap_id, value, asset_id_blob, asset_id, spend_hook_blob, spend_hook,
                    user_data_blob, user_data,
                    leaf_position, commitment_blob, commitment, cap_blind_blob, cap_blind,
                    value_blind_blob, value_blind, asset_blind_blob, asset_blind,
                    contract_id_blob, func_id_blob, capability_discriminant,
                    revoked, revoked_at_height, created_at_height,
                    capability_name, resource, action, primitives_csv, barbs_csv, key_coords_blob
             FROM held_capabilities WHERE asset_id_blob = ?1 AND revoked = ?2 AND contract_id_blob = ?3",
        )?;

        // Inflation guard: fee/coin selection is native-token only — foreign
        // capabilities are never spendable value (see capability_balance).
        let revoked_param: Option<i64> = revoked.map(|r| if r { 1 } else { 0 });
        let mut rows = stmt.query(params![
            asset_id.to_bytes().to_vec(),
            revoked_param,
            NATIVE_TOKEN_CONTRACT_ID.to_bytes().to_vec(),
        ])?;

        // Column index constants (same layout as get_held_capabilities SELECT)
        const C_ASSET_BLOB: usize = 2;
        const C_ASSET_TEXT: usize = 3;
        const C_SPEND_BLOB: usize = 4;
        const C_SPEND_TEXT: usize = 5;
        const C_USER_BLOB: usize = 6;
        const C_USER_TEXT: usize = 7;
        const C_COMMIT_BLOB: usize = 9;
        const C_COMMIT_TEXT: usize = 10;
        const C_CAPBLIND_BLOB: usize = 11;
        const C_CAPBLIND_TEXT: usize = 12;
        const C_VALBLIND_BLOB: usize = 13;
        const C_VALBLIND_TEXT: usize = 14;
        const C_ASSETBLIND_BLOB: usize = 15;
        const C_ASSETBLIND_TEXT: usize = 16;
        const C_CONTRACT_BLOB: usize = 17;
        const C_FUNC_BLOB: usize = 18;
        // Capability discriminant — referenced by external tooling
        #[allow(dead_code)]
        const C_DISCRIMINANT: usize = 19;

        /// Read a [u8; 32] from a BLOB column, falling back to bs58 decode from a TEXT column.
        fn read_blob32_text_fallback(
            row: &rusqlite::Row,
            idx_blob: usize,
            idx_text: usize,
        ) -> std::result::Result<[u8; 32], rusqlite::Error> {
            match row.get::<_, Vec<u8>>(idx_blob) {
                Ok(v) if v.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&v);
                    Ok(arr)
                }
                _ => {
                    let text: String = row.get(idx_text)?;
                    let bytes = bs58::decode(&text).into_vec()
                        .map_err(|_| rusqlite::Error::InvalidColumnName("bs58 decode failed".into()))?;
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    Ok(arr)
                }
            }
        }

        /// Read an Option<[u8; 32]> from nullable BLOB/TEXT columns.
        fn read_opt_blob32_text_fallback(
            row: &rusqlite::Row,
            idx_blob: usize,
            idx_text: usize,
        ) -> std::result::Result<Option<[u8; 32]>, rusqlite::Error> {
            match row.get::<_, Option<Vec<u8>>>(idx_blob) {
                Ok(Some(v)) if v.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&v);
                    Ok(Some(arr))
                }
                Ok(Some(_)) => {
                    let text: Option<String> = row.get(idx_text)?;
                    match text {
                        Some(t) if !t.is_empty() => {
                            let bytes = bs58::decode(&t).into_vec()
                                .map_err(|_| rusqlite::Error::InvalidColumnName("bs58 decode failed".into()))?;
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Ok(Some(arr))
                        }
                        _ => Ok(None),
                    }
                }
                _ => {
                    let text: Option<String> = row.get(idx_text)?;
                    match text {
                        Some(t) if !t.is_empty() => {
                            let bytes = bs58::decode(&t).into_vec()
                                .map_err(|_| rusqlite::Error::InvalidColumnName("bs58 decode failed".into()))?;
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            Ok(Some(arr))
                        }
                        _ => Ok(None),
                    }
                }
            }
        }

        let mut caps = vec![];
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    let cap_id: String = row.get(0).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let value: i64 = row.get(1).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let asset_id_val = bytes_to_asset_id(
                        read_blob32_text_fallback(&row, C_ASSET_BLOB, C_ASSET_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let spend_hook: Option<FuncId> = match read_opt_blob32_text_fallback(&row, C_SPEND_BLOB, C_SPEND_TEXT)
                        .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    {
                        Some(bytes) => Some(bytes_to_func_id(bytes)?),
                        None => None,
                    };
                    let user_data: Option<[u8; 32]> = read_opt_blob32_text_fallback(&row, C_USER_BLOB, C_USER_TEXT)
                        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let leaf_position: i64 = row.get(8).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let commitment = bytes_to_commitment(
                        read_blob32_text_fallback(&row, C_COMMIT_BLOB, C_COMMIT_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let cap_blind = bytes_to_base_blind(
                        read_blob32_text_fallback(&row, C_CAPBLIND_BLOB, C_CAPBLIND_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let value_blind = bytes_to_scalar_blind(
                        read_blob32_text_fallback(&row, C_VALBLIND_BLOB, C_VALBLIND_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let asset_blind = bytes_to_base_blind(
                        read_blob32_text_fallback(&row, C_ASSETBLIND_BLOB, C_ASSETBLIND_TEXT)
                            .map_err(|_| WalletDbError::QueryExecutionFailed)?
                    )?;
                    let contract_id = match row.get::<_, Option<Vec<u8>>>(C_CONTRACT_BLOB) {
                        Ok(Some(v)) if v.len() == 32 => {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&v);
                            bytes_to_contract_id(arr)?
                        }
                        _ => {
                            tracing::warn!(
                                "CapRecord missing/invalid contract_id_blob — using ZERO sentinel. \
                                 Legacy rows must be re-scanned to populate contract_id."
                            );
                            ContractId::ZERO
                        }
                    };
                    let func_id: Option<FuncId> = match row.get::<_, Option<Vec<u8>>>(C_FUNC_BLOB) {
                        Ok(Some(v)) if v.len() == 32 => {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&v);
                            Some(bytes_to_func_id(arr)?)
                        }
                        _ => None,
                    };
                    let capability_discriminant: Option<i64> = row.get(19).ok().flatten();
                    let spent_val: i64 = row.get(20).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let revoked_at_height: Option<i64> = row.get(21).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let created_at_height: i64 = row.get(22).map_err(|_| WalletDbError::QueryExecutionFailed)?;
                    let capability_name: Option<String> = row.get(23).ok().flatten();
                    let resource: Option<String> = row.get(24).ok().flatten();
                    let action: Option<String> = row.get(25).ok().flatten();
                    // A corrupt/unknown CSV degrades to an empty vec: typed
                    // composition metadata is non-load-bearing (display/typing
                    // only), so it is not surfaced as a read error.
                    let primitives = row.get::<_, Option<String>>(26).ok().flatten()
                        .and_then(|s| primitives_from_csv(&s)).unwrap_or_default();
                    let barbs = row.get::<_, Option<String>>(27).ok().flatten()
                        .and_then(|s| barbs_from_csv(&s)).unwrap_or_default();
                    // Column 28: key_coords_blob (nullable, not key material)
                    let key_coords: Option<dwow_accounts::KeyCoordinates> =
                        row.get::<_, Option<Vec<u8>>>(28).ok().flatten()
                            .and_then(|v| dwow_serial::deserialize(&v).ok());

                    caps.push(CapRecord {
                        cap_id,
                        key_coords,
                        value: u64::try_from(value).unwrap_or(0),
                        asset_id: asset_id_val,
                        spend_hook,
                        user_data,
                        leaf_position: u64::try_from(leaf_position).unwrap_or(0),
                        commitment,
                        contract_id,
                        func_id,
                        cap_blind,
                        value_blind,
                        asset_blind,
                        capability_discriminant: capability_discriminant.map(|d| d as u8),
                        revoked: spent_val != 0,
                        revoked_at_height: revoked_at_height.map(|h| u64::try_from(h).unwrap_or(0)),
                        status: None,
                        status_height: None,
                        created_at_height: u64::try_from(created_at_height).unwrap_or(0),
                        capability_name,
                        resource,
                        action,
                        primitives,
                        barbs,
                    });
                }
                Ok(None) => break,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            }
        }

        Ok(caps)
    }

    /// Mark a held capability as revoked (nullifier published on-chain).
    /// HAZOP WP-7: also sets status = 'processing' for the confirmation lifecycle.
    pub fn mark_revoked(&self, cap_id: &str, block_height: u64) -> WalletDbResult<()> {
        
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "UPDATE held_capabilities SET revoked = 1, revoked_at_height = ?1, status = 'processing', status_height = ?1 WHERE cap_id = ?2",
            params![i64::try_from(block_height).unwrap_or(i64::MAX), cap_id],
        )
        .map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// HAZOP WP-7: set capability lifecycle status with transition validation.
    /// Allowed transitions:
    ///   NULL → Pending (broadcast)
    ///   Pending → NULL (timeout/expiry)
    ///   NULL/Pending → Processing (scan detects nullifier)
    ///   Processing → Spent (maturity reached)
    /// All other transitions return an error.
    pub fn set_cap_status(&self, cap_id: &str, new_status: crate::capability::CapStatus, height: u64) -> WalletDbResult<()> {
        use crate::capability::CapStatus;
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;

        // Read current status to validate transition
        let current: Option<String> = conn.query_row(
            "SELECT status FROM held_capabilities WHERE cap_id = ?1",
            params![cap_id],
            |row| row.get(0),
        ).ok().flatten();

        let current_status = current.as_deref().and_then(|s| CapStatus::from_str(s));

        // Transition validation
        let valid = match (current_status, new_status) {
            (None, CapStatus::Pending) => true,           // broadcast
            (Some(CapStatus::Pending), _) if new_status == CapStatus::Pending => false, // already pending
            (Some(CapStatus::Pending), CapStatus::Processing) => true, // mined
            (Some(CapStatus::Processing), CapStatus::Spent) => true,  // matured
            (_, CapStatus::Pending) if current_status.is_some() => false, // can't pend non-null
            (Some(CapStatus::Processing), _) if new_status != CapStatus::Spent => false,
            (Some(CapStatus::Spent), _) => false,         // terminal
            _ => false,
        };

        if !valid {
            return Err(WalletDbError::QueryExecutionFailed);
        }

        // Also update revoked for Processing/Spent states
        let revoked_val: Option<i64> = match new_status {
            CapStatus::Processing | CapStatus::Spent => Some(1),
            _ => None,
        };

        if let Some(r) = revoked_val {
            conn.execute(
                "UPDATE held_capabilities SET status = ?1, status_height = ?2, revoked = ?3, revoked_at_height = ?2 WHERE cap_id = ?4",
                params![new_status.as_str(), i64::try_from(height).unwrap_or(i64::MAX), r, cap_id],
            ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        } else {
            conn.execute(
                "UPDATE held_capabilities SET status = ?1, status_height = ?2, revoked = 0, revoked_at_height = NULL WHERE cap_id = ?3",
                params![new_status.as_str(), i64::try_from(height).unwrap_or(i64::MAX), cap_id],
            ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        }
        Ok(())
    }

    /// HAZOP WP-7: clear capability status (return to spendable/unspent).
    pub fn clear_cap_status(&self, cap_id: &str) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "UPDATE held_capabilities SET status = NULL, status_height = NULL WHERE cap_id = ?1",
            params![cap_id],
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
                "INSERT OR IGNORE INTO held_capabilities (cap_id, value, asset_id_blob, spend_hook_blob, user_data_blob,
                    leaf_position, commitment_blob, contract_id_blob, func_id_blob, capability_discriminant,
                    cap_blind_blob, value_blind_blob, asset_blind_blob,
                    revoked, revoked_at_height, created_at_height,
                    capability_name, resource, action, primitives_csv, barbs_csv, key_coords_blob,
                    status, status_height)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
                params![
                    cap.cap_id,
                    i64::try_from(cap.value).unwrap_or(i64::MAX),
                    cap.asset_id.to_bytes().to_vec(),
                    cap.spend_hook.map(|f| f.to_bytes().to_vec()),
                    cap.user_data.map(|b| b.to_vec()),
                    i64::try_from(cap.leaf_position).unwrap_or(i64::MAX),
                    cap.commitment.to_bytes().to_vec(),
                    cap.contract_id.to_bytes().to_vec(),
                    cap.func_id.map(|f| f.to_bytes().to_vec()),
                    cap.capability_discriminant.map(|d| d as i64),
                    cap.cap_blind.inner().to_repr().to_vec(),
                    cap.value_blind.inner().to_repr().to_vec(),
                    cap.asset_blind.inner().to_repr().to_vec(),
                    if cap.revoked { 1 } else { 0 },
                    cap.revoked_at_height.map(|h| i64::try_from(h).unwrap_or(i64::MAX)),
                    i64::try_from(cap.created_at_height).unwrap_or(i64::MAX),
                    cap.capability_name,
                    cap.resource,
                    cap.action,
                    primitives_to_csv(&cap.primitives),
                    barbs_to_csv(&cap.barbs),
                    cap.key_coords.as_ref().map(|k| dwow_serial::serialize(k)),
                    cap.status.as_ref().map(|s| s.as_str().to_string()),
                    cap.status_height.map(|h| i64::try_from(h).unwrap_or(i64::MAX)),
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
            if let Err(e) = conn.execute("ROLLBACK", []) {
                tracing::error!("SQLite ROLLBACK failed: {e} — database may be in inconsistent state");
            }
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
    pub fn remove_capabilities_after(&self, height: u64) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let height = i64::try_from(height).unwrap_or(i64::MAX);

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

    /// HAZOP C2 fix: reorg retention — clear both revoked AND status/status_height
    /// for caps exercised at heights above the reorg target. Previously only
    /// cleared revoked, leaving status='processing' — a dual-write inconsistency.
    pub fn retain_capabilities_after(&self, height: u64) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let height = i64::try_from(height).unwrap_or(i64::MAX);
        conn.execute(
            "UPDATE held_capabilities SET revoked = 0, revoked_at_height = NULL,
                status = NULL, status_height = NULL
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
    pub deploy_height: u64,
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
                i64::try_from(record.deploy_height).unwrap_or(i64::MAX),
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
            deploy_height: u64::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
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
                deploy_height: u64::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
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
                deploy_height: u64::try_from(row.get::<_, i64>(7)?).unwrap_or(0),
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

    // ── ZK circuit binary store (wallet.md §3, §6.4.1 step 3) ─────

    /// Store a zkas binary for a deployed contract's circuit.
    /// Keyed by (contract_id bs58, namespace, circuit_name).
    /// Genesis circuits are embedded at wallet init via
    /// `dwow_native_token_contract` compile-time binaries.
    pub fn store_zkas_binary(
        &self,
        contract_id: &str,
        namespace: &str,
        circuit_name: &str,
        zkas_bytes: &[u8],
    ) -> WalletDbResult<()> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        conn.execute(
            "INSERT OR REPLACE INTO zkas_binaries (contract_id, namespace, circuit_name, zkas_bytes) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![contract_id, namespace, circuit_name, zkas_bytes],
        ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    /// Load a zkas binary for a contract's circuit.
    pub fn load_zkas_binary(
        &self,
        contract_id: &str,
        namespace: &str,
        circuit_name: &str,
    ) -> WalletDbResult<Option<Vec<u8>>> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT zkas_bytes FROM zkas_binaries WHERE contract_id = ?1 AND namespace = ?2 AND circuit_name = ?3",
        )?;
        let mut rows = stmt.query(rusqlite::params![contract_id, namespace, circuit_name])?;
        match rows.next()? {
            Some(row) => {
                let bytes: Vec<u8> = row.get(0)?;
                Ok(Some(bytes))
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

    /// Load a stored Merkle tree by name.
    ///
    /// Returns `None` when the tree does not exist (first run / fresh state).
    /// Corrupt tree data (checksum or deserialization failure) is logged but also
    /// returns `None` — the caller creates a fresh tree. The log ensures the
    /// corruption is visible rather than silently replaced.
    pub fn get_merkle_tree(&self, name: &[u8]) -> Option<MerkleTree> {
        let conn = self.conn.lock().ok()?;
        let tree_bytes: Vec<u8> = conn.query_row(
            "SELECT tree_blob FROM merkle_trees WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        ).ok()?;
        let raw = match crate::sled_checksum::checksum_decode(&tree_bytes) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(target: "dww::walletdb",
                    "merkle tree '{:?}' checksum failed: {} — creating fresh tree",
                    std::str::from_utf8(name).unwrap_or("?"), e);
                return None;
            }
        };
        match deserialize(&raw) {
            Ok(tree) => Some(tree),
            Err(e) => {
                tracing::error!(target: "dww::walletdb",
                    "merkle tree '{:?}' deserialization failed: {} — creating fresh tree",
                    std::str::from_utf8(name).unwrap_or("?"), e);
                None
            }
        }
    }

    pub fn insert_scanned_block(
        &self, height: &u64, hash: &HeaderHash, signing_key: &Option<SecretKey>,
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

    pub fn get_scanned_block(&self, height: &u64) -> WalletDbResult<(String, String)> {
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

    pub fn get_scanned_block_records(&self) -> WalletDbResult<Vec<(u64, String, String)>> {
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

    pub fn get_last_scanned_block(&self) -> WalletDbResult<(u64, String)> {
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

    pub fn delete_scanned_blocks_above(&self, height: u64) -> WalletDbResult<()> {
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
            params![i64::try_from(height).unwrap_or(i64::MAX), block_json],
        ).map_err(|_| WalletDbError::QueryExecutionFailed)?;
        Ok(())
    }

    pub fn get_block(&self, height: u64) -> WalletDbResult<dwow_chain::Block> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let mut stmt = conn.prepare(
            "SELECT block_json FROM chain_blocks WHERE height = ?1",
        )?;
        stmt.query_row(params![i64::try_from(height).unwrap_or(i64::MAX)], |row| {
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

    pub fn chain_height(&self) -> WalletDbResult<dwow_sdk::blockchain::BlockHeight> {
        let conn = self.conn.lock().map_err(|_| WalletDbError::FailedToAquireLock)?;
        let height: i64 = conn.query_row(
            "SELECT COALESCE(MAX(height), 0) FROM chain_blocks",
            [],
            |row| row.get(0),
        )?;
        dwow_sdk::blockchain::BlockHeight::from_sqlite_i64(height)
            .ok_or_else(|| {
                tracing::error!(
                    "chain_height: corrupt DB — negative height {} in chain_blocks",
                    height
                );
                WalletDbError::QueryExecutionFailed
            })
    }
}

/// TokenInfo struct REMOVED — dead code (tokens table was removed; token
/// knowledge comes from capabilities).

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
    use dwow_sdk::crypto::Blind;
    use dwow_sdk::pasta::pallas;

    use super::*;
    use crate::walletdb::WalletDb;

    // test_mem_wallet REMOVED — tested query_single/query_custom (both dead).
    // test_query_single REMOVED — tested dead query_single/query_custom.
    // test_query_multi REMOVED — tested dead query_multiple/query_custom.

    // test_insert_and_get_capabilities REMOVED — tested get_capabilities (dead).

    // test_address_stores_secret REMOVED — the addresses table / key store is gone;
    // the wallet derives its identity on boot via AccountManager. Key-path coverage
    // now lives in the dwow-accounts determinism tests + the full-path integration test.

    fn setup_test_wallet() -> WalletPtr {
        let wallet = WalletDb::new(None, None, false).unwrap();
        wallet.exec_batch_sql(include_str!("../wallet.sql")).unwrap();
        wallet
    }

    fn make_test_cap() -> CapRecord {
        CapRecord {
            cap_id: "test_cap_1".to_string(),
            value: 1,
            asset_id: TokenId::from_bytes([1u8; 32]).unwrap(),
            spend_hook: Some(FuncId::from_bytes([2u8; 32]).unwrap()),
            user_data: Some([3u8; 32]),
            leaf_position: 0,
            commitment: CoinCommitment::from_bytes([4u8; 32]).unwrap(),
            contract_id: ContractId::from_bytes([5u8; 32]).unwrap(),
            func_id: Some(FuncId::from_bytes([6u8; 32]).unwrap()),
            capability_discriminant: Some(7u8),
            cap_blind: Blind(pallas::Base::from(8u64)),
            value_blind: Blind(pallas::Scalar::from(9u64)),
            asset_blind: Blind(pallas::Base::from(10u64)),
            revoked: false,
            revoked_at_height: None,
            created_at_height: 1,
            capability_name: None,
            resource: None,
            action: None,
            primitives: vec![],
            barbs: vec![],
            status: None, status_height: None, key_coords: None,
        }
    }

    #[test]
    fn test_cap_record_insert_and_read() {
        let wallet = setup_test_wallet();
        let cap = make_test_cap();
        let proof = MerkleProof {
            siblings: vec![],
            root: "11111111111111111111111111111111".to_string(),
        };

        wallet.insert_capability(&cap, &proof).unwrap();

        let caps = wallet.get_held_capabilities(None).unwrap();
        assert_eq!(caps.len(), 1);
        let read = &caps[0];

        assert_eq!(read.cap_id, cap.cap_id);
        assert_eq!(read.value, cap.value);
        assert_eq!(read.asset_id, cap.asset_id);
        assert_eq!(read.commitment, cap.commitment);
        assert_eq!(read.contract_id, cap.contract_id);
        assert_eq!(read.func_id, cap.func_id);
        assert_eq!(read.capability_discriminant, cap.capability_discriminant);
        assert_eq!(read.cap_blind, cap.cap_blind);
        assert_eq!(read.value_blind, cap.value_blind);
        assert_eq!(read.asset_blind, cap.asset_blind);
        assert_eq!(read.spend_hook, cap.spend_hook);
        assert_eq!(read.revoked, false);
        assert_eq!(read.created_at_height, 1);
    }

    #[test]
    fn test_cap_record_discriminant_roundtrip() {
        let wallet = setup_test_wallet();
        let mut cap = make_test_cap();
        cap.capability_discriminant = Some(42);
        let proof = MerkleProof {
            siblings: vec![],
            root: "11111111111111111111111111111111".to_string(),
        };

        wallet.insert_capability(&cap, &proof).unwrap();
        let caps = wallet.get_held_capabilities(None).unwrap();
        assert_eq!(caps[0].capability_discriminant, Some(42));
    }

    #[test]
    fn test_get_capabilities_by_asset_filter() {
        let wallet = setup_test_wallet();
        let proof = MerkleProof {
            siblings: vec![],
            root: "11111111111111111111111111111111".to_string(),
        };

        // Insert cap for asset A
        let mut cap_a = make_test_cap();
        cap_a.cap_id = "cap_a".to_string();
        cap_a.asset_id = TokenId::from_bytes([1u8; 32]).unwrap();
        wallet.insert_capability(&cap_a, &proof).unwrap();

        // Insert cap for asset B
        let mut cap_b = make_test_cap();
        cap_b.cap_id = "cap_b".to_string();
        cap_b.asset_id = TokenId::from_bytes([2u8; 32]).unwrap();
        wallet.insert_capability(&cap_b, &proof).unwrap();

        // Both caps should be stored and retrievable
        let all_caps = wallet.get_held_capabilities(Some(false)).unwrap();
        assert_eq!(all_caps.len(), 2, "both caps must be stored");
        assert!(all_caps.iter().any(|c| c.cap_id == "cap_a"));
        assert!(all_caps.iter().any(|c| c.cap_id == "cap_b"));
    }

    #[test]
    fn test_get_capabilities_by_asset_excludes_non_native() {
        // Inflation guard: a foreign-contract cap with the SAME asset_id must not
        // be returned for value/fee selection — only native-token caps are spendable.
        let wallet = setup_test_wallet();
        let proof = MerkleProof {
            siblings: vec![],
            root: "11111111111111111111111111111111".to_string(),
        };
        let asset = TokenId::from_bytes([1u8; 32]).unwrap();

        let mut native = make_test_cap();
        native.cap_id = "native".to_string();
        native.asset_id = asset;
        native.contract_id = *NATIVE_TOKEN_CONTRACT_ID;
        wallet.insert_capability(&native, &proof).unwrap();

        let mut foreign = make_test_cap();
        foreign.cap_id = "foreign".to_string();
        foreign.asset_id = asset; // SAME asset_id
        foreign.commitment = CoinCommitment::from_bytes([8u8; 32]).unwrap();
        foreign.contract_id = ContractId::from_bytes([9u8; 32]).unwrap();
        wallet.insert_capability(&foreign, &proof).unwrap();

        // Both are stored (get_held_capabilities is intentionally ungated)...
        assert_eq!(wallet.get_held_capabilities(Some(false)).unwrap().len(), 2);
        // ...but only the native one is selectable for value.
        let selectable = wallet.get_capabilities_by_asset(&asset, Some(false)).unwrap();
        assert_eq!(selectable.len(), 1, "foreign cap must be excluded from selection");
        assert_eq!(selectable[0].cap_id, "native");
        assert_eq!(selectable[0].contract_id, *NATIVE_TOKEN_CONTRACT_ID);
    }

    #[test]
    fn test_cap_record_typed_composition_roundtrip() {
        use dwow_sdk::capability::{Barb, Primitive};
        let wallet = setup_test_wallet();
        let proof = MerkleProof {
            siblings: vec![],
            root: "11111111111111111111111111111111".to_string(),
        };
        let mut cap = make_test_cap();
        cap.capability_name = Some("coin".to_string());
        cap.resource = Some("coin".to_string());
        cap.action = Some("transfer".to_string());
        cap.primitives = vec![Primitive::SecretKey, Primitive::Commitment, Primitive::Nullifier];
        cap.barbs = vec![Barb::Spend, Barb::Commit, Barb::Nullify];
        wallet.insert_capability(&cap, &proof).unwrap();

        let read = &wallet.get_held_capabilities(None).unwrap()[0];
        assert_eq!(read.capability_name.as_deref(), Some("coin"));
        assert_eq!(read.resource.as_deref(), Some("coin"));
        assert_eq!(read.action.as_deref(), Some("transfer"));
        assert_eq!(read.primitives, cap.primitives);
        assert_eq!(read.barbs, cap.barbs);
    }

    #[test]
    fn test_cap_record_revoked_filter() {
        let wallet = setup_test_wallet();
        let proof = MerkleProof {
            siblings: vec![],
            root: "11111111111111111111111111111111".to_string(),
        };

        let mut cap_active = make_test_cap();
        cap_active.cap_id = "active".to_string();
        wallet.insert_capability(&cap_active, &proof).unwrap();

        let mut cap_revoked = make_test_cap();
        cap_revoked.cap_id = "revoked".to_string();
        cap_revoked.revoked = true;
        wallet.insert_capability(&cap_revoked, &proof).unwrap();

        // All caps
        assert_eq!(wallet.get_held_capabilities(None).unwrap().len(), 2);
        // Active only
        assert_eq!(wallet.get_held_capabilities(Some(false)).unwrap().len(), 1);
        // Revoked only
        assert_eq!(wallet.get_held_capabilities(Some(true)).unwrap().len(), 1);
    }

    /// P12 — INSERT OR IGNORE idempotence and manifest atomic storage.
    ///
    /// Verifies that inserting the same capability twice does not produce
    /// duplicate rows, and that `insert_contract_metadata_with_manifest`
    /// stores both metadata and manifest JSON in a single operation.
    #[test]
    fn test_insert_idempotence_and_manifest_atomicity() {
        use crate::walletdb::{WalletDb, WalletPtr};
        use dwow_chain::CoinCommitment;

        let wallet = WalletDb::new(None, None, false).expect("in-memory WalletDb");
        wallet.exec_batch_sql(include_str!("../wallet.sql")).ok();

        // Build a minimal CapRecord
        let cap_id = "test_cap_idempotent_01";
        let record = super::CapRecord {
            cap_id: cap_id.to_string(), value: 100,
            asset_id: dwow_sdk::crypto::TokenId::DRKW,
            spend_hook: None, user_data: None,
            leaf_position: 0,
            commitment: CoinCommitment::from_base(dwow_sdk::pasta::pallas::Base::from(42)),
            contract_id: *NATIVE_TOKEN_CONTRACT_ID, func_id: None,
            cap_blind: dwow_sdk::crypto::Blind(dwow_sdk::pasta::pallas::Base::zero()),
            value_blind: dwow_sdk::crypto::Blind(dwow_sdk::pasta::pallas::Scalar::zero()),
            asset_blind: dwow_sdk::crypto::Blind(dwow_sdk::pasta::pallas::Base::zero()),
            capability_discriminant: None, capability_name: None,
            resource: None, action: None, primitives: vec![], barbs: vec![],
            revoked: false, revoked_at_height: None,
            created_at_height: 1, status: None, status_height: None, key_coords: None,
        };
        let proof = super::MerkleProof { root: String::new(), siblings: vec![] };

        // Insert once — should succeed
        wallet.insert_capability(&record, &proof)
            .expect("P12: first insert must succeed");
        assert_eq!(wallet.get_held_capabilities(None).unwrap().len(), 1,
            "P12: one cap after first insert");

        // Insert again with the SAME cap_id — should not duplicate
        wallet.insert_capability(&record, &proof)
            .expect("P12: second insert must succeed (INSERT OR IGNORE)");
        assert_eq!(wallet.get_held_capabilities(None).unwrap().len(), 1,
            "P12: still one cap after duplicate insert (idempotent)");

        // ── Manifest atomicity ────────────────────────────────
        let cid_str = "test_atomic_manifest_cid";
        let manifest_json = r#"{"name":"atomic_test","category":"Testing","version":"1.0.0","description":"atomic","dependencies":[],"functions":[],"capabilities":[],"actions":[],"trees":[],"circuits":[]}"#;
        let record = super::ContractMetadataRecord {
            contract_id: cid_str.to_string(), name: "atomic_test".into(),
            symbol: None, category: "Testing".into(),
            description: Some("atomic test".into()), public: true,
            deployer_pubkey: String::new(), deploy_height: 1,
            attestations_json: "[]".into(), lock_status: "unlocked".into(),
        };
        wallet.insert_contract_metadata_with_manifest(&record, Some(manifest_json))
            .expect("P12: insert metadata+manifest must succeed");

        // Verify manifest is retrievable
        let stored = wallet.get_contract_manifest(cid_str)
            .expect("P12: get_contract_manifest must succeed");
        assert!(stored.is_some(), "P12: manifest must be retrievable after insert");
        let m = stored.unwrap();
        assert_eq!(m.name, "atomic_test",
            "P12: stored manifest name must match");
    }
}
