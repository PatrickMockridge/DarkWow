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

//! InsuranceMarket Test Harness
//!
//! Provides isolated testing for InsuranceMarket contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// InsuranceMarket Harness for isolated testing
pub struct InsuranceMarketHarness {
    /// UnderwriteWithCapability_V1 ZkBinary
    underwrite_zkbin: ZkBinary,
    /// UnderwriteWithCapability_V1 ProvingKey
    underwrite_pk: ProvingKey,
    /// PurchaseCoverageWithCapability_V1 ZkBinary
    purchase_coverage_zkbin: ZkBinary,
    /// PurchaseCoverageWithCapability_V1 ProvingKey
    purchase_coverage_pk: ProvingKey,
}

impl InsuranceMarketHarness {
    /// Spawn a new InsuranceMarket harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let underwrite_bin =
            include_bytes!("../../../insurance_market/proof/underwrite_with_capability_v1.zk.bin");
        let purchase_bin =
            include_bytes!("../../../insurance_market/proof/purchase_coverage_with_capability_v1.zk.bin");

        let underwrite_zkbin = ZkBinary::decode(underwrite_bin, false).unwrap();
        let purchase_coverage_zkbin = ZkBinary::decode(purchase_bin, false).unwrap();

        let underwrite_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&underwrite_zkbin).unwrap(),
            &underwrite_zkbin,
        );
        let purchase_coverage_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&purchase_coverage_zkbin).unwrap(),
            &purchase_coverage_zkbin,
        );

        let underwrite_pk = ProvingKey::build(underwrite_zkbin.k, &underwrite_circuit);
        let purchase_coverage_pk =
            ProvingKey::build(purchase_coverage_zkbin.k, &purchase_coverage_circuit);

        Self {
            underwrite_zkbin,
            underwrite_pk,
            purchase_coverage_zkbin,
            purchase_coverage_pk,
        }
    }
}

impl super::ContractHarness for InsuranceMarketHarness {
    fn name(&self) -> &str {
        "insurance_market"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["UnderwriteWithCapability", "PurchaseCoverageWithCapability"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "UnderwriteWithCapability" => Some(&self.underwrite_zkbin),
            "PurchaseCoverageWithCapability" => Some(&self.purchase_coverage_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "UnderwriteWithCapability" => Some(&self.underwrite_pk),
            "PurchaseCoverageWithCapability" => Some(&self.purchase_coverage_pk),
            _ => None,
        }
    }
}