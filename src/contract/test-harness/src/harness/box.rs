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

//! Box Test Harness
//!
//! Provides isolated testing for the Box contract (put/take circuits).

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Box Harness for isolated testing
pub struct BoxHarness {
    /// PutV1 ZkBinary
    put_zkbin: ZkBinary,
    /// PutV1 ProvingKey
    put_pk: ProvingKey,
    /// TakeV1 ZkBinary
    take_zkbin: ZkBinary,
    /// TakeV1 ProvingKey
    take_pk: ProvingKey,
}

impl BoxHarness {
    /// Spawn a new Box harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let put_bin = include_bytes!("../../../box/proof/put_v1.zk.bin");
        let take_bin = include_bytes!("../../../box/proof/take_v1.zk.bin");

        let put_zkbin = ZkBinary::decode(put_bin, false).unwrap();
        let take_zkbin = ZkBinary::decode(take_bin, false).unwrap();

        // Build proving keys
        let put_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&put_zkbin).unwrap(), &put_zkbin);
        let take_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&take_zkbin).unwrap(), &take_zkbin);

        let put_pk = ProvingKey::build(put_zkbin.k, &put_circuit).expect("ProvingKey::build failed");
        let take_pk = ProvingKey::build(take_zkbin.k, &take_circuit).expect("ProvingKey::build failed");

        Self {
            put_zkbin,
            put_pk,
            take_zkbin,
            take_pk,
        }
    }

    pub fn put(&self) -> dwow_core::Result<BoxPutResult> {
        let w = dwow_core::zk::empty_witnesses(&self.put_zkbin)?;
        let c = ZkCircuit::new(w, &self.put_zkbin);
        let proof = Proof::create(&self.put_pk, &[c], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;
        Ok(BoxPutResult { call_data: vec![0x01], proof })
    }

    pub fn take(&self) -> dwow_core::Result<BoxTakeResult> {
        let w = dwow_core::zk::empty_witnesses(&self.take_zkbin)?;
        let c = ZkCircuit::new(w, &self.take_zkbin);
        let proof = Proof::create(&self.take_pk, &[c], &[], rand::rngs::OsRng)
            .map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;
        Ok(BoxTakeResult { call_data: vec![0x02], proof })
    }
}

impl super::ContractHarness for BoxHarness {
    fn name(&self) -> &str {
        "box"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["PutV1", "TakeV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "PutV1" => Some(&self.put_zkbin),
            "TakeV1" => Some(&self.take_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "PutV1" => Some(&self.put_pk),
            "TakeV1" => Some(&self.take_pk),
            _ => None,
        }
    }
}

pub struct BoxPutResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct BoxTakeResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
