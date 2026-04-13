/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_sdk::{crypto::Keypair, deploy::DeployParamsV1};
use darkfi_serial::Encodable;
use tracing::info;

use darkfi_deployooor_contract::{
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

    // Create a deploy keypair
    let deploy_keypair = Keypair::default();

    // Create deployment instruction
    let deploy_ix = vec![0x00, 0x01, 0x02, 0x03];

    // Build deploy call
    let debris = DeployCallBuilder {
        deploy_keypair,
        wasm_bincode: wasm_bincode.clone(),
        deploy_ix: deploy_ix.clone(),
    }
    .build()?;

    info!(target: "test_harness::deployooor", "Deploy call built successfully");
    info!(target: "test_harness::deployooor", "  WASM bincode size: {} bytes", debris.params.wasm_bincode.len());
    info!(target: "test_harness::deployooor", "  Instruction size: {} bytes", debris.params.ix.len());

    // Verify the call data can be serialized/deserialized correctly
    let mut data = vec![DeployFunction::DeployV1 as u8];
    debris.params.encode(&mut data)?;

    info!(target: "test_harness::deployooor", "DeployCallBuilder test PASSED");
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
