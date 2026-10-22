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

use dwow_core::{zk::ProvingKey, zkas::ZkBinary, Result};
use dwow_sdk::crypto::Keypair;

use dwow_deployooor_contract::client::{
    deploy_v1::{DeployCallBuilder, DeployCallDebris},
    lock_v1::{LockCallBuilder, LockCallDebris},
};

/// Deployooor Harness for isolated testing.
///
/// Deployooor is plaintext infrastructure: `DeployV1` and `LockV1` carry no
/// circuit. Authorization is the transaction signature over the deploy key.
pub struct DeployooorHarness;

impl DeployooorHarness {
    /// Spawn a new Deployooor harness.
    pub fn spawn() -> Self {
        Self
    }

    /// Build a deploy call.
    pub fn build_deploy_call(
        &self,
        deploy_keypair: Keypair,
        wasm_bincode: Vec<u8>,
        deploy_ix: Vec<u8>,
    ) -> Result<DeployCallDebris> {
        let builder = DeployCallBuilder { deploy_keypair, wasm_bincode, deploy_ix };
        builder.build()
    }

    /// Build a `LockV1` call over the deployer's keypair.
    pub fn build_lock_call(&self, deploy_keypair: Keypair) -> Result<LockCallDebris> {
        let builder = LockCallBuilder { deploy_keypair };
        builder.build()
    }
}

impl super::ContractHarness for DeployooorHarness {
    fn name(&self) -> &str {
        "deployooor"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![]
    }

    fn get_zkbin(&self, _ns: &str) -> Option<&ZkBinary> {
        None
    }

    fn get_pk(&self, _ns: &str) -> Option<&ProvingKey> {
        None
    }
}
