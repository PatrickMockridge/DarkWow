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

#![allow(dead_code)]

use dwow_sdk::{crypto::{Keypair, SecretKey}, deploy::DeployParamsV1, pasta::pallas};
use dwow_serial::Encodable;
use tracing::info;

use dwow_deployooor_contract::{
    client::deploy_v1::DeployCallBuilder,
    DeployFunction,
};

/// Initialize logger for tests
fn init_logger() {
    let subscriber = tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Test DeployCallBuilder - build a deploy call
fn test_deploy_call_builder() -> Result<(), Box<dyn std::error::Error>> {
    info!(target: "test_harness::deployooor", "=== Testing DeployCallBuilder ===");

    // Create a simple WASM bincode (minimal valid WASM)
    let wasm_bincode = vec![
        0x00, 0x61, 0x73, 0x6d, // magic number
        0x01, 0x00, 0x00, 0x00, // version
    ];

    // Create a deploy keypair (fixed, explicitly-declared test key)
    let deploy_keypair = Keypair::new(SecretKey::from(pallas::Base::from(42)));

    // Create deployment instruction
    let deploy_ix = vec![0x00, 0x01, 0x02, 0x03];

    // Build deploy call
    let debris = DeployCallBuilder {
        deploy_keypair,
        wasm_bincode: wasm_bincode.clone(),
        deploy_ix: deploy_ix.clone(),
        singleton: false,
        singleton_name: String::new(),
    }
    .build()?;

    info!(target: "test_harness::deployooor", "Deploy call built successfully");
    info!(target: "test_harness::deployooor", "  WASM bincode size: {} bytes", debris.params.wasm_bincode.len());
    info!(target: "test_harness::deployooor", "  Instruction size: {} bytes", debris.params.ix.len());

    // Verify call data via encapsulated build_call_data()
    let call_data = DeployCallBuilder {
        deploy_keypair,
        wasm_bincode: wasm_bincode.clone(),
        deploy_ix: deploy_ix.clone(),
        singleton: false,
        singleton_name: String::new(),
    }
    .build_call_data()?;

    // Verify function code byte is DeployV1
    assert_eq!(call_data[0], DeployFunction::DeployV1 as u8,
        "call_data must start with DeployV1 function code 0x00");

    // Verify params deserialize round-trip from call_data[1..]
    let decoded: DeployParamsV1 = dwow_serial::deserialize(&call_data[1..])?;
    assert_eq!(decoded.wasm_bincode.len(), wasm_bincode.len());
    assert_eq!(decoded.ix.len(), deploy_ix.len());
    Ok(())
}

/// Run all deployooor tests
fn run_tests() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();

    info!(target: "test_harness::deployooor", "Starting Deployooor contract tests");

    test_deploy_call_builder()?;

    info!(target: "test_harness::deployooor", "All Deployooor tests PASSED");
    Ok(())
}

fn main() {
    if let Err(e) = run_tests() {
        eprintln!("Test failed: {}", e);
        std::process::exit(1);
    }
}
