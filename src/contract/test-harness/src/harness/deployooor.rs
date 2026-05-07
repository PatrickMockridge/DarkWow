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

//! Deployooor Test Harness
//!
//! Provides isolated testing for Deployooor contract (WASM contract deployer).
//!
//! Note: Deployooor has NO ZK circuits - it's a pure WASM contract that validates
//! and deploys other WASM contracts. This harness only tests the client API.

use darkfi::Result;
use darkfi_sdk::crypto::Keypair;

use darkfi_deployooor_contract::{
    client::{
        deploy_v1::{DeployCallBuilder, DeployCallDebris},
        lock_v1::{LockCallBuilder, LockCallDebris},
    },
    DeployFunction,
};

/// Deployooor Harness for isolated testing
///
/// Deployooor is a native contract that validates and deploys WASM contracts.
/// It has NO ZK circuits - all logic is in WASM.
pub struct DeployooorHarness {}

impl DeployooorHarness {
    /// Spawn a new Deployooor harness
    ///
    /// No ZK circuits to load - Deployooor is pure WASM.
    pub fn spawn() -> Self {
        Self {}
    }

    /// Get circuit namespaces
    ///
    /// Deployooor has no ZK circuits.
    pub fn circuits(&self) -> Vec<&'static str> {
        vec![]
    }

    /// Build a deploy call
    ///
    /// Creates a DeployV1 call to deploy a WASM contract.
    pub fn build_deploy_call(
        &self,
        deploy_keypair: Keypair,
        wasm_bincode: Vec<u8>,
        deploy_ix: Vec<u8>,
    ) -> Result<DeployCallDebris> {
        let builder = DeployCallBuilder { deploy_keypair, wasm_bincode, deploy_ix };
        builder.build()
    }

    /// Build a lock call
    ///
    /// Creates a LockV1 call to lock the deployer's public key.
    pub fn build_lock_call(&self, deploy_keypair: Keypair) -> Result<LockCallDebris> {
        let builder = LockCallBuilder { deploy_keypair };
        builder.build()
    }
}

/// Result of deploy call
pub struct DeployResult {
    pub wasm_bincode: Vec<u8>,
    pub public_key: darkfi_sdk::crypto::PublicKey,
    pub ix: Vec<u8>,
}

/// Result of lock call
pub struct LockResult {
    pub public_key: darkfi_sdk::crypto::PublicKey,
}
