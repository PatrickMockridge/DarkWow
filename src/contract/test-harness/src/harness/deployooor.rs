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

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Keypair},
    pasta::pallas,
};

use dwow_deployooor_contract::client::{
    deploy_v1::{DeployCallBuilder, DeployCallDebris},
    lock_v1::{build_lock_call, LockPlan},
};

/// Deployooor Harness for isolated testing
///
/// `DeployV1` is native WASM with no circuit. `LockV1` carries the `LockV2` proof that the caller
/// knows the deployer's secret (docs-review F-511).
pub struct DeployooorHarness {
    lock_zkbin: ZkBinary,
    lock_pk: ProvingKey,
}

impl DeployooorHarness {
    /// Spawn a new Deployooor harness, loading the `LockV2` circuit.
    pub fn spawn() -> Self {
        let lock_bin: &[u8] = include_bytes!("../../../deployooor/proof/lock.zk.bin");
        let lock_zkbin = ZkBinary::decode(lock_bin, false).unwrap();
        let lock_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&lock_zkbin).unwrap(), &lock_zkbin);
        let lock_pk =
            ProvingKey::build(lock_zkbin.k, &lock_circuit).expect("ProvingKey::build failed");
        Self { lock_zkbin, lock_pk }
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
        let builder = DeployCallBuilder {
            deploy_keypair,
            wasm_bincode,
            deploy_ix,
            singleton: false,
            singleton_name: String::new(),
        };
        builder.build()
    }

    /// Prepare a `LockV1` call over the deployer's keypair. The caller proves it against the frame
    /// commitment (`plan.prove(frame)`) once the call's frame is known.
    pub fn build_lock_call(
        &self,
        deploy_keypair: Keypair,
        tx_nonce: pallas::Base,
    ) -> Result<LockPlan> {
        let deployer_secret = *deploy_keypair.secret.inner();
        build_lock_call(
            deploy_keypair,
            deployer_secret,
            tx_nonce,
            self.lock_zkbin.clone(),
            self.lock_pk.clone(),
        )
    }
}

impl super::ContractHarness for DeployooorHarness {
    fn name(&self) -> &str {
        "deployooor"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["LockV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        (ns == "LockV2").then_some(&self.lock_zkbin)
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        (ns == "LockV2").then_some(&self.lock_pk)
    }
}

/// Result of deploy call
pub struct DeployResult {
    pub wasm_bincode: Vec<u8>,
    pub public_key: dwow_sdk::crypto::PublicKey,
    pub ix: Vec<u8>,
}

/// Result of lock call
pub struct LockResult {
    pub public_key: dwow_sdk::crypto::PublicKey,
}
