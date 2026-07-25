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

//! InsuranceMarket Test Harness
//!
//! Provides isolated testing for InsuranceMarket contract.

use dwow_core::{
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_serial::Encodable;

use dwow_insurance_market_contract::client::{
    underwrite_with_capability_v1::{
        UnderwriteWithCapabilityV1CallData, UnderwriteWithCapabilityV1PublicInputs,
        underwrite_with_capability_v1_proof,
    },
    purchase_coverage_with_capability_v1::{
        PurchaseCoverageWithCapabilityV1CallData, PurchaseCoverageWithCapabilityV1PublicInputs,
        purchase_coverage_with_capability_v1_proof,
    },
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
    /// PurchaseCoverageV1 ZkBinary
    purchase_coverage_v1_zkbin: ZkBinary,
    /// PurchaseCoverageV1 ProvingKey
    purchase_coverage_v1_pk: ProvingKey,
    /// PurchaseCoverageWithDAG ZkBinary
    purchase_coverage_dag_zkbin: ZkBinary,
    /// PurchaseCoverageWithDAG ProvingKey
    purchase_coverage_dag_pk: ProvingKey,
}

impl InsuranceMarketHarness {
    /// Spawn a new InsuranceMarket harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let underwrite_bin =
            include_bytes!("../../../insurance_market/proof/underwrite_with_capability_v1.zk.bin");
        let purchase_bin =
            include_bytes!("../../../insurance_market/proof/purchase_coverage_with_capability_v1.zk.bin");
        let purchase_v1_bin =
            include_bytes!("../../../insurance_market/proof/purchase_coverage_v1.zk.bin");
        let purchase_dag_bin =
            include_bytes!("../../../insurance_market/proof/purchase_coverage_with_dag_v1.zk.bin");

        let underwrite_zkbin = ZkBinary::decode(underwrite_bin, false).unwrap();
        let purchase_coverage_zkbin = ZkBinary::decode(purchase_bin, false).unwrap();
        let purchase_coverage_v1_zkbin = ZkBinary::decode(purchase_v1_bin, false).unwrap();
        let purchase_coverage_dag_zkbin = ZkBinary::decode(purchase_dag_bin, false).unwrap();

        let underwrite_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&underwrite_zkbin).unwrap(),
            &underwrite_zkbin,
        );
        let purchase_coverage_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&purchase_coverage_zkbin).unwrap(),
            &purchase_coverage_zkbin,
        );

        let underwrite_pk = ProvingKey::build(underwrite_zkbin.k, &underwrite_circuit).expect("ProvingKey::build failed");
        let purchase_coverage_pk =
            ProvingKey::build(purchase_coverage_zkbin.k, &purchase_coverage_circuit).expect("ProvingKey::build failed");

        let purchase_coverage_v1_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&purchase_coverage_v1_zkbin).unwrap(),
            &purchase_coverage_v1_zkbin,
        );
        let purchase_coverage_v1_pk =
            ProvingKey::build(purchase_coverage_v1_zkbin.k, &purchase_coverage_v1_circuit).expect("ProvingKey::build failed");

        let purchase_coverage_dag_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&purchase_coverage_dag_zkbin).unwrap(),
            &purchase_coverage_dag_zkbin,
        );
        let purchase_coverage_dag_pk =
            ProvingKey::build(purchase_coverage_dag_zkbin.k, &purchase_coverage_dag_circuit).expect("ProvingKey::build failed");

        Self {
            underwrite_zkbin,
            underwrite_pk,
            purchase_coverage_zkbin,
            purchase_coverage_pk,
            purchase_coverage_v1_zkbin,
            purchase_coverage_v1_pk,
            purchase_coverage_dag_zkbin,
            purchase_coverage_dag_pk,
        }
    }

    /// Underwrite with ZK proof (function code 0x00)
    pub fn underwrite(
        &self,
        params: &dwow_insurance_market_contract::model::UnderwriteParamsV1,
    ) -> Result<UnderwriteResult> {
        use dwow_sdk::pasta::pallas;
        let input = UnderwriteWithCapabilityV1CallData::new(
            pallas::Scalar::from(1u64), pallas::Base::from(1u64),
            params.underwriter, pallas::Base::from(1u64), pallas::Base::from(1u64),
        );
        let (proof, public_inputs) = underwrite_with_capability_v1_proof(
            &self.underwrite_zkbin, &self.underwrite_pk, &input,
        )?;

        let mut call_data = vec![0x00];
        params.encode(&mut call_data)?;

        Ok(UnderwriteResult { call_data, proof, public_inputs })
    }

    /// Purchase coverage with ZK proof (function code 0x01)
    pub fn purchase_coverage(
        &self,
        params: &dwow_insurance_market_contract::model::PurchaseCoverageParamsV1,
    ) -> Result<PurchaseCoverageResult> {
        use dwow_sdk::pasta::pallas;
        let input = PurchaseCoverageWithCapabilityV1CallData::new(
            pallas::Scalar::from(1u64), pallas::Base::from(1u64),
            params.buyer, pallas::Base::from(1u64), pallas::Base::from(1u64),
        );
        let (proof, public_inputs) = purchase_coverage_with_capability_v1_proof(
            &self.purchase_coverage_zkbin, &self.purchase_coverage_pk, &input,
        )?;

        let mut call_data = vec![0x01];
        params.encode(&mut call_data)?;

        Ok(PurchaseCoverageResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for InsuranceMarketHarness {
    fn name(&self) -> &str {
        "insurance_market"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["UnderwriteWithCapability", "PurchaseCoverageWithCapability", "PurchaseCoverageV1", "PurchaseCoverageWithDAG"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "UnderwriteWithCapability" => Some(&self.underwrite_zkbin),
            "PurchaseCoverageWithCapability" => Some(&self.purchase_coverage_zkbin),
            "PurchaseCoverageV1" => Some(&self.purchase_coverage_v1_zkbin),
            "PurchaseCoverageWithDAG" => Some(&self.purchase_coverage_dag_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "UnderwriteWithCapability" => Some(&self.underwrite_pk),
            "PurchaseCoverageWithCapability" => Some(&self.purchase_coverage_pk),
            "PurchaseCoverageV1" => Some(&self.purchase_coverage_v1_pk),
            "PurchaseCoverageWithDAG" => Some(&self.purchase_coverage_dag_pk),
            _ => None,
        }
    }
}

/// Result of underwrite
pub struct UnderwriteResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
    pub public_inputs: UnderwriteWithCapabilityV1PublicInputs,
}

/// Result of purchase_coverage
pub struct PurchaseCoverageResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
    pub public_inputs: PurchaseCoverageWithCapabilityV1PublicInputs,
}