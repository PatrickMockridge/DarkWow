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
    crypto::ContractId, dark_tree::DarkLeaf, error::ContractResult, wasm, ContractCall,
};
use dwow_serial::deserialize;

use crate::{
    model::{DeployUpdateV1, LockUpdateV1},
    DeployFunction, DEPLOY_CONTRACT_DB_VERSION, DEPLOY_CONTRACT_INFO_TREE,
    DEPLOY_CONTRACT_LOCK_TREE,
};

/// `Deployooor::Deploy` functions
mod deploy;
use deploy::{deploy_get_metadata_v1, deploy_process_instruction_v1, deploy_process_update_v1};

/// `Deployooor::Lock` functions
mod lock;
use lock::{lock_get_metadata_v1, lock_process_instruction_v1, lock_process_update_v1};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// This entrypoint function runs when the contract is (re)deployed and initialized.
/// We use this function to initialize all the necessary databases and prepare them
/// with initial data if necessary.
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Set up a database tree for arbitrary data
    let info_db = match wasm::db::db_lookup(cid, DEPLOY_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DEPLOY_CONTRACT_INFO_TREE)?,
    };

    // Set up a database to hold the set of locked contracts
    // k=ContractId, v=bool
    if wasm::db::db_lookup(cid, DEPLOY_CONTRACT_LOCK_TREE).is_err() {
        wasm::db::db_init(cid, DEPLOY_CONTRACT_LOCK_TREE)?;
    }

    // Update db version
    wasm::db::db_set(info_db, DEPLOY_CONTRACT_DB_VERSION, env!("CARGO_PKG_VERSION").as_bytes())?;

    Ok(())
}

/// This function is used by the wasm VM's host to fetch the necessary metadata
/// for verifying signatures and zk proofs. The payload given here are all the
/// contract calls in the transaction.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DeployFunction::try_from(self_.data[0])?;

    let metadata = match func {
        DeployFunction::DeployV1 => deploy_get_metadata_v1(cid, call_idx, calls)?,
        DeployFunction::LockV1 => lock_get_metadata_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&metadata)
}

/// This function verifies a state transition and produces a state update
/// if everything is successful.
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = DeployFunction::try_from(func_byte)?;

    let update_data = match func {
        DeployFunction::DeployV1 => deploy_process_instruction_v1(cid, call_idx, calls)?,
        DeployFunction::LockV1 => lock_process_instruction_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&[&[func_byte], &update_data[..]].concat())
}

/// This function attempts to write a given state update provided the previous
/// steps of the contract call execution were all successful. It's the last in
/// line, and assumes that the transaction/call was successful. The payload
/// given to the function is the update data retrieved from `process_instruction()`,
/// prefixed with the contract function.
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DeployFunction::try_from(update_data[0])? {
        DeployFunction::DeployV1 => {
            let update = DeployUpdateV1::decode(&update_data[1..])?;
            Ok(deploy_process_update_v1(cid, update)?)
        }

        DeployFunction::LockV1 => {
            let update = LockUpdateV1::decode(&update_data[1..])?;
            Ok(lock_process_update_v1(cid, update)?)
        }
    }
}
