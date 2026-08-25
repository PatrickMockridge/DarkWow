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

use std::io::Cursor;

use dwow_sdk::{
    crypto::{
        pasta_prelude::*,
        smt::{PoseidonFp, SparseMerkleTree, StorageAdapter, EMPTY_NODES_FP, SMT_FP_DEPTH},
    },
    error::{ContractError, ContractResult},
    wasm,
};
use dwow_serial::{deserialize, serialize, Decodable, Encodable};
use halo2_proofs::pasta::pallas;
use num_bigint::BigUint;
use tracing::{debug, error};
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::runtime::vm_runtime::{ContractSection, Env, RuntimeBackend};

/// An SMT adapter for runtime backend storage. Deterministic, no overlay/diffs.
pub struct SimpleDbStorage<'a> {
    backend: &'a dyn RuntimeBackend,
    tree_key: &'a [u8],
}

/// Namespace prefix prepended to all SMT keys to prevent collision with
/// contract application keys (nullifiers, commitments, etc.) stored in the same
/// sled tree. Without this, SMT internal node keys (BigUint::to_bytes_le())
/// and contract keys (nullifier.to_bytes(), commitment.to_bytes()) share a key
/// space — non-colliding in practice but with no formal guarantee.
const SMT_KEY_PREFIX: u8 = 0x01;

fn smt_key(key: &BigUint) -> Vec<u8> {
    let raw = key.to_bytes_le();
    let mut prefixed = Vec::with_capacity(1 + raw.len());
    prefixed.push(SMT_KEY_PREFIX);
    prefixed.extend_from_slice(&raw);
    prefixed
}

impl StorageAdapter for SimpleDbStorage<'_> {
    type Value = pallas::Base;

    fn put(&mut self, key: BigUint, value: pallas::Base) -> ContractResult {
        let prefixed = smt_key(&key);
        if let Err(e) = self.backend.db_insert(self.tree_key, &prefixed, &value.to_repr()) {
            error!(
                target: "runtime::smt::SimpleDbStorage::put",
                "[WASM] SimpleDbStorage::put(): inserting key {key:?}, value {value:?} into DB tree: {:?}: {e}",
                self.tree_key
            );
            return Err(ContractError::SmtPutFailed)
        }
        Ok(())
    }

    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let prefixed = smt_key(key);
        let value = match self.backend.db_get(self.tree_key, &prefixed) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "runtime::smt::SimpleDbStorage::get",
                    "[WASM] SimpleDbStorage::get(): Fetching key {key:?} from DB tree: {:?}: {e}",
                    self.tree_key
                );
                return None
            }
        };
        let value = value?;
        // Length guard: contract application keys (nullifiers, commitments)
        // stored in the same sled tree are not valid SMT node values.
        // A contract entry at a colliding key would produce wrong-length
        // data — guard against panic in copy_from_slice.
        if value.len() != 32 {
            error!(
                target: "runtime::smt::SimpleDbStorage::get",
                "[WASM] SimpleDbStorage::get(): value for key {key:?} has length {}, expected 32 — \
                 possible contract/SMT key collision or corrupt data",
                value.len()
            );
            return None
        }
        let mut repr = [0; 32];
        repr.copy_from_slice(&value);
        pallas::Base::from_repr(repr).into()
    }

    fn del(&mut self, key: &BigUint) -> ContractResult {
        let prefixed = smt_key(key);
        if let Err(e) = self.backend.db_remove(self.tree_key, &prefixed) {
            error!(
                target: "runtime::smt::SimpleDbStorage::del",
                "[WASM] SimpleDbStorage::del(): Removing key {key:?} from DB tree: {:?}: {e}",
                self.tree_key
            );
            return Err(ContractError::SmtDelFailed)
        }
        Ok(())
    }
}

/// Adds data to sparse merkle tree. The tree, database connection, and new data to add is
/// read from `ptr` at offset specified by `len`.
/// Returns `0` on success; otherwise, returns an error-code corresponding to a
/// [`ContractError`] (defined in the SDK).
/// See also the method `merkle_add` in `sdk/src/merkle.rs`.
///
/// Permissions: update
pub(crate) fn sparse_merkle_insert_batch(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Update]) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // HAZOP RC-C fix: charge_gas checks exhaustion
    if env.charge_gas(&mut store, 1) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, len) else {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::SMT_MEMORY_FAULT
    };

    let mut buf = vec![0_u8; len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::SMT_MEMORY_FAULT
    };

    // The buffer should deserialize into:
    // - db_smt
    // - db_roots
    // - nullifiers (as Vec<pallas::Base>)
    let mut buf_reader = Cursor::new(buf);
    let db_info_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to decode db_info DbHandle: {e}"
            );
            return dwow_sdk::error::SMT_DECODE_FAILED
        }
    };
    let db_info_index = db_info_index as usize;

    let db_smt_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to decode db_smt DbHandle: {e}"
            );
            return dwow_sdk::error::SMT_DECODE_FAILED
        }
    };
    let db_smt_index = db_smt_index as usize;

    let db_roots_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to decode db_roots DbHandle: {e}"
            );
            return dwow_sdk::error::SMT_DECODE_FAILED
        }
    };
    let db_roots_index = db_roots_index as usize;

    let db_handles = env.db_handles.borrow();
    let n_dbs = db_handles.len();

    if n_dbs <= db_info_index || n_dbs <= db_smt_index || n_dbs <= db_roots_index {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::SMT_HANDLE_OUT_OF_BOUNDS
    }
    let db_info = &db_handles[db_info_index];
    let db_smt = &db_handles[db_smt_index];
    let db_roots = &db_handles[db_roots_index];

    // Make sure that the contract owns the dbs it wants to write to
    if db_info.contract_id != env.contract_id ||
        db_smt.contract_id != env.contract_id ||
        db_roots.contract_id != env.contract_id
    {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Unauthorized to write to DbHandle"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // This `key` represents the sled key in info where the latest root is
    let root_key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to decode key vec: {e}"
            );
            return dwow_sdk::error::SMT_DECODE_FAILED
        }
    };

    // This `nullifier` represents the leaf we're adding to the Merkle tree
    let nullifiers: Vec<pallas::Base> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to decode pallas::Base: {e}"
            );
            return dwow_sdk::error::SMT_DECODE_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != (len as u64) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Mismatch between given length, and cursor length"
        );
        return dwow_sdk::error::SMT_CURSOR_MISMATCH
    }

    // Generate the SimpleDbStorage SMT
    let hasher = PoseidonFp::new();
    let smt_store = SimpleDbStorage { backend: env.backend.as_ref(), tree_key: &db_smt.tree };
    let mut smt = SparseMerkleTree::<
        SMT_FP_DEPTH,
        { SMT_FP_DEPTH + 1 },
        pallas::Base,
        PoseidonFp,
        SimpleDbStorage,
    >::new(smt_store, hasher, &EMPTY_NODES_FP);

    // Count the nullifiers for gas calculation
    let inserted_nullifiers = nullifiers.len() * 32;

    // Insert the new nullifiers
    let leaves: Vec<_> = nullifiers.iter().map(|x| (*x, *x)).collect();
    if let Err(e) = smt.insert_batch(leaves) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): SMT failed to insert batch: {e}"
        );
        return dwow_sdk::error::SMT_INSERT_FAILED
    };

    // Grab the current SMT root to add in our set of roots.
    // Since each update to the tree is atomic, we only need to add the last root.
    let latest_root = smt.root();

    // Validate latest root data, to ensure their integrity
    let latest_root_data = serialize(&latest_root);
    if latest_root_data.len() != 32 {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Latest root data length missmatch: {}", latest_root_data.len(),
        );
        return dwow_sdk::error::SMT_DATA_MISMATCH
    }

    // Validate the new value data, to ensure their integrity
    let mut new_value_data = Vec::with_capacity(32 + 1);
    if let Err(e) = env.tx_hash.inner().encode(&mut new_value_data) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to serialize transaction hash: {e}"
        );
        return dwow_sdk::error::SMT_ENCODE_FAILED
    };
    if let Err(e) = env.call_idx.encode(&mut new_value_data) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to serialize call index: {e}"
        );
        return dwow_sdk::error::SMT_ENCODE_FAILED
    };
    if new_value_data.len() != 32 + 1 {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): New value data length missmatch: {}", new_value_data.len(),
        );
        return dwow_sdk::error::SMT_DATA_MISMATCH
    }

    // Retrieve snapshot root data set
    let root_value_data_set = match env.backend.db_get(&db_roots.tree, &latest_root_data) {
        Ok(data) => data,
        Err(e) => {
            error!(
                target: "runtime::smt::sparse_merkle_insert_batch",
                "[WASM] [{cid}] sparse_merkle_insert_batch(): SMT failed to retrieve current root snapshot: {e}"
            );
            return dwow_sdk::error::DB_GET_FAILED
        }
    };

    // If the record exists, append the new value data,
    // otherwise create a new set with it.
    let root_value_data_set = match root_value_data_set {
        Some(value_data_set) => {
            let mut value_data_set: Vec<Vec<u8>> = match deserialize(&value_data_set) {
                Ok(set) => set,
                Err(e) => {
                    error!(
                        target: "runtime::smt::sparse_merkle_insert_batch",
                        "[WASM] [{cid}] sparse_merkle_insert_batch(): Failed to deserialize current root snapshot: {e}"
                    );
                    return dwow_sdk::error::SMT_DECODE_FAILED
                }
            };

            if !value_data_set.contains(&new_value_data) {
                value_data_set.push(new_value_data);
            }

            value_data_set
        }
        None => vec![new_value_data],
    };

    // Write the latest root snapshot
    debug!(
        target: "runtime::smt::sparse_merkle_insert_batch",
        "[WASM] [{cid}] sparse_merkle_insert_batch(): Appending SMT root to db: {latest_root:?}"
    );
    if let Err(e) = env.backend.db_insert(&db_roots.tree, &latest_root_data, &serialize(&root_value_data_set)) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): insert to db_roots failed tree={:?} err={:?}",
            db_roots.tree,
            e
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    // Update the pointer to the latest known root
    debug!(
        target: "runtime::smt::sparse_merkle_insert_batch",
        "[WASM] [{cid}] sparse_merkle_insert_batch(): Replacing latest SMT root pointer"
    );
    if let Err(e) = env.backend.db_insert(&db_info.tree, &root_key, &latest_root_data) {
        error!(
            target: "runtime::smt::sparse_merkle_insert_batch",
            "[WASM] [{cid}] sparse_merkle_insert_batch(): insert to db_info failed tree={:?} root_key={:?} err={:?}",
            db_info.tree,
            root_key.iter().take(8).collect::<Vec<_>>(),
            e
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    // HAZOP RC-C fix: charge_gas checks exhaustion
    drop(db_handles);
    if env.charge_gas(&mut store, inserted_nullifiers as u64) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    wasm::entrypoint::SUCCESS
}
