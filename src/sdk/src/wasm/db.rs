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


#[cfg(target_arch = "wasm32")]
use dwow_serial::{Decodable, Encodable};

use crate::{
    crypto::ContractId,
    error::{ContractError, GenericResult},
};

pub type DbHandle = u32;

/// Create a new database instance for the given contract.
/// This should be called in the `init_contract()` section to create any databases
/// that the contract might need or use.
///
/// Returns a `DbHandle` which provides methods for reading and writing.
#[cfg(target_arch = "wasm32")]
pub fn db_init(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += contract_id.encode(&mut buf)?;
        len += db_name.to_string().encode(&mut buf)?;

        let ret = db_init_(buf.as_ptr(), len as u32);

        if ret < 0 {
            return Err(ContractError::from(ret))
        }

        Ok(ret as u32)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_init(_contract_id: ContractId, _db_name: &str) -> GenericResult<DbHandle> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Everyone can call this. Assumes that the database already went through `db_init()`.
#[cfg(target_arch = "wasm32")]
pub fn db_lookup(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += contract_id.encode(&mut buf)?;
        len += db_name.to_string().encode(&mut buf)?;

        let ret = db_lookup_(buf.as_ptr(), len as u32);

        if ret < 0 {
            return Err(ContractError::from(ret))
        }

        Ok(ret as u32)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_lookup(_contract_id: ContractId, _db_name: &str) -> GenericResult<DbHandle> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Everyone can call this. Will read a key from the key-value store.
///
/// ```
/// value = db_get(db_handle, key);
/// ```
#[cfg(target_arch = "wasm32")]
pub fn db_get(db_handle: DbHandle, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.to_vec().encode(&mut buf)?;

    let ret = unsafe { db_get_(buf.as_ptr(), len as u32) };
    crate::wasm::util::parse_ret(ret)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_get(_db_handle: DbHandle, _key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Everyone can call this. Checks if a key is contained in the key-value store.
///
/// ```
/// if db_contains_key(db_handle, key) {
///     println!("true");
/// }
/// ```
#[cfg(target_arch = "wasm32")]
pub fn db_contains_key(db_handle: DbHandle, key: &[u8]) -> GenericResult<bool> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.to_vec().encode(&mut buf)?;

    let ret = unsafe { db_contains_key_(buf.as_ptr(), len as u32) };

    if ret < 0 {
        return Err(ContractError::from(ret))
    }

    match ret {
        0 => Ok(false),
        1 => Ok(true),
        _ => unreachable!(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_contains_key(_db_handle: DbHandle, _key: &[u8]) -> GenericResult<bool> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Only update() can call this. Set a value within the transaction.
///
/// ```
/// db_set(tx_handle, key, value);
/// ```
#[cfg(target_arch = "wasm32")]
pub fn db_set(db_handle: DbHandle, key: &[u8], value: &[u8]) -> GenericResult<()> {
    // Check entry for tx_handle is not None
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;
        len += value.to_vec().encode(&mut buf)?;

        let ret = db_set_(buf.as_ptr(), len as u32);

        if ret != crate::wasm::entrypoint::SUCCESS {
            return Err(ContractError::from(ret))
        }

        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_set(_db_handle: DbHandle, _key: &[u8], _value: &[u8]) -> GenericResult<()> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Mark a key as spent by writing a non-empty marker.
///
/// Contracts use this to record nullifiers, commitments, and other "consumed" markers
/// that must be visible to `db_contains_key`. This writes `&[1]` explicitly
/// because `db_set(key, &[])` writes an empty value, which `db_contains_key`
/// treats as absent (the empty-value-as-absent defect). This is the single
/// nullifier storage convention (contract-wasm-standards-best-practices.md §9):
/// write via `db_mark_spent`, read via `db_contains_key`.
#[cfg(target_arch = "wasm32")]
pub fn db_mark_spent(db_handle: DbHandle, key: &[u8]) -> GenericResult<()> {
    db_set(db_handle, key, &[1])
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_mark_spent(_db_handle: DbHandle, _key: &[u8]) -> GenericResult<()> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Only update() can call this. Removes a key from the db.
///
/// ```
///     db_del(tx_handle, key);
/// ```
#[cfg(target_arch = "wasm32")]
pub fn db_del(db_handle: DbHandle, key: &[u8]) -> GenericResult<()> {
    // Check entry for tx_handle is not None
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;

        let ret = db_del_(buf.as_ptr(), len as u32);

        if ret != crate::wasm::entrypoint::SUCCESS {
            return Err(ContractError::from(ret))
        }

        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_del(_db_handle: DbHandle, _key: &[u8]) -> GenericResult<()> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

// ── Local (ephemeral) DB functions ──────────────────────────────────
// These operate on the transaction-local state (TxLocalState) rather
// than the persistent contract state. Values are discarded when the
// transaction completes. Used for temporary in-memory storage during
// contract execution.

/// Look up or create a local (ephemeral) database handle.
/// Local DBs live in TxLocalState — discarded at tx completion.
#[cfg(target_arch = "wasm32")]
pub fn db_lookup_local(cid: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += cid.encode(&mut buf)?;
        len += db_name.to_string().encode(&mut buf)?;
        let ret = db_lookup_local_(buf.as_ptr(), len as u32);
        if ret != crate::wasm::entrypoint::SUCCESS {
            return Err(ContractError::from(ret));
        }
        Ok(ret as DbHandle)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_lookup_local(_cid: ContractId, _db_name: &str) -> GenericResult<DbHandle> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Set a key-value pair in a local (ephemeral) database.
/// Only callable from update() context.
#[cfg(target_arch = "wasm32")]
pub fn db_set_local(db_handle: DbHandle, key: &[u8], value: &[u8]) -> GenericResult<()> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;
        len += value.to_vec().encode(&mut buf)?;
        let ret = db_set_local_(buf.as_ptr(), len as u32);
        if ret != crate::wasm::entrypoint::SUCCESS {
            return Err(ContractError::from(ret));
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_set_local(_db_handle: DbHandle, _key: &[u8], _value: &[u8]) -> GenericResult<()> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Get a value from a local (ephemeral) database.
#[cfg(target_arch = "wasm32")]
pub fn db_get_local(db_handle: DbHandle, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    let mut len = 0;
    let mut buf = vec![];
    len += db_handle.encode(&mut buf)?;
    len += key.to_vec().encode(&mut buf)?;
    let ret = unsafe { db_get_local_(buf.as_ptr(), len as u32) };
    crate::wasm::util::parse_ret(ret)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_get_local(_db_handle: DbHandle, _key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Check if a key exists in a local (ephemeral) database.
#[cfg(target_arch = "wasm32")]
pub fn db_contains_key_local(db_handle: DbHandle, key: &[u8]) -> GenericResult<bool> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;
        let ret = db_contains_key_local_(buf.as_ptr(), len as u32);
        match ret {
            0 => Ok(false),
            1 => Ok(true),
            _ => unreachable!(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_contains_key_local(_db_handle: DbHandle, _key: &[u8]) -> GenericResult<bool> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

/// Delete a key from a local (ephemeral) database.
/// Only callable from update() context.
#[cfg(target_arch = "wasm32")]
pub fn db_del_local(db_handle: DbHandle, key: &[u8]) -> GenericResult<()> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;
        let ret = db_del_local_(buf.as_ptr(), len as u32);
        if ret != crate::wasm::entrypoint::SUCCESS {
            return Err(ContractError::from(ret));
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn db_del_local(_db_handle: DbHandle, _key: &[u8]) -> GenericResult<()> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

// ── Circuit loading ─────────────────────────────────────────────────

/// Only deploy() can call this.
#[cfg(target_arch = "wasm32")]
pub fn zkas_db_set(bincode: &[u8]) -> GenericResult<()> {
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += bincode.to_vec().encode(&mut buf)?;

        let ret = zkas_db_set_(buf.as_ptr(), len as u32);

        if ret != crate::wasm::entrypoint::SUCCESS {
            return Err(ContractError::from(ret))
        }

        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn zkas_db_set(_bincode: &[u8]) -> GenericResult<()> {
    Err(ContractError::IoError("wasm host function unavailable".to_string()))
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn db_init_(ptr: *const u8, len: u32) -> i64;
    fn db_lookup_(ptr: *const u8, len: u32) -> i64;
    fn db_get_(ptr: *const u8, len: u32) -> i64;
    fn db_contains_key_(ptr: *const u8, len: u32) -> i64;
    fn db_set_(ptr: *const u8, len: u32) -> i64;
    fn db_del_(ptr: *const u8, len: u32) -> i64;

    fn db_lookup_local_(ptr: *const u8, len: u32) -> i64;
    fn db_set_local_(ptr: *const u8, len: u32) -> i64;
    fn db_get_local_(ptr: *const u8, len: u32) -> i64;
    fn db_contains_key_local_(ptr: *const u8, len: u32) -> i64;
    fn db_del_local_(ptr: *const u8, len: u32) -> i64;

    fn zkas_db_set_(ptr: *const u8, len: u32) -> i64;
}
