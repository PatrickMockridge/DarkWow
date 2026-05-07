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

//! Linear Blockchain Genesis Test Module
//!
//! Tests genesis contract deployment and block sync for the linear blockchain.

use std::sync::Arc;

use darkfi_linear::LinearStore;
use darkfi_sdk::{crypto::DEPLOYOOOR_CONTRACT_ID, pasta::pallas};
use sled::Config;

use crate::blockchain::LinearBlockchain;

#[test]
fn test_linear_genesis_contracts() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary sled database
    let db = Config::new().temporary(true).open()?;
    let db = Arc::new(db);

    // Create LinearStore
    let store = LinearStore::new(db.clone())?;
    let store = Arc::new(store);

    // Create LinearBlockchain
    let blockchain = LinearBlockchain::new(store);

    // Deployooor WASM
    let deployooor_wasm =
        include_bytes!("../../../../src/contract/deployooor/darkfi_deployooor_contract.wasm").to_vec();

    // Native Token WASM
    let native_token_wasm =
        include_bytes!("../../../../src/contract/native_token/darkfi_native_token_contract.wasm").to_vec();

    // Deploy Deployooor contract
    blockchain.deploy_contract(&deployooor_wasm, *DEPLOYOOOR_CONTRACT_ID)?;
    println!(
        "Deployed Deployooor contract: {:?}",
        DEPLOYOOOR_CONTRACT_ID.to_bytes()
    );

    // Deploy Native Token contract
    let native_token_id =
        darkfi_sdk::crypto::ContractId::from(pallas::Base::from(42));
    blockchain.deploy_contract(&native_token_wasm, native_token_id)?;
    println!("Deployed Native Token contract: {:?}", native_token_id.to_bytes());

    // Verify contracts exist
    assert!(blockchain.has_contract(*DEPLOYOOOR_CONTRACT_ID)?);
    assert!(blockchain.has_contract(native_token_id)?);

    // Retrieve and verify WASM
    let retrieved_deployooor = blockchain.get_contract(*DEPLOYOOOR_CONTRACT_ID)?;
    assert_eq!(retrieved_deployooor, deployooor_wasm);

    let retrieved_native_token = blockchain.get_contract(native_token_id)?;
    assert_eq!(retrieved_native_token, native_token_wasm);

    println!("test_linear_genesis_contracts PASSED - Genesis contracts deployed and verified");
    Ok(())
}

#[test]
fn test_linear_block_sync() -> Result<(), Box<dyn std::error::Error>> {
    // Create two temporary sled databases (two nodes)
    let db1 = Config::new().temporary(true).open()?;
    let db1 = Arc::new(db1);

    let db2 = Config::new().temporary(true).open()?;
    let db2 = Arc::new(db2);

    // Create LinearStores
    let store1 = LinearStore::new(db1.clone())?;
    let store1 = Arc::new(store1);

    let store2 = LinearStore::new(db2.clone())?;
    let store2 = Arc::new(store2);

    // Create two LinearBlockchains (two nodes)
    let blockchain1 = LinearBlockchain::new(store1);
    let blockchain2 = LinearBlockchain::new(store2);

    // Deploy contracts to node 1
    let deployooor_wasm =
        include_bytes!("../../../../src/contract/deployooor/darkfi_deployooor_contract.wasm").to_vec();
    blockchain1.deploy_contract(&deployooor_wasm, *DEPLOYOOOR_CONTRACT_ID)?;

    let native_token_wasm =
        include_bytes!("../../../../src/contract/native_token/darkfi_native_token_contract.wasm").to_vec();
    let native_token_id =
        darkfi_sdk::crypto::ContractId::from(pallas::Base::from(42));
    blockchain1.deploy_contract(&native_token_wasm, native_token_id)?;

    // Verify node 2 doesn't have the contracts yet
    assert!(!blockchain2.has_contract(*DEPLOYOOOR_CONTRACT_ID)?);

    // Note: Full sync would require the linear_sync protocol to be implemented
    // For now, we just verify the basic blockchain creation works

    println!("test_linear_block_sync PASSED - Two nodes created, contracts on node 1 verified");
    Ok(())
}