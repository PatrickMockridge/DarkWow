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

//! Wallet cache — SQLite-backed scan state.
//!
//! Formerly used sled trees. Consolidated into the wallet SQLite database
//! (2026-07-02) to eliminate the dual-database anti-pattern. The wallet now
//! has TWO databases: chain sled (blocks) + wallet SQLite (everything else).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::wallet_error::{Error, Result};
use dwow_core::blockchain::HeaderHash;
use dwow_sdk::{
    crypto::{
        pasta_prelude::PrimeField,
        smt::{PoseidonFp, SparseMerkleTree, StorageAdapter, SMT_FP_DEPTH},
        MerkleTree, SecretKey,
    },
    error::{ContractError, ContractResult},
    pasta::pallas,
};
use dwow_serial::{deserialize, serialize};
use num_bigint::BigUint;
use rusqlite;
use tracing::error;

/// SQLite-backed wallet cache. Replaces the sled cache DB.
/// All scan state (scanned blocks, merkle trees, nullifier SMT)
/// lives in the wallet SQLite database.
/// Uses Arc<Mutex<>> for thread-safe sharing across async tasks.
#[derive(Clone)]
pub struct Cache {
    pub conn: Arc<Mutex<rusqlite::Connection>>,
}

impl Cache {
    /// Create cache backed by a SQLite connection (already wrapped in Arc<Mutex<>>).
    pub fn new(conn: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { conn }
    }

    fn lock(&self) -> std::sync::MutexGuard<rusqlite::Connection> {
        self.conn.lock().unwrap()
    }

    /// Insert merkle tree checkpoints (replaces sled batch insert).
    pub fn insert_merkle_trees(&self, trees: &[(&[u8], &MerkleTree)]) -> Result<()> {
        let conn = self.lock();
        for (key, tree) in trees {
            let raw = serialize(*tree);
            let checked = crate::sled_checksum::checksum_encode(&raw);
            conn.execute(
                "INSERT OR REPLACE INTO merkle_trees (name, tree_blob) VALUES (?1, ?2)",
                rusqlite::params![key, checked],
            ).map_err(|e| Error::Custom(format!("insert_merkle_trees: {e}")))?;
        }
        Ok(())
    }

    /// Get a Merkle tree by name from the cache.
    pub fn get_merkle_tree(&self, name: &[u8]) -> Option<MerkleTree> {
        let conn = self.lock();
        let tree_bytes: Vec<u8> = conn.query_row(
            "SELECT tree_blob FROM merkle_trees WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        ).ok()?;
        let raw = crate::sled_checksum::checksum_decode(&tree_bytes).ok()?;
        deserialize(&raw).ok()
    }
}

/// Block scanner that writes scanned block records to SQLite.
pub struct BlockScanner {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl BlockScanner {
    pub fn new(cache: &Cache) -> Self {
        Self { conn: cache.conn.clone() }
    }

    /// Insert a scanned block record into the scanned_blocks table.
    pub fn insert_scanned_block(
        &self,
        height: &u32,
        hash: &HeaderHash,
        signing_key: &Option<SecretKey>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let hash_str = hash.to_string();
        let key_str = match signing_key {
            Some(key) => key.to_string(),
            None => String::from("-"),
        };
        conn.execute(
            "INSERT OR REPLACE INTO scanned_blocks (height, hash, signing_key) VALUES (?1, ?2, ?3)",
            rusqlite::params![height, hash_str, key_str],
        ).map_err(|e| Error::Custom(format!("insert_scanned_block: {e}")))?;
        Ok(())
    }
}

pub type CacheSmt = SparseMerkleTree<
    'static,
    SMT_FP_DEPTH,
    { SMT_FP_DEPTH + 1 },
    pallas::Base,
    PoseidonFp,
    PnSmtStorage,
>;

/// Sparse Merkle Tree storage backed by SQLite.
/// Uses Arc<Mutex<>> for compatibility with SMT's StorageAdapter trait
/// (put/del take &mut self, get takes &self).
pub struct PnSmtStorage {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl PnSmtStorage {
    pub fn new(conn: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { conn }
    }

    /// Snapshot the entire SMT into a HashMap.
    pub fn snapshot(&self) -> Result<HashMap<BigUint, pallas::Base>> {
        let conn = self.conn.lock().unwrap();
        let mut smt = HashMap::new();
        let mut stmt = conn.prepare("SELECT key, value FROM nullifier_smt")
            .map_err(|e| Error::Custom(format!("snapshot prepare: {e}")))?;
        let rows = stmt.query_map([], |row| {
            let key: Vec<u8> = row.get(0)?;
            let value: Vec<u8> = row.get(1)?;
            Ok((key, value))
        }).map_err(|e| Error::Custom(format!("snapshot query: {e}")))?;

        for row in rows {
            let (key, value) = row.map_err(|e| Error::Custom(format!("snapshot row: {e}")))?;
            let raw = crate::sled_checksum::checksum_decode(&value)
                .map_err(|e| Error::Custom(
                    format!("[cache::PnSmtStorage::snapshot] Checksum failed: {e}")
                ))?;
            let mut repr = [0; 32];
            repr.copy_from_slice(&raw);
            let Some(value) = pallas::Base::from_repr(repr).into() else {
                return Err(Error::ParseFailed(
                    "[cache::PnSmtStorage::snapshot] Value conversion failed",
                ))
            };
            smt.insert(BigUint::from_bytes_le(&key), value);
        }
        Ok(smt)
    }

    /// Clear all entries (used by reset_scanned_blocks).
    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM nullifier_smt", [])
            .map_err(|e| Error::Custom(format!("nullifier_smt clear: {e}")))?;
        Ok(())
    }
}

impl StorageAdapter for PnSmtStorage {
    type Value = pallas::Base;

    fn put(&mut self, key: BigUint, value: pallas::Base) -> ContractResult {
        let conn = self.conn.lock().unwrap();
        let checked = crate::sled_checksum::checksum_encode(&value.to_repr());
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO nullifier_smt (key, value) VALUES (?1, ?2)",
            rusqlite::params![key.to_bytes_le(), checked],
        ) {
            error!(target: "cache::StorageAdapter::put",
                "Inserting key {key:?}, value {value:?} into DB failed: {e}");
            return Err(ContractError::SmtPutFailed)
        }
        Ok(())
    }

    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let conn = self.conn.lock().unwrap();
        let value: Vec<u8> = match conn.query_row(
            "SELECT value FROM nullifier_smt WHERE key = ?1",
            rusqlite::params![key.to_bytes_le()],
            |row| row.get(0),
        ) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return None,
            Err(e) => {
                error!(target: "cache::StorageAdapter::get",
                    "Fetching key {key:?} from DB failed: {e}");
                return None
            }
        };

        let raw = match crate::sled_checksum::checksum_decode(&value) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "cache::StorageAdapter::get",
                    "Checksum failed for key {key:?}: {e}");
                return None;
            }
        };

        let mut repr = [0; 32];
        repr.copy_from_slice(&raw);

        pallas::Base::from_repr(repr).into()
    }

    fn del(&mut self, key: &BigUint) -> ContractResult {
        let conn = self.conn.lock().unwrap();
        if let Err(e) = conn.execute(
            "DELETE FROM nullifier_smt WHERE key = ?1",
            rusqlite::params![key.to_bytes_le()],
        ) {
            error!(target: "cache::StorageAdapter::del",
                "Removing key {key:?} from DB failed: {e}");
            return Err(ContractError::SmtDelFailed)
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::wallet_error::Result;
    use dwow_core::zk::halo2::Field;
    use dwow_sdk::{
        crypto::smt::{gen_empty_nodes, util::FieldHasher, PoseidonFp, SparseMerkleTree},
        pasta::pallas,
    };
    use rand::rngs::OsRng;
    use rusqlite;

    use crate::cache::{Cache, PnSmtStorage};

    fn test_cache() -> Cache {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS merkle_trees (
                name TEXT PRIMARY KEY, tree_blob BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS nullifier_smt (
                key BLOB PRIMARY KEY, value BLOB NOT NULL
            ) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS scanned_blocks (
                height INTEGER PRIMARY KEY, hash TEXT NOT NULL, signing_key TEXT NOT NULL
            );
        ").unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        Cache::new(conn)
    }

    #[test]
    fn test_cache_smt() -> Result<()> {
        let cache = test_cache();

        const HEIGHT: usize = 3;
        let hasher = PoseidonFp::new();
        let empty_leaf = pallas::Base::ZERO;
        let empty_nodes = gen_empty_nodes::<{ HEIGHT + 1 }, _, _>(&hasher, empty_leaf);
        let store = PnSmtStorage::new(cache.conn.clone());
        let mut smt = SparseMerkleTree::<HEIGHT, { HEIGHT + 1 }, _, _, _>::new(
            store,
            hasher.clone(),
            &empty_nodes,
        );

        // Verify database is empty
        let count: i64 = cache.conn.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM nullifier_smt", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 0);

        let leaves = vec![
            (pallas::Base::from(1), pallas::Base::random(&mut OsRng)),
            (pallas::Base::from(2), pallas::Base::random(&mut OsRng)),
            (pallas::Base::from(3), pallas::Base::random(&mut OsRng)),
        ];
        smt.insert_batch(leaves.clone()).unwrap();

        let hash1 = leaves[0].1;
        let hash2 = leaves[1].1;
        let hash3 = leaves[2].1;

        let hash = |l, r| hasher.hash([l, r]);

        let hash01 = hash(empty_nodes[3], hash1);
        let hash23 = hash(hash2, hash3);

        let hash0123 = hash(hash01, hash23);
        let root = hash(hash0123, empty_nodes[1]);
        assert_eq!(root, smt.root());

        // Now try to construct a membership proof for leaf 3
        let pos = leaves[2].0;
        let path = smt.prove_membership(&pos);
        assert_eq!(path.path[0], empty_nodes[1]);
        assert_eq!(path.path[1], hash01);
        assert_eq!(path.path[2], hash2);

        assert_eq!(hash23, hash(path.path[2], hash3));
        assert_eq!(hash0123, hash(path.path[1], hash(path.path[2], hash3)));
        assert_eq!(root, hash(hash(path.path[1], hash(path.path[2], hash3)), path.path[0]));

        assert!(path.verify(&root, &hash3, &pos));

        // Verify database contains keys
        let count: i64 = cache.conn.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM nullifier_smt", [], |row| row.get(0),
        ).unwrap();
        assert!(count > 0);

        Ok(())
    }
}
