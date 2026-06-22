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

//! Bearer Bond Test Harness

use dwow_core::{
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Bearer Bond Harness for isolated testing
pub struct BearerBondHarness {
    blind_output_zkbin: ZkBinary,
    blind_output_pk: ProvingKey,
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    redeem_zkbin: ZkBinary,
    redeem_pk: ProvingKey,
    prove_coverage_zkbin: ZkBinary,
    prove_coverage_pk: ProvingKey,
}

impl BearerBondHarness {
    pub fn spawn() -> Self {
        let blind_output_bin = include_bytes!("../../../bearer_bond/proof/blind_output_v1.zk.bin");
        let burn_bin = include_bytes!("../../../bearer_bond/proof/burn_v1.zk.bin");
        let redeem_bin = include_bytes!("../../../bearer_bond/proof/redeem_v1.zk.bin");
        let prove_coverage_bin = include_bytes!("../../../bearer_bond/proof/prove_coverage_v1.zk.bin");

        let blind_output_zkbin = ZkBinary::decode(blind_output_bin, false).unwrap();
        let blind_output_circuit = ZkCircuit::new(empty_witnesses(&blind_output_zkbin).unwrap(), &blind_output_zkbin);
        let blind_output_pk = ProvingKey::build(blind_output_zkbin.k, &blind_output_circuit);
        let burn_zkbin = ZkBinary::decode(burn_bin, false).unwrap();
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);
        let redeem_zkbin = ZkBinary::decode(redeem_bin, false).unwrap();
        let redeem_circuit = ZkCircuit::new(empty_witnesses(&redeem_zkbin).unwrap(), &redeem_zkbin);
        let redeem_pk = ProvingKey::build(redeem_zkbin.k, &redeem_circuit);
        let prove_coverage_zkbin = ZkBinary::decode(prove_coverage_bin, false).unwrap();
        let prove_coverage_circuit = ZkCircuit::new(empty_witnesses(&prove_coverage_zkbin).unwrap(), &prove_coverage_zkbin);
        let prove_coverage_pk = ProvingKey::build(prove_coverage_zkbin.k, &prove_coverage_circuit);

        Self {
            blind_output_zkbin, blind_output_pk,
            burn_zkbin, burn_pk,
            redeem_zkbin, redeem_pk,
            prove_coverage_zkbin, prove_coverage_pk,
        }
    }
}

impl super::ContractHarness for BearerBondHarness {
    fn name(&self) -> &str {
        "bearer_bond"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["Burn_V1", "BlindOutput_V1", "Redeem_V1", "ProveCoverage_V1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "Burn_V1" => Some(&self.burn_zkbin),
            "BlindOutput_V1" => Some(&self.blind_output_zkbin),
            "Redeem_V1" => Some(&self.redeem_zkbin),
            "ProveCoverage_V1" => Some(&self.prove_coverage_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "Burn_V1" => Some(&self.burn_pk),
            "BlindOutput_V1" => Some(&self.blind_output_pk),
            "Redeem_V1" => Some(&self.redeem_pk),
            "ProveCoverage_V1" => Some(&self.prove_coverage_pk),
            _ => None,
        }
    }
}
