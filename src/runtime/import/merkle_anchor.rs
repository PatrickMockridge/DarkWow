/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use dwow_sdk::error::ContractError;
use tracing::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::runtime::vm_runtime::{ContractSection, Env, to_builtin};

/// Block-level Merkle tree anchoring host function.
///
/// Appends a contract state anchor (AnchorEntry) to the block-level Merkle tree.
/// The entry links the contract-local Merkle proof to the block header via a
/// shared nullifier.
///
/// Entry format: 96 bytes `[nullifier: 32B] [contract_id: 32B] [contract_root: 32B]`
///
/// ρ-calculus: ν(block_tree).ν(contract_tree).P — the nullifier is the extruded
/// name linking both restrictions. See contract-wasm-type-system.md Part C §C.3.7.
///
/// Permissions: Update
pub(crate) fn merkle_anchor_add(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    len: u32,
) -> i64 {
    let (env, store) = ctx.data_and_store_mut();

    // Enforce function ACL — Update section only (same as merkle_add)
    if let Err(e) = acl_allow(env, &[ContractSection::Update]) {
        return e;
    }

    // Validate length
    if len != 96 {
        return to_builtin!(ContractError::IoError(
            "merkle_anchor_add: expected 96 bytes".into()));
    }

    // Read entry bytes from WASM memory
    let memory = env.memory_view(&store);
    let slice = match ptr.slice(&memory, len) {
        Ok(s) => s,
        Err(e) => {
            error!("merkle_anchor_add: memory slice error: {:?}", e);
            return to_builtin!(ContractError::IoError("merkle_anchor_add: memory access".into()));
        }
    };

    let mut entry_bytes = [0u8; 96];
    for (i, cell) in slice.iter().enumerate() {
        entry_bytes[i] = cell.get();
    }

    // Append to block-level anchor tree via backend
    match env.backend.block_anchor_append(&entry_bytes) {
        Ok(()) => 0,
        Err(e) => to_builtin!(e),
    }
}
