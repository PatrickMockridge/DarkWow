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
