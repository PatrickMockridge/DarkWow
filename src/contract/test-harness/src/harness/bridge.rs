/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Bridge Test Harness
//!
//! Provides isolated testing for Bridge contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use darkfi_bridge_contract::client::deposit_v1::{DepositCallData, create_deposit_proof};

/// Bridge Harness for isolated testing
pub struct BridgeHarness {
    /// Deposit_V1 ZkBinary
    deposit_zkbin: ZkBinary,
    /// Deposit_V1 ProvingKey
    deposit_pk: ProvingKey,
}

impl BridgeHarness {
    /// Spawn a new Bridge harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let deposit_bin = include_bytes!("../../../bridge/proof/deposit_v1.zk.bin");

        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();

        let deposit_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&deposit_zkbin).unwrap(),
            &deposit_zkbin,
        );

        let deposit_pk = ProvingKey::build(deposit_zkbin.k, &deposit_circuit);

        Self { deposit_zkbin, deposit_pk }
    }
}

impl super::ContractHarness for BridgeHarness {
    fn name(&self) -> &str {
        "bridge"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["DepositV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "DepositV1" => Some(&self.deposit_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "DepositV1" => Some(&self.deposit_pk),
            _ => None,
        }
    }
}