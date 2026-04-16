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

//! RelayerEndowment Test Harness
//!
//! Provides isolated testing for RelayerEndowment contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// RelayerEndowment Harness for isolated testing
pub struct RelayerEndowmentHarness {
    /// Initialize_V1 ZkBinary
    initialize_zkbin: ZkBinary,
    /// Initialize_V1 ProvingKey
    initialize_pk: ProvingKey,
    /// DeployCapital_V1 ZkBinary
    deploy_capital_zkbin: ZkBinary,
    /// DeployCapital_V1 ProvingKey
    deploy_capital_pk: ProvingKey,
    /// ClaimFees_V1 ZkBinary
    claim_fees_zkbin: ZkBinary,
    /// ClaimFees_V1 ProvingKey
    claim_fees_pk: ProvingKey,
}

impl RelayerEndowmentHarness {
    /// Spawn a new RelayerEndowment harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let init_bin = include_bytes!("../../../relayer_endowment/proof/initialize_v1.zk.bin");
        let deploy_bin = include_bytes!("../../../relayer_endowment/proof/deploy_capital_v1.zk.bin");
        let claim_bin = include_bytes!("../../../relayer_endowment/proof/claim_fees_v1.zk.bin");

        let initialize_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let deploy_capital_zkbin = ZkBinary::decode(deploy_bin, false).unwrap();
        let claim_fees_zkbin = ZkBinary::decode(claim_bin, false).unwrap();

        let init_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&initialize_zkbin).unwrap(),
            &initialize_zkbin,
        );
        let deploy_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&deploy_capital_zkbin).unwrap(),
            &deploy_capital_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_fees_zkbin).unwrap(),
            &claim_fees_zkbin,
        );

        let initialize_pk = ProvingKey::build(initialize_zkbin.k, &init_circuit);
        let deploy_capital_pk = ProvingKey::build(deploy_capital_zkbin.k, &deploy_circuit);
        let claim_fees_pk = ProvingKey::build(claim_fees_zkbin.k, &claim_circuit);

        Self {
            initialize_zkbin,
            initialize_pk,
            deploy_capital_zkbin,
            deploy_capital_pk,
            claim_fees_zkbin,
            claim_fees_pk,
        }
    }
}

impl super::ContractHarness for RelayerEndowmentHarness {
    fn name(&self) -> &str {
        "relayer_endowment"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["Initialize", "DeployCapital", "ClaimFees"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "Initialize" => Some(&self.initialize_zkbin),
            "DeployCapital" => Some(&self.deploy_capital_zkbin),
            "ClaimFees" => Some(&self.claim_fees_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "Initialize" => Some(&self.initialize_pk),
            "DeployCapital" => Some(&self.deploy_capital_pk),
            "ClaimFees" => Some(&self.claim_fees_pk),
            _ => None,
        }
    }
}