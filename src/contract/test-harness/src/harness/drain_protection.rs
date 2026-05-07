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

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
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
            darkfi::zk::empty_witnesses(&exit_zkbin).unwrap(),
            &exit_zkbin,
        );

        let exit_pk = ProvingKey::build(exit_zkbin.k, &exit_circuit);

        Self {
            exit_zkbin,
            exit_pk,
        }
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
