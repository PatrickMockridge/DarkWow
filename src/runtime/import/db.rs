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
    crypto::contract_id::{
        ContractId, SMART_CONTRACT_MONOTREE_DB_NAME, SMART_CONTRACT_ZKAS_DB_NAME,
    },
    wasm,
};
use dwow_serial::{deserialize, serialize, Decodable};
use tracing::{debug, error, info};
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::{
    runtime::vm_runtime::{ContractSection, Env},
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Internal wasm runtime API for sled trees
#[derive(PartialEq)]
pub struct DbHandle {
    pub contract_id: ContractId,
    pub tree: [u8; 32],
}

impl DbHandle {
    pub fn new(contract_id: ContractId, tree: [u8; 32]) -> Self {
        Self { contract_id, tree }
    }
}

/// Create a new database instance for the calling contract.
///
/// This function expects to receive a pointer from which a `ContractId`
/// and the `db_name` will be read.
///
/// This function should **only** be allowed in `ContractSection::Deploy`, as that
/// is called when a contract is being (re)deployed and databases have to be created.
///
/// Permissions: deploy
pub(crate) fn db_init(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // HAZOP RC-C fix: charge_gas logs and returns true if exhausted
    if env.charge_gas(&mut store, 100) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    // Create a mem slice of the wasm VM memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_INIT_FAILED
    };

    // Allocate a buffer and copy all the data from the pointer into the buffer
    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Failed to read memory slice: {e}"
        );
        return dwow_sdk::error::DB_INIT_FAILED
    };

    // Once the data is copied, we'll attempt to deserialize it into the objects
    // we're expecting.
    let mut buf_reader = Cursor::new(buf);
    let read_cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed decoding ContractId: {e}"
            );
            return dwow_sdk::error::DB_INIT_FAILED
        }
    };

    let read_db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Failed decoding db_name: {e}"
            );
            return dwow_sdk::error::DB_INIT_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_INIT_FAILED
    }

    // We cannot allow initializing the special zkas db:
    if read_db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Attempted to init zkas db"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Nor can we allow initializing the special monotree db:
    if read_db_name == SMART_CONTRACT_MONOTREE_DB_NAME {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Attempted to init monotree db"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Nor can we allow another contract to initialize a db for someone else:
    if cid != read_cid {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): Unauthorized ContractId"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Now try to initialize the tree. The tree handle is just a hash of
    // contract_id + tree_name. We use simple_db directly.
    let tree_handle = read_cid.hash_state_id(&read_db_name);

    // Create the DbHandle
    let db_handle = DbHandle::new(read_cid, tree_handle);
    let mut db_handles = env.db_handles.borrow_mut();

    // Make sure we don't duplicate the DbHandle in the vec.
    // It's not really an issue, but it's better to be pedantic.
    if db_handles.contains(&db_handle) {
        error!(
            target: "runtime::db::db_init",
            "[WASM] [{cid}] db_init(): DbHandle initialized twice during execution"
        );
        return dwow_sdk::error::DB_INIT_FAILED
    }

    // This tries to cast into u32
    match db_handles.len().try_into() {
        Ok(db_handle_idx) => {
            db_handles.push(db_handle);
            // Return the db handle index
            db_handle_idx
        }
        Err(_) => {
            error!(
                target: "runtime::db::db_init",
                "[WASM] [{cid}] db_init(): Too many open DbHandles"
            );
            dwow_sdk::error::DB_INIT_FAILED
        }
    }
}

/// Lookup a database handle from its name.
/// If it exists, push it to the Vector of db_handles.
///
/// Returns the index of the DbHandle in the db_handles Vector on success.
/// Otherwise, returns an error value.
///
/// This function can be called from any [`ContractSection`].
///
/// Permissions: deploy, metadata, exec, update
pub(crate) fn db_lookup(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    // Enforce function ACL
    if let Err(e) = acl_allow(
        env,
        &[
            ContractSection::Deploy,
            ContractSection::Metadata,
            ContractSection::Exec,
            ContractSection::Update,
        ],
    ) {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup() called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Opening an existing db should be free (i.e. 1 gas unit).
    env.subtract_gas(&mut store, 1);

    // Read memory location that contains the ContractId and DB name
    let memory_view = env.memory_view(&store);

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Failed to make slice from ptr."
        );
        return dwow_sdk::error::DB_LOOKUP_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_LOOKUP_FAILED
    };

    // Wrap the buffer into a Cursor for stream reading
    let mut buf_reader = Cursor::new(buf);

    // Decode ContractId from memory
    let requested_cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Failed to decode ContractId: {e}"
            );
            return dwow_sdk::error::DB_LOOKUP_FAILED
        }
    };

    // Decode DB name from memory
    let db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Failed to decode db_name: {e}"
            );
            return dwow_sdk::error::DB_LOOKUP_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(), Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_LOOKUP_FAILED
    }

    // Cross-contract access prevention (type-system.md §5, §7 invariant 3):
    // A contract SHALL only open databases that belong to its own ContractId.
    // The caller-provided ContractId must match the executing contract's identity.
    // This check is symmetric with the equivalent checks in db_set (line 436),
    // db_del (line 569), and db_init (line 155).
    if requested_cid != cid {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Unauthorized ContractId — cross-contract access denied"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    if db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Attempted to lookup zkas db"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    if db_name == SMART_CONTRACT_MONOTREE_DB_NAME {
        error!(
            target: "runtime::db::db_lookup",
            "[WASM] [{cid}] db_lookup(): Attempted to lookup monotree db"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Lookup contract state - compute tree handle directly from hash
    let tree_handle = requested_cid.hash_state_id(&db_name);

    // Create the DbHandle
    let db_handle = DbHandle::new(requested_cid, tree_handle);
    let mut db_handles = env.db_handles.borrow_mut();

    // Make sure we don't duplicate the DbHandle in the vec
    if let Some(index) = db_handles.iter().position(|x| x == &db_handle) {
        return index as i64
    }

    // Push the new DbHandle to the Vec of opened DbHandles
    match db_handles.len().try_into() {
        Ok(db_handle_idx) => {
            db_handles.push(db_handle);
            db_handle_idx
        }
        Err(_) => {
            error!(
                target: "runtime::db::db_lookup",
                "[WASM] [{cid}] db_lookup(): Too many open DbHandles"
            );
            dwow_sdk::error::DB_LOOKUP_FAILED
        }
    }
}

/// Set a value within the transaction.
///
/// * `ptr` must contain the DbHandle index and the key-value pair.
/// * The DbHandle must match the ContractId.
///
/// This function can be called only from the Deploy or Update [`ContractSection`].
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// Permissions: deploy, update
pub(crate) fn db_set(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Ensure that it is possible to read from the memory that this function needs
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_SET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_SET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set",
                "[WASM] [{cid}] db_set(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // Decode key and value
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set",
                "[WASM] [{cid}] db_set(): Failed to decode key vec: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    let value: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set",
                "[WASM] [{cid}] db_set(): Failed to decode value vec: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Check DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    // Retrive DbHandle using the index. tree is [u8; 32] (Copy) —
    // copy it out so we can drop the Ref before calling subtract_gas.
    let db_handle = &db_handles[db_handle_index];
    let tree = db_handle.tree;

    // Validate that the DbHandle matches the contract ID
    let contract_id_ok = db_handle.contract_id == env.contract_id;
    drop(db_handles);

    if !contract_id_ok {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): Unauthorized to write to DbHandle"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Only charge for the net increase in size when
    // replacing existing data. key is decoded above, tree is a copy.
    let existing_len = env
        .backend
        .db_get(&tree, &key)
        .unwrap_or(None)
        .map_or(0, |d| d.len());
    let charge = if ptr_len as usize > existing_len {
        (ptr_len as usize - existing_len) as u64
    } else {
        ptr_len as u64 // Still charge for I/O overhead on same-size or smaller writes
    };
    if env.charge_gas(&mut store, charge) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    // Insert key-value pair into the database corresponding to this contract
    // Use simple_db for deterministic direct sled access
    if let Err(e) = env.backend.db_insert(&tree, &key, &value) {
        error!(
            target: "runtime::db::db_set",
            "[WASM] [{cid}] db_set(): insert failed tree={:?} key={:?} value_len={} err={:?}",
            tree,
            key.iter().take(8).collect::<Vec<_>>(),
            value.len(),
            e
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    wasm::entrypoint::SUCCESS
}

/// Remove a key from the database.
///
/// This function can be called only from the Deploy or Update [`ContractSection`].
/// Returns `SUCCESS` on success, otherwise returns an error value.
///
/// Permissions: deploy, update
pub(crate) fn db_del(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Gas is charged AFTER decoding the key (below), proportional to the
    // size of the data being deleted — symmetric with db_set which charges
    // proportional to the net data increase. Charging 1 gas for arbitrary-
    // size deletions enables write-large/delete-small resource exhaustion
    // (type-system.md §8.6.2: resource bounds SHALL be proportional to work).

    // Ensure that it is possible to read from the memory that this function needs
    let memory_view = env.memory_view(&store);

    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_del",
                "[WASM] [{cid}] db_del(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_DEL_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    // Decode key corresponding to the value that will be deleted
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_del",
                "[WASM] [{cid}] db_del(): Failed to decode key vec: {e}"
            );
            return dwow_sdk::error::DB_DEL_FAILED
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    }

    let db_handles = env.db_handles.borrow();

    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    }

    // Retrive DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Validate that the DbHandle matches the contract ID
    let contract_id_ok = db_handle.contract_id == cid;
    let tree = db_handle.tree; // Copy ([u8; 32]) — extract before dropping borrow
    drop(db_handles);

    if !contract_id_ok {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Unauthorized to write to DbHandle"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Charge gas proportional to the existing value size BEFORE deletion.
    // Symmetric with db_set (lines 449-458) which charges proportional to
    // data increase. Without this, write-large/delete-small loops consume
    // disproportionate host I/O for negligible gas (type-system.md §8.6.2).
    let existing_len = env
        .backend
        .db_get(&tree, &key)
        .unwrap_or(None)
        .map_or(0, |d| d.len());
    if env.charge_gas(&mut store, std::cmp::max(1, existing_len as u64)) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    // Remove key-value pair from the database corresponding to this contract
    if let Err(e) = env.backend.db_remove(&tree, &key) {
        error!(
            target: "runtime::db::db_del",
            "[WASM] [{cid}] db_del(): Couldn't remove key from db_handle tree: {e}"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    }

    wasm::entrypoint::SUCCESS
}

/// Reads a value by key from the key-value store.
///
/// This function can be called from the Deploy, Exec, or Metadata [`ContractSection`].
///
/// On success, returns the length of the `objects` Vector in the environment.
/// Otherwise, returns an error code.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn db_get(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec, ContractSection::Update])
    {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Reading is free.
    env.subtract_gas(&mut store, 1);

    // Ensure that it is possible to read memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_GET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_GET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get",
                "[WASM] [{cid}] db_get(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_GET_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // Decode key for key-value pair that we wish to retrieve
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get",
                "[WASM] [{cid}] db_get(): Failed to decode key from vec: {e}"
            );
            return dwow_sdk::error::DB_GET_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer. This means we've used all data that was
    // supplied.
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_GET_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Ensure that the index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_GET_FAILED
    }

    // Get DbHandle using db_handle_index
    let db_handle = &db_handles[db_handle_index];

    // Cross-contract access prevention (type-system.md §5, §7 invariant 3):
    // Verify the DbHandle belongs to this contract before reading.
    // This check is symmetric with db_set:436-444 and db_del:569.
    // Prior to 2026-07-22, db_get lacked this check while db_set and db_del
    // both had it — asymmetric ACL enabled cross-contract state reads (C1).
    if db_handle.contract_id != cid {
        error!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Unauthorized to read from DbHandle — cross-contract access denied"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Retrieve data using the `key`
    let ret = match env.backend.db_get(&db_handle.tree, &key) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "runtime::db::db_get",
                    "[WASM] [{cid}] db_get(): Internal error getting from tree: {e}"
                );
                return dwow_sdk::error::DB_GET_FAILED
            }
        };
    drop(db_handles);

    // Return special error if the data is empty
    let Some(return_data) = ret else {
        debug!(
            target: "runtime::db::db_get",
            "[WASM] [{cid}] db_get(): Return data is empty"
        );
        return dwow_sdk::error::DB_GET_EMPTY
    };

    if return_data.len() > u32::MAX as usize {
        return dwow_sdk::error::DATA_TOO_LARGE
    }

    // Subtract used gas. Here we count the length of the data read from db.
    env.subtract_gas(&mut store, return_data.len() as u64);

    // Copy the data (Vec<u8>) to the VM by pushing it to the objects Vector.
    let mut objects = env.objects.borrow_mut();
    if objects.len() == u32::MAX as usize {
        return dwow_sdk::error::DATA_TOO_LARGE
    }

    // Return the length of the objects Vector.
    // This is the location of the data that was retrieved and pushed
    objects.push(return_data.to_vec());
    (objects.len() - 1) as i64
}

/// Check if a database contains a given key.
///
/// Returns `1` if the key is found.
/// Returns `0` if the key is not found and there are no errors.
/// Otherwise, returns an error code.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn db_contains_key(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas. Reading is free.
    env.subtract_gas(&mut store, 1);

    // Ensure memory is readable
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    // Decode DbHandle index
    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_contains_key",
                "[WASM] [{cid}] db_contains_key(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };

    let db_handle_index = db_handle_index as usize;

    // Decode key
    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_contains_key",
                "[WASM] [{cid}] db_contains_key(): Failed to decode key vec: {e}"
            );
            return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };

    // Make sure there are no trailing bytes in the buffer.
    // This means we've used all data that was supplied.
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    let db_handles = env.db_handles.borrow();

    // Ensure DbHandle index is within bounds
    if db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    // Retrieve DbHandle using the index
    let db_handle = &db_handles[db_handle_index];

    // Cross-contract access prevention (type-system.md §5, §7 invariant 3):
    // Verify the DbHandle belongs to this contract before reading.
    // This check is symmetric with db_set:436-444, db_del:569, and db_get:695.
    if db_handle.contract_id != cid {
        error!(
            target: "runtime::db::db_contains_key",
            "[WASM] [{cid}] db_contains_key(): Unauthorized to read from DbHandle — cross-contract access denied"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Lookup key parameter in the database
    match env.backend.db_contains_key(&db_handle.tree, &key) {
        Ok(v) => i64::from(v), // <- 0=false, 1=true. Convert bool to i64.
        Err(e) => {
            error!(
                target: "runtime::db::db_contains_key",
                "[WASM] [{cid}] db_contains_key(): simple_db.contains_key failed: {e}"
            );
            dwow_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    }
}

/// Given a zkas circuit, create a VerifyingKey and insert them both into the db.
///
/// This function can only be called from the Deploy [`ContractSection`].
/// Returns `SUCCESS` on success, otherwise returns an error code.
///
/// Permissions: deploy
pub(crate) fn zkas_db_set(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy]) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    let memory_view = env.memory_view(&store);

    // Ensure that the memory is readable
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_SET_FAILED
    };

    let mut buf = vec![0u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_SET_FAILED
    };

    // Deserialize the ZkBinary bytes from the buffer
    let zkbin_bytes: Vec<u8> = match deserialize(&buf) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Could not deserialize bytes from buffer: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    // Validate the bytes by decoding them into the ZkBinary format
    let zkbin = match ZkBinary::decode(&zkbin_bytes, false) {
        Ok(zkbin) => zkbin,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Invalid zkas bincode passed to function: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    // Subtract used gas. ZK circuit verification is expensive — weight
    // opcodes more heavily than literals/witnesses since they involve
    // constraint evaluation. Literals and witnesses are just data.
    let gas_cost = (zkbin.opcodes.len() as u64 * 200)
        + ((zkbin.literals.len() + zkbin.witnesses.len()) as u64 * 50);
    if env.charge_gas(&mut store, gas_cost) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    // Because of `Runtime::Deploy`, we should be sure that the zkas db is index zero.
    let db_handles = env.db_handles.borrow();
    let db_handle = &db_handles[0];
    // Redundant check
    if db_handle.contract_id != cid {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Internal error, zkas db at index 0 incorrect"
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    // Check if there is existing bincode and compare it. Return DB_SUCCESS if
    // they're the same. The assumption should be that VerifyingKey was generated
    // already so we can skip things after this guard.
    match env.backend.db_get(&db_handle.tree, &serialize(&zkbin.namespace)) {
        Ok(v) => {
            if let Some(bytes) = v {
                let (existing_zkbin, _): (Vec<u8>, Vec<u8>) = match deserialize(&bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        error!(
                            target: "runtime::db::zkas_db_set",
                            "[WASM] [{cid}] zkas_db_set(): Corrupt zkas namespace data: {e}"
                        );
                        return dwow_sdk::error::DB_SET_FAILED
                    }
                };

                if existing_zkbin == zkbin_bytes {
                    debug!(
                        target: "runtime::db::zkas_db_set",
                        "[WASM] [{cid}] zkas_db_set(): Existing zkas bincode is the same. Skipping."
                    );
                    return wasm::entrypoint::SUCCESS
                }
            }
        }
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Internal error getting from tree: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    // We didn't find any existing bincode, so let's create a new VerifyingKey and write it all.
    info!(
        target: "runtime::db::zkas_db_set",
        "[WASM] [{cid}] zkas_db_set(): Creating VerifyingKey for {} zkas circuit",
        zkbin.namespace,
    );

    let witnesses = match empty_witnesses(&zkbin) {
        Ok(w) => w,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): Failed to create empty witnesses: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    // Construct the circuit and build the VerifyingKey.
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let vk = match VerifyingKey::build(zkbin.k, &circuit) {
        Ok(vk) => vk,
        Err(e) => {
            error!(
                target: "runtime::db::zkas_db_set",
                "[WASM] [{cid}] zkas_db_set(): VerifyingKey::build failed for circuit '{}': {e}",
                zkbin.namespace,
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };
    let mut vk_buf = vec![];
    if let Err(e) = vk.write(&mut vk_buf) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Failed to serialize VerifyingKey: {e}"
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    // Insert the key-value pair into the database.
    let key = serialize(&zkbin.namespace);
    let value = serialize(&(zkbin_bytes, vk_buf));
    if let Err(e) = env.backend.db_insert(&db_handle.tree, &key, &value) {
        error!(
            target: "runtime::db::zkas_db_set",
            "[WASM] [{cid}] zkas_db_set(): Couldn't insert to db_handle tree: {e}"
        );
        return dwow_sdk::error::DB_SET_FAILED
    }
    drop(db_handles);

    // Subtract used gas. Here we count the bytes written into the db.
    if env.charge_gas(&mut store, (key.len() + value.len()) as u64) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    wasm::entrypoint::SUCCESS
}

/// Lookup a database handle from its name (local/ephemeral).
/// Same as db_lookup but pushes to local_db_handles instead of db_handles.
///
/// Permissions: deploy, metadata, exec, update
pub(crate) fn db_lookup_local(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(
        env,
        &[
            ContractSection::Deploy,
            ContractSection::Metadata,
            ContractSection::Exec,
            ContractSection::Update,
        ],
    ) {
        error!(
            target: "runtime::db::db_lookup_local",
            "[WASM] [{cid}] db_lookup_local() called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    env.subtract_gas(&mut store, 1);

    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_lookup_local",
            "[WASM] [{cid}] db_lookup_local(): Failed to make slice from ptr."
        );
        return dwow_sdk::error::DB_LOOKUP_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_lookup_local",
            "[WASM] [{cid}] db_lookup_local(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_LOOKUP_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    let requested_cid: ContractId = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup_local",
                "[WASM] [{cid}] db_lookup_local(): Failed to decode ContractId: {e}"
            );
            return dwow_sdk::error::DB_LOOKUP_FAILED
        }
    };

    let db_name: String = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_lookup_local",
                "[WASM] [{cid}] db_lookup_local(): Failed to decode db_name: {e}"
            );
            return dwow_sdk::error::DB_LOOKUP_FAILED
        }
    };

    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_lookup_local",
            "[WASM] [{cid}] db_lookup_local(), Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_LOOKUP_FAILED
    }

    // Cross-contract access prevention (type-system.md §5, §7 invariant 3):
    // Same check as db_lookup — a contract SHALL only open local databases
    // that belong to its own ContractId.
    if requested_cid != cid {
        error!(
            target: "runtime::db::db_lookup_local",
            "[WASM] [{cid}] db_lookup_local(): Unauthorized ContractId — cross-contract access denied"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    if db_name == SMART_CONTRACT_ZKAS_DB_NAME {
        error!(
            target: "runtime::db::db_lookup_local",
            "[WASM] [{cid}] db_lookup_local(): Attempted to lookup zkas db"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    if db_name == SMART_CONTRACT_MONOTREE_DB_NAME {
        error!(
            target: "runtime::db::db_lookup_local",
            "[WASM] [{cid}] db_lookup_local(): Attempted to lookup monotree db"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    let tree_handle = requested_cid.hash_state_id(&db_name);
    let db_handle = DbHandle::new(requested_cid, tree_handle);
    let mut local_db_handles = env.local_db_handles.borrow_mut();

    if let Some(index) = local_db_handles.iter().position(|x| x == &db_handle) {
        return index as i64
    }

    match local_db_handles.len().try_into() {
        Ok(db_handle_idx) => {
            local_db_handles.push(db_handle);
            db_handle_idx
        }
        Err(_) => {
            error!(
                target: "runtime::db::db_lookup_local",
                "[WASM] [{cid}] db_lookup_local(): Too many open DbHandles"
            );
            dwow_sdk::error::DB_LOOKUP_FAILED
        }
    }
}

/// Set a value within the transaction-local (ephemeral) state.
/// Never committed to the blockchain.
///
/// Permissions: deploy, update
pub(crate) fn db_set_local(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(
            target: "runtime::db::db_set_local",
            "[WASM] [{cid}] db_set_local(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    if env.charge_gas(&mut store, ptr_len as u64) {
        return dwow_sdk::error::INTERNAL_ERROR;
    }

    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_set_local",
            "[WASM] [{cid}] db_set_local(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_SET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_set_local",
            "[WASM] [{cid}] db_set_local(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_SET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set_local",
                "[WASM] [{cid}] db_set_local(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set_local",
                "[WASM] [{cid}] db_set_local(): Failed to decode key vec: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    let value: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_set_local",
                "[WASM] [{cid}] db_set_local(): Failed to decode value vec: {e}"
            );
            return dwow_sdk::error::DB_SET_FAILED
        }
    };

    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_set_local",
            "[WASM] [{cid}] db_set_local(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    let local_db_handles = env.local_db_handles.borrow();
    if local_db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_set_local",
            "[WASM] [{cid}] db_set_local(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_SET_FAILED
    }

    let db_handle = &local_db_handles[db_handle_index];
    if db_handle.contract_id != env.contract_id {
        error!(
            target: "runtime::db::db_set_local",
            "[WASM] [{cid}] db_set_local(): Unauthorized to write to DbHandle"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Write to tx_local BTreeMap (ephemeral, never committed)
    let mut tx_local = env.tx_local.lock().unwrap();
    tx_local
        .entry(cid)
        .or_default()
        .entry(db_handle.tree)
        .or_default()
        .insert(key, value);

    wasm::entrypoint::SUCCESS
}

/// Get a value from the transaction-local (ephemeral) state.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn db_get_local(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::db_get_local",
            "[WASM] [{cid}] db_get_local(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    env.subtract_gas(&mut store, 1);

    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_get_local",
            "[WASM] [{cid}] db_get_local(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_GET_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_get_local",
            "[WASM] [{cid}] db_get_local(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_GET_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get_local",
                "[WASM] [{cid}] db_get_local(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_GET_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_get_local",
                "[WASM] [{cid}] db_get_local(): Failed to decode key from vec: {e}"
            );
            return dwow_sdk::error::DB_GET_FAILED
        }
    };

    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_get_local",
            "[WASM] [{cid}] db_get_local(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_GET_FAILED
    }

    let local_db_handles = env.local_db_handles.borrow();
    if local_db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_get_local",
            "[WASM] [{cid}] db_get_local(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_GET_FAILED
    }

    let db_handle = &local_db_handles[db_handle_index];

    // Read from tx_local BTreeMap
    let tx_local = env.tx_local.lock().unwrap();
    let return_data = tx_local
        .get(&cid)
        .and_then(|trees| trees.get(&db_handle.tree))
        .and_then(|kv| kv.get(&key))
        .cloned();

    drop(tx_local);
    drop(local_db_handles);

    let Some(return_data) = return_data else {
        debug!(
            target: "runtime::db::db_get_local",
            "[WASM] [{cid}] db_get_local(): Return data is empty"
        );
        return dwow_sdk::error::DB_GET_EMPTY
    };

    if return_data.len() > u32::MAX as usize {
        return dwow_sdk::error::DATA_TOO_LARGE
    }

    env.subtract_gas(&mut store, return_data.len() as u64);

    let mut objects = env.objects.borrow_mut();
    if objects.len() == u32::MAX as usize {
        return dwow_sdk::error::DATA_TOO_LARGE
    }

    objects.push(return_data.to_vec());
    (objects.len() - 1) as i64
}

/// Delete a key from the transaction-local (ephemeral) state.
///
/// Permissions: deploy, update
pub(crate) fn db_del_local(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Deploy, ContractSection::Update]) {
        error!(
            target: "runtime::db::db_del_local",
            "[WASM] [{cid}] db_del_local(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    env.subtract_gas(&mut store, 1);

    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_del_local",
            "[WASM] [{cid}] db_del_local(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_del_local",
            "[WASM] [{cid}] db_del_local(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_del_local",
                "[WASM] [{cid}] db_del_local(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_DEL_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_del_local",
                "[WASM] [{cid}] db_del_local(): Failed to decode key vec: {e}"
            );
            return dwow_sdk::error::DB_DEL_FAILED
        }
    };

    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_del_local",
            "[WASM] [{cid}] db_del_local(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    }

    let local_db_handles = env.local_db_handles.borrow();
    if local_db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_del_local",
            "[WASM] [{cid}] db_del_local(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_DEL_FAILED
    }

    let db_handle = &local_db_handles[db_handle_index];
    if db_handle.contract_id != cid {
        error!(
            target: "runtime::db::db_del_local",
            "[WASM] [{cid}] db_del_local(): Unauthorized to write to DbHandle"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    // Remove from tx_local BTreeMap
    let mut tx_local = env.tx_local.lock().unwrap();
    if let Some(trees) = tx_local.get_mut(&cid) {
        if let Some(kv) = trees.get_mut(&db_handle.tree) {
            kv.remove(&key);
        }
    }

    wasm::entrypoint::SUCCESS
}

/// Check if a key exists in the transaction-local (ephemeral) state.
///
/// Permissions: deploy, metadata, exec
pub(crate) fn db_contains_key_local(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) =
        acl_allow(env, &[ContractSection::Deploy, ContractSection::Metadata, ContractSection::Exec])
    {
        error!(
            target: "runtime::db::db_contains_key_local",
            "[WASM] [{cid}] db_contains_key_local(): Called in unauthorized section: {e}"
        );
        return dwow_sdk::error::CALLER_ACCESS_DENIED
    }

    env.subtract_gas(&mut store, 1);

    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::db::db_contains_key_local",
            "[WASM] [{cid}] db_contains_key_local(): Failed to make slice from ptr"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    };

    let mut buf = vec![0_u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::db::db_contains_key_local",
            "[WASM] [{cid}] db_contains_key_local(): Failed to read from memory slice: {e}"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    };

    let mut buf_reader = Cursor::new(buf);

    let db_handle_index: u32 = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_contains_key_local",
                "[WASM] [{cid}] db_contains_key_local(): Failed to decode DbHandle: {e}"
            );
            return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };
    let db_handle_index = db_handle_index as usize;

    let key: Vec<u8> = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::db::db_contains_key_local",
                "[WASM] [{cid}] db_contains_key_local(): Failed to decode key vec: {e}"
            );
            return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
        }
    };

    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::db::db_contains_key_local",
            "[WASM] [{cid}] db_contains_key_local(): Trailing bytes in argument stream"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    let local_db_handles = env.local_db_handles.borrow();
    if local_db_handles.len() <= db_handle_index {
        error!(
            target: "runtime::db::db_contains_key_local",
            "[WASM] [{cid}] db_contains_key_local(): Requested DbHandle that is out of bounds"
        );
        return dwow_sdk::error::DB_CONTAINS_KEY_FAILED
    }

    let db_handle = &local_db_handles[db_handle_index];

    // Check in tx_local BTreeMap
    let tx_local = env.tx_local.lock().unwrap();
    let found = tx_local
        .get(&cid)
        .and_then(|trees| trees.get(&db_handle.tree))
        .map(|kv| kv.contains_key(&key))
        .unwrap_or(false);

    i64::from(found)
}
