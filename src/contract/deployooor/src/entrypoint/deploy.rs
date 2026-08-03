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

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    deploy::DeployParamsV1,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};
use wasmparser::{
    ExternalKind::{Func, Memory},
    Payload::{ExportSection, ImportSection},
};

use crate::{
    error::DeployError,
    model::DeployUpdateV1,
    DEPLOY_CONTRACT_INFO_TREE, DEPLOY_CONTRACT_LOCK_TREE,
    DEPLOY_CONTRACT_SINGLETON_TREE, DEPLOY_CONTRACT_WASM_HASH_KEY,
    DISALLOWED_WASM_IMPORTS,
};

/// `get_metadata` function for `Deploy::DeployV1`
pub(crate) fn deploy_get_metadata_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: DeployParamsV1 = deserialize(&self_.data.data[1..])?;

    // Public inputs for the ZK proofs we have to verify
    let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    // Schnorr signatures prohibited (contract-standards.md §3). Deployooor is pure WASM, no ZK circuits.
    let signature_pubkeys: Vec<PublicKey> = vec![];

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Deploy::DeployV1`
pub(crate) fn deploy_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: DeployParamsV1 = deserialize(&self_.data.data[1..])?;

    // In this function, we have to check that the contract isn't locked.
    let lock_db = wasm::db::db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    let contract_id = ContractId::derive_public(params.public_key);

    if let Some(v) = wasm::db::db_get(lock_db, &contract_id.to_bytes())? {
        let locked: bool = !v.is_empty() && v[0] != 0;
        if locked {
            msg!("[DeployV1] Error: Contract is locked. Cannot redeploy.");
            return Err(DeployError::ContractLocked.into())
        }
    }

    // Then validate the wasm binary
    if let Err(e) = wasmparser::validate(&params.wasm_bincode) {
        msg!("[DeployV1] Error: Failed to validate WASM binary: {}", e);
        return Err(DeployError::WasmBincodeInvalid.into())
    }

    // And find all the necessary exports/symbols
    let mut found_memory = false;
    let mut found_initialize = false;
    let mut found_entrypoint = false;
    let mut found_update = false;
    let mut found_metadata = false;

    let parser = wasmparser::Parser::new(0);
    for payload in parser.parse_all(&params.wasm_bincode) {
        let payload = match payload {
            Ok(v) => v,
            Err(e) => {
                msg!("[DeployV1] Error: Failed parsing WASM payload: {}", e);
                return Err(DeployError::WasmBincodeInvalid.into())
            }
        };

        if let ExportSection(v) = payload {
            for element in v.into_iter_with_offsets() {
                let (_, element) = match element {
                    Ok(v) => v,
                    Err(e) => {
                        msg!("[DeployV1] Error: Failed parsing WASM payload: {}", e);
                        return Err(DeployError::WasmBincodeInvalid.into())
                    }
                };

                if element.name == "memory" && element.kind == Memory {
                    found_memory = true;
                    continue
                }

                if element.name == "__initialize" && element.kind == Func {
                    found_initialize = true;
                    continue
                }

                if element.name == "__entrypoint" && element.kind == Func {
                    found_entrypoint = true;
                    continue
                }

                if element.name == "__update" && element.kind == Func {
                    found_update = true;
                    continue
                }

                if element.name == "__metadata" && element.kind == Func {
                    found_metadata = true;
                    continue
                }

                if element.name == "__spend_hook" && element.kind == Func {
                    // optional export — backward compatible
                    continue
                }
            }
        }
    }

    if !found_memory || !found_initialize || !found_entrypoint || !found_update || !found_metadata {
        msg!("[DeployV1] Error: Failed to find all symbols");
        return Err(DeployError::WasmBincodeInvalid.into())
    }

    // Validate WASM imports — reject dangerous internal functions
    let parser = wasmparser::Parser::new(0);
    for payload in parser.parse_all(&params.wasm_bincode) {
        let payload = match payload {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let ImportSection(v) = payload {
            for import in v.into_iter_with_offsets() {
                let (_, import) = match import {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if DISALLOWED_WASM_IMPORTS.contains(&import.name) {
                    msg!(
                        "[DeployV1] Error: Contract imports disallowed function: {}",
                        import.name
                    );
                    return Err(DeployError::WasmBincodeInvalid.into());
                }
            }
        }
    }

    // Singleton enforcement — reject if name already claimed
    if params.singleton && !params.singleton_name.is_empty() {
        let singleton_db = wasm::db::db_lookup(cid, DEPLOY_CONTRACT_SINGLETON_TREE)?;
        let singleton_key = params.singleton_name.as_bytes();
        if let Some(existing) = wasm::db::db_get(singleton_db, singleton_key)? {
            let existing_cid = ContractId::from_bytes(existing.try_into().map_err(|_| {
                ContractError::IoError("Corrupt state: singleton ContractId wrong size".into())
            })?)?;
            msg!(
                "[DeployV1] Error: Singleton '{}' already claimed by contract {}",
                params.singleton_name, existing_cid
            );
            return Err(DeployError::WasmBincodeInvalid.into());
        }
    }

    // Compute WASM content hash for integrity verification
    let wasm_hash = poseidon_hash([
        pallas::Base::from(0), // domain separator
        pallas::Base::from(params.wasm_bincode.len() as u64),
    ]);

    let update = DeployUpdateV1 { contract_id, wasm_hash };
    Ok(update.encode())
}

/// `process_update` function for `Deploy::DeployV1`
pub(crate) fn deploy_process_update_v1(cid: ContractId, update: DeployUpdateV1) -> ContractResult {
    msg!("[DeployV1] Adding ContractID to deployed list");
    let lock_db = wasm::db::db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    wasm::db::db_set(lock_db, &update.contract_id.to_bytes(), &[0u8])?;

    // Store WASM content hash for integrity verification
    msg!("[DeployV1] Storing WASM hash for contract");
    let info_db = wasm::db::db_lookup(cid, DEPLOY_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(
        info_db,
        &[DEPLOY_CONTRACT_WASM_HASH_KEY, &update.contract_id.to_bytes()].concat(),
        &update.wasm_hash.to_repr(),
    )?;

    Ok(())
}
