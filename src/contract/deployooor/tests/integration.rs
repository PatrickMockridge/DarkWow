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

//! Integration test for the Deployooor contract.
//!
//! Tests the deploy/lock lifecycle using the contract test harness:
//!   1. Build a deploy call for a WASM contract
//!   2. Build a lock call
//!   3. Negative: building a deploy call with empty WASM fails

use dwow_contract_test_harness::{harness::DeployooorHarness, init_logger};
use dwow_sdk::{crypto::{Keypair, SecretKey}, pasta::pallas};
use tracing::info;

#[test]
fn deploy_integration() {
    init_logger();
    info!(target: "deploy", "Starting Deployooor integration test");

    let harness = DeployooorHarness::spawn();
    let deploy_keypair = Keypair::new(SecretKey::from_base(pallas::Base::from(42)));

    let wasm_bincode = vec![
        0x00, 0x61, 0x73, 0x6d, // magic number
        0x01, 0x00, 0x00, 0x00, // version
    ];

    // Build a deploy call
    info!(target: "deploy", "Building deploy call");
    let deploy_result = harness.build_deploy_call(
        deploy_keypair.clone(),
        wasm_bincode.clone(),
        vec![0x00, 0x01, 0x02, 0x03],
    );
    assert!(deploy_result.is_ok(), "Deploy call should build successfully");

    // Build another deploy call (simulates replacing the deployed contract)
    info!(target: "deploy", "Building second deploy call (replacement)");
    let deploy_result2 = harness.build_deploy_call(
        deploy_keypair.clone(),
        wasm_bincode.clone(),
        vec![0x00, 0x01, 0x02, 0x03],
    );
    assert!(deploy_result2.is_ok(), "Second deploy call should build successfully");

    // Build a lock call
    info!(target: "deploy", "Building lock call");
    let lock_result = harness.build_lock_call(deploy_keypair.clone());
    assert!(lock_result.is_ok(), "Lock call should build successfully");

    // Negative: building a deploy call with empty WASM must fail
    info!(target: "deploy", "Verifying empty WASM is rejected");
    let empty_wasm_result = harness.build_deploy_call(
        deploy_keypair.clone(),
        vec![],
        vec![0x00, 0x01, 0x02, 0x03],
    );
    assert!(
        empty_wasm_result.is_err(),
        "Deploy call with empty WASM should fail"
    );

    // Negative: building a deploy call with empty WASM and empty deploy_ix
    info!(target: "deploy", "Verifying empty WASM with empty deploy_ix is rejected");
    let empty_both_result = harness.build_deploy_call(
        deploy_keypair.clone(),
        vec![],
        vec![],
    );
    assert!(
        empty_both_result.is_err(),
        "Deploy call with empty WASM and empty deploy_ix should fail"
    );

    info!(target: "deploy", "All integration tests PASSED");
}
