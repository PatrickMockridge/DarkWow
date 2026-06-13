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

use std::collections::HashMap;

use dwow_core::{blockchain::HeaderHash, Error, Result};
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
use sled;
use tracing::error;

pub const SLED_SCANNED_BLOCKS_TREE: &[u8] = b"_scanned_blocks";
pub const SLED_MERKLE_TREES_TREE: &[u8] = b"_merkle_trees";
pub const SLED_NULLIFIER_SMT_TREE: &[u8] = b"_nullifier_smt";
pub const SLED_BB_SMT_TREE: &[u8] = b"_bb_smt";

/// Structure holding all sled trees that define the blockchain cache.
/// Uses plain sled — no overlay/diff mechanism. DarkWow's linear
/// architecture is forward-only and deterministic; rollback is handled
/// by re-scanning from the target height.
#[derive(Clone)]
pub struct Cache {
    /// Main pointer to the sled db connection
    pub db: sled::Db,
    /// The `sled` tree storing the scanned blocks from the blockchain,
    /// where the key is the height number, and the value is the blocks'
    /// hash.
    pub scanned_blocks: sled::Tree,
    /// The `sled` tree storing the merkle trees of the blockchain,
    /// where the key is the tree name, and the value is the serialized
    /// merkle tree itself.
    pub merkle_trees: sled::Tree,
    /// The `sled` tree storing the Sparse Merkle Tree of the Money
    /// contract.
    pub nullifier_smt: sled::Tree,
    /// The `sled` tree storing the Sparse Merkle Tree of the Bearer
    /// Bond contract.
    pub bb_smt: sled::Tree,
}

impl Cache {
    /// Instantiate a new `Cache` with the given `sled` database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let scanned_blocks = db.open_tree(SLED_SCANNED_BLOCKS_TREE)?;
        let merkle_trees = db.open_tree(SLED_MERKLE_TREES_TREE)?;
        let nullifier_smt = db.open_tree(SLED_NULLIFIER_SMT_TREE)?;
        let bb_smt = db.open_tree(SLED_BB_SMT_TREE)?;

        Ok(Self { db: db.clone(), scanned_blocks, merkle_trees, nullifier_smt, bb_smt })
    }

    /// Execute an atomic sled batch corresponding to inserts to the
    /// merkle trees tree. For each record, the bytes slice is used as
    /// the key, and the serialized merkle tree is used as value.
    pub fn insert_merkle_trees(&self, trees: &[(&[u8], &MerkleTree)]) -> Result<()> {
        let mut batch = sled::Batch::default();
        for (key, tree) in trees {
            batch.insert(*key, serialize(*tree));
        }
        self.merkle_trees.apply_batch(batch)?;
        Ok(())
    }

    /// Get a Merkle tree by name from the cache.
    pub fn get_merkle_tree(&self, name: &[u8]) -> Option<MerkleTree> {
        let tree_bytes = self.merkle_trees.get(name).ok()??;
        deserialize(&tree_bytes).ok()
    }
}

/// Simple block scanner that writes directly to sled — no overlay.
pub struct BlockScanner {
    tree: sled::Tree,
}

impl BlockScanner {
    /// Create a new block scanner writing to the scanned_blocks tree.
    pub fn new(cache: &Cache) -> Self {
        Self { tree: cache.scanned_blocks.clone() }
    }

    /// Insert a scanned block record directly into the sled tree.
    pub fn insert_scanned_block(
        &self,
        height: &u32,
        hash: &HeaderHash,
        signing_key: &Option<SecretKey>,
    ) -> Result<()> {
        let block_signing_key = match signing_key {
            Some(key) => key.to_string(),
            None => String::from("-"),
        };
        self.tree.insert(
            height.to_be_bytes(),
            serialize(&(hash.to_string(), block_signing_key)),
        )?;
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

/// Sparse Merkle Tree storage backed directly by a sled tree — no overlay.
pub struct PnSmtStorage {
    tree: sled::Tree,
}

impl PnSmtStorage {
    pub fn new(tree: sled::Tree) -> Self {
        Self { tree }
    }

    pub fn snapshot(&self) -> Result<HashMap<BigUint, pallas::Base>> {
        let mut smt = HashMap::new();
        for record in self.tree.iter() {
            let (key, value) = record?;
            let mut repr = [0; 32];
            repr.copy_from_slice(&value);
            let Some(value) = pallas::Base::from_repr(repr).into() else {
                return Err(Error::ParseFailed(
                    "[cache::PnSmtStorage::snapshot] Value conversion failed",
                ))
            };
            smt.insert(BigUint::from_bytes_le(&key), value);
        }
        Ok(smt)
    }
}

impl StorageAdapter for PnSmtStorage {
    type Value = pallas::Base;

    fn put(&mut self, key: BigUint, value: pallas::Base) -> ContractResult {
        if let Err(e) = self.tree.insert(key.to_bytes_le(), &value.to_repr()) {
            error!(target: "cache::StorageAdapter::put", "Inserting key {key:?}, value {value:?} into DB failed: {e}");
            return Err(ContractError::SmtPutFailed)
        }
        Ok(())
    }

    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let value = match self.tree.get(key.to_bytes_le()) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "cache::StorageAdapter::get", "Fetching key {key:?} from DB failed: {e}");
                return None
            }
        };

        let value = value?;

        let mut repr = [0; 32];
        repr.copy_from_slice(&value);

        pallas::Base::from_repr(repr).into()
    }

    fn del(&mut self, key: &BigUint) -> ContractResult {
        if let Err(e) = self.tree.remove(key.to_bytes_le()) {
            error!(target: "cache::StorageAdapter::del", "Removing key {key:?} from DB failed: {e}");
            return Err(ContractError::SmtDelFailed)
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use dwow_core::{zk::halo2::Field, Result};
    use dwow_sdk::{
        crypto::smt::{gen_empty_nodes, util::FieldHasher, PoseidonFp, SparseMerkleTree},
        pasta::pallas,
    };
    use rand::rngs::OsRng;
    use sled;

    use crate::cache::{Cache, PnSmtStorage};

    #[test]
    fn test_cache_smt() -> Result<()> {
        let sled_db = sled::Config::new().temporary(true).open()?;
        let cache = Cache::new(&sled_db)?;

        // Setup SMT backed directly by the sled tree
        const HEIGHT: usize = 3;
        let hasher = PoseidonFp::new();
        let empty_leaf = pallas::Base::ZERO;
        let empty_nodes = gen_empty_nodes::<{ HEIGHT + 1 }, _, _>(&hasher, empty_leaf);
        let store = PnSmtStorage::new(cache.nullifier_smt.clone());
        let mut smt = SparseMerkleTree::<HEIGHT, { HEIGHT + 1 }, _, _, _>::new(
            store,
            hasher.clone(),
            &empty_nodes,
        );

        // Verify database is empty
        assert!(cache.nullifier_smt.is_empty());

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

        // Verify database contains keys (direct sled writes, no overlay)
        assert!(!cache.nullifier_smt.is_empty());

        Ok(())
    }
}
