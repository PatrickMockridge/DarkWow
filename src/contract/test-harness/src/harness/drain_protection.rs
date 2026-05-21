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

//! DrainProtection Test Harness
//!
//! Provides isolated testing for DrainProtection contract.
//!
//! Note: drain_protection has an exit_v1 ZK circuit loaded, but the client proof
//! generation module is not yet implemented. This harness exposes the circuit and
//! proving key via the ContractHarness trait for direct use in tests.

use dwow::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};

/// DrainProtection Harness for isolated testing
pub struct DrainProtectionHarness {
    /// Exit_V1 ZkBinary
    exit_zkbin: ZkBinary,
    /// Exit_V1 ProvingKey
    exit_pk: ProvingKey,
}

impl DrainProtectionHarness {
    /// Spawn a new DrainProtection harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let exit_bin = include_bytes!("../../../drain_protection/proof/exit_v1.zk.bin");

        let exit_zkbin = ZkBinary::decode(exit_bin, false).unwrap();

        let exit_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&exit_zkbin).unwrap(),
            &exit_zkbin,
        );

        let exit_pk = ProvingKey::build(exit_zkbin.k, &exit_circuit);

        Self {
            exit_zkbin,
            exit_pk,
        }
    }

    /// Build exit call data
    ///
    /// Client proof module not yet implemented — the ExitParamsV1 model type
    /// must be constructed directly from `dwow_drain_protection_contract::model`.
    /// The ZK circuit and proving key are available via the ContractHarness trait.
    pub fn build_exit_call_data(
        &self,
        params: &dwow_drain_protection_contract::model::ExitParamsV1,
    ) -> Result<Vec<u8>> {
        use dwow_serial::Encodable;
        let mut call_data = vec![];
        params.encode(&mut call_data)?;
        Ok(call_data)
    }

    /// Build initialize call data
    pub fn build_initialize_call_data(
        &self,
        params: &dwow_drain_protection_contract::model::InitializeParamsV1,
    ) -> Result<Vec<u8>> {
        use dwow_serial::Encodable;
        let mut call_data = vec![];
        params.encode(&mut call_data)?;
        Ok(call_data)
    }
}

impl super::ContractHarness for DrainProtectionHarness {
    fn name(&self) -> &str {
        "drain_protection"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["DRAIN_PROTECTION_EXIT"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "DRAIN_PROTECTION_EXIT" => Some(&self.exit_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "DRAIN_PROTECTION_EXIT" => Some(&self.exit_pk),
            _ => None,
        }
    }
}
