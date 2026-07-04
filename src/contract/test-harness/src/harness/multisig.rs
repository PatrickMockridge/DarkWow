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

//! MultiSig Test Harness
//!
//! Provides isolated testing for MultiSig contract (multisig wallets).

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// MultiSig Harness for isolated testing
pub struct MultiSigHarness {
    /// CreateGroupV1 ZkBinary
    create_group_zkbin: ZkBinary,
    /// CreateGroupV1 ProvingKey
    create_group_pk: ProvingKey,
    /// FinalizeV1 ZkBinary
    finalize_zkbin: ZkBinary,
    /// FinalizeV1 ProvingKey
    finalize_pk: ProvingKey,
    /// SignV1 ZkBinary
    sign_zkbin: ZkBinary,
    /// SignV1 ProvingKey
    sign_pk: ProvingKey,
}

impl MultiSigHarness {
    /// Spawn a new MultiSig harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let create_group_bin =
            include_bytes!("../../../multisig/proof/create_group_v1.zk.bin");
        let finalize_bin = include_bytes!("../../../multisig/proof/finalize_v1.zk.bin");
        let sign_bin = include_bytes!("../../../multisig/proof/sign_v1.zk.bin");

        let create_group_zkbin = ZkBinary::decode(create_group_bin, false).unwrap();
        let finalize_zkbin = ZkBinary::decode(finalize_bin, false).unwrap();
        let sign_zkbin = ZkBinary::decode(sign_bin, false).unwrap();

        // Build proving keys
        let create_group_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_group_zkbin).unwrap(),
            &create_group_zkbin,
        );
        let finalize_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&finalize_zkbin).unwrap(),
            &finalize_zkbin,
        );
        let sign_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&sign_zkbin).unwrap(),
            &sign_zkbin,
        );

        let create_group_pk =
            ProvingKey::build(create_group_zkbin.k, &create_group_circuit)
                .expect("ProvingKey::build failed");
        let finalize_pk =
            ProvingKey::build(finalize_zkbin.k, &finalize_circuit)
                .expect("ProvingKey::build failed");
        let sign_pk =
            ProvingKey::build(sign_zkbin.k, &sign_circuit)
                .expect("ProvingKey::build failed");

        Self {
            create_group_zkbin,
            create_group_pk,
            finalize_zkbin,
            finalize_pk,
            sign_zkbin,
            sign_pk,
        }
    }
}

impl super::ContractHarness for MultiSigHarness {
    fn name(&self) -> &str {
        "multisig"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateGroupV1", "FinalizeV1", "SignV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateGroupV1" => Some(&self.create_group_zkbin),
            "FinalizeV1" => Some(&self.finalize_zkbin),
            "SignV1" => Some(&self.sign_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateGroupV1" => Some(&self.create_group_pk),
            "FinalizeV1" => Some(&self.finalize_pk),
            "SignV1" => Some(&self.sign_pk),
            _ => None,
        }
    }
}
