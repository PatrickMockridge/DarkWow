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

use core::{mem::size_of, slice::from_raw_parts};

use crate::crypto::ContractId;
use crate::error::ContractError;

/// Success exit code for a contract
pub const SUCCESS: i64 = 0;

/// Internal macro: generates the 4 standard WASM exports.
/// Called by both `define_contract!` and `define_contract_with_spend_hook!`.
#[macro_export]
macro_rules! __contract_exports {
    (
        init: $init_func:ident,
        exec: $exec_func:ident,
        apply: $apply_func:ident,
        metadata: $metadata_func:ident
    ) => {
        /// # Safety
        #[no_mangle]
        pub unsafe extern "C" fn __initialize(input: *mut u8) -> i64 {
            let (contract_id, instruction_data) = match $crate::wasm::entrypoint::deserialize(input) {
                Ok(v) => v,
                Err(e) => return e.into(),
            };

            match $init_func(contract_id, &instruction_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
        #[no_mangle]
        pub unsafe extern "C" fn __entrypoint(input: *mut u8) -> i64 {
            let (contract_id, instruction_data) = match $crate::wasm::entrypoint::deserialize(input) {
                Ok(v) => v,
                Err(e) => return e.into(),
            };

            match $exec_func(contract_id, &instruction_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
        #[no_mangle]
        pub unsafe extern "C" fn __update(input: *mut u8) -> i64 {
            let (contract_id, update_data) = match $crate::wasm::entrypoint::deserialize(input) {
                Ok(v) => v,
                Err(e) => return e.into(),
            };

            match $apply_func(contract_id, &update_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
        #[no_mangle]
        pub unsafe extern "C" fn __metadata(input: *mut u8) -> i64 {
            let (contract_id, instruction_data) = match $crate::wasm::entrypoint::deserialize(input) {
                Ok(v) => v,
                Err(e) => return e.into(),
            };

            match $metadata_func(contract_id, &instruction_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }

        /// Allocate the buffer the host writes a call payload into.
        ///
        /// **Why this exists.** Every section export above takes `input: *mut u8` and the host
        /// writes `[32-byte contract_id][u64 length][payload]` there. The host used to write
        /// that at offset 0, inside the guest's **shadow stack**: the one region the allocator
        /// never returns, and safe only while the payload fits below `__stack_pointer`
        /// (1,048,576 in every artifact). A payload of 1,494,589 bytes — `deployooor`
        /// deploying its own artifact, the only one over the window — overwrote `.rodata` and
        /// dlmalloc's own state, so the guest's first `malloc` inside `deserialize` allocated
        /// from payload bytes.
        ///
        /// No host-side placement is sound, because dlmalloc owns `[__heap_base, max)`: the
        /// heap can grow to the memory's maximum (256 MB, set by the host's `MemoryType`), so
        /// a payload parked anywhere above `__heap_base` can be reached by a later allocation,
        /// and one parked below `__stack_pointer` shares the window with a descending stack.
        /// Handing the choice to the allocator is the only arrangement in which nothing else
        /// can take the buffer.
        ///
        /// **The contract with the host:** call this, write the payload at the returned
        /// pointer, pass that pointer to the section export, then call [`__dwow_dealloc`].
        /// The buffer must not be freed by the guest before then.
        ///
        /// # Safety
        /// Returns a pointer to `len` writable bytes owned by the guest, or a dangling
        /// pointer if `len` is 0. The caller must pass the same `len` to `__dwow_dealloc`.
        #[no_mangle]
        pub unsafe extern "C" fn __dwow_alloc(len: u32) -> *mut u8 {
            let mut buf = ::std::vec::Vec::<u8>::with_capacity(len as usize);
            let ptr = buf.as_mut_ptr();
            // Ownership moves to the caller, which returns it through `__dwow_dealloc`.
            // Dropping the `Vec` here would free the buffer the host is about to write.
            ::core::mem::forget(buf);
            ptr
        }

        /// Release a buffer from [`__dwow_alloc`].
        ///
        /// # Safety
        /// `ptr` and `len` must be exactly the values `__dwow_alloc` returned and was called
        /// with, and the buffer must not be used afterwards.
        #[no_mangle]
        pub unsafe extern "C" fn __dwow_dealloc(ptr: *mut u8, len: u32) {
            if !ptr.is_null() && len > 0 {
                // Reconstructed with length 0 and the original capacity: `__dwow_alloc`
                // `forget`s a `Vec` of capacity `len` that never held any elements.
                drop(::std::vec::Vec::from_raw_parts(ptr, 0, len as usize));
            }
        }
    };
}

#[macro_export]
macro_rules! define_contract {
    (
        init: $init_func:ident,
        exec: $exec_func:ident,
        apply: $apply_func:ident,
        metadata: $metadata_func:ident
    ) => {
        $crate::__contract_exports! {
            init: $init_func,
            exec: $exec_func,
            apply: $apply_func,
            metadata: $metadata_func
        }
    };
}

/// Like [`define_contract!`] but also generates a `__spend_hook` WASM export
/// for contracts that receive spend-hook callbacks from Promissory Note burns.
#[macro_export]
macro_rules! define_contract_with_spend_hook {
    (
        init: $init_func:ident,
        exec: $exec_func:ident,
        apply: $apply_func:ident,
        metadata: $metadata_func:ident,
        spend_hook: $spend_hook_func:ident
    ) => {
        $crate::__contract_exports! {
            init: $init_func,
            exec: $exec_func,
            apply: $apply_func,
            metadata: $metadata_func
        }

        /// # Safety
        #[no_mangle]
        pub unsafe extern "C" fn __spend_hook(input: *mut u8) -> i64 {
            let (contract_id, instruction_data) = match $crate::wasm::entrypoint::deserialize(input) {
                Ok(v) => v,
                Err(e) => return e.into(),
            };

            match $spend_hook_func(contract_id, &instruction_data) {
                Ok(()) => $crate::wasm::entrypoint::SUCCESS,
                Err(e) => e.into(),
            }
        }
    };
}

/// Deserialize a given payload in `entrypoint`
/// The return values from this are the input values for the above defined functions.
///
/// Returns a typed error rather than panicking: this fn is the first thing every generated
/// export calls, so a panic here is a panic location compiled into every contract artifact —
/// and the module carrying it is the one that decides whether a wasm carries panic machinery
/// at all (see contrib/wasm_artifact_check.sh).
///
/// # Safety
pub unsafe fn deserialize<'a>(input: *mut u8) -> Result<(ContractId, &'a [u8]), ContractError> {
    let mut offset: usize = 0;

    let contract_id_len = 32;
    let contract_id_slice = { from_raw_parts(input.add(offset), contract_id_len) };
    offset += contract_id_len;

    let instruction_data_len = *(input.add(offset) as *const u64) as usize;
    offset += size_of::<u64>();
    let instruction_data = { from_raw_parts(input.add(offset), instruction_data_len) };

    let Ok(contract_id_bytes) = <[u8; 32]>::try_from(contract_id_slice) else {
        return Err(ContractError::IoError(
            "deserialize: contract id slice is not 32 bytes".to_string(),
        ))
    };
    // `from_bytes` rejects a noncanonical encoding and the identity point, so this is the
    // canonicality check for the id the runtime handed us — previously an unwrap whose stated
    // reason was "invalid contract id is a runtime bug", i.e. a panic blamed on the caller.
    Ok((ContractId::from_bytes(contract_id_bytes)?, instruction_data))
}
