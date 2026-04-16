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

//! Identity Test Harness
//!
//! Provides isolated testing for Identity contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use darkfi_identity_contract::client::{
    create_claim_v1::{CreateClaimCallData, create_claim_proof},
    create_claim_v1_dag::{CreateClaimDagCallData, create_claim_dag_proof},
    create_claim_v1_l1::{CreateClaimL1CallData, create_claim_l1_proof},
    create_claim_v1_l1_v2::{CreateClaimL1V2CallData, create_claim_l1_v2_proof},
    create_claim_v1_multi::{CreateClaimMultiCallData, create_claim_multi_proof},
    create_claim_v1_ratio::{CreateClaimRatioCallData, create_claim_ratio_proof},
    issue_credential_v1::{IssueCredentialCallData, create_issue_credential_proof},
    verify_capability_v1::{VerifyCapabilityCallData, create_verify_capability_proof},
};

/// Identity Harness for isolated testing
pub struct IdentityHarness {
    /// CreateClaim_V1 ZkBinary
    create_claim_zkbin: ZkBinary,
    /// CreateClaim_V1 ProvingKey
    create_claim_pk: ProvingKey,
    /// CreateClaimDag_V1 ZkBinary
    create_claim_dag_zkbin: ZkBinary,
    /// CreateClaimDag_V1 ProvingKey
    create_claim_dag_pk: ProvingKey,
    /// CreateClaimL1_V1 ZkBinary
    create_claim_l1_zkbin: ZkBinary,
    /// CreateClaimL1_V1 ProvingKey
    create_claim_l1_pk: ProvingKey,
    /// CreateClaimL1V2_V1 ZkBinary
    create_claim_l1_v2_zkbin: ZkBinary,
    /// CreateClaimL1V2_V1 ProvingKey
    create_claim_l1_v2_pk: ProvingKey,
    /// CreateClaimMulti_V1 ZkBinary
    create_claim_multi_zkbin: ZkBinary,
    /// CreateClaimMulti_V1 ProvingKey
    create_claim_multi_pk: ProvingKey,
    /// CreateClaimRatio_V1 ZkBinary
    create_claim_ratio_zkbin: ZkBinary,
    /// CreateClaimRatio_V1 ProvingKey
    create_claim_ratio_pk: ProvingKey,
    /// IssueCredential_V1 ZkBinary
    issue_credential_zkbin: ZkBinary,
    /// IssueCredential_V1 ProvingKey
    issue_credential_pk: ProvingKey,
    /// VerifyCapability_V1 ZkBinary
    verify_capability_zkbin: ZkBinary,
    /// VerifyCapability_V1 ProvingKey
    verify_capability_pk: ProvingKey,
}

impl IdentityHarness {
    /// Spawn a new Identity harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let claim_bin = include_bytes!("../../../identity/proof/create_claim_v1.zk.bin");
        let claim_dag_bin = include_bytes!("../../../identity/proof/create_claim_v1_dag.zk.bin");
        let claim_l1_bin = include_bytes!("../../../identity/proof/create_claim_v1_l1.zk.bin");
        let claim_l1_v2_bin = include_bytes!("../../../identity/proof/create_claim_v1_l1_v2.zk.bin");
        let claim_multi_bin = include_bytes!("../../../identity/proof/create_claim_v1_multi.zk.bin");
        let claim_ratio_bin = include_bytes!("../../../identity/proof/create_claim_v1_ratio.zk.bin");
        let issue_bin = include_bytes!("../../../identity/proof/issue_credential_v1.zk.bin");
        let verify_bin = include_bytes!("../../../identity/proof/verify_capability_v1.zk.bin");

        let create_claim_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let create_claim_dag_zkbin = ZkBinary::decode(claim_dag_bin, false).unwrap();
        let create_claim_l1_zkbin = ZkBinary::decode(claim_l1_bin, false).unwrap();
        let create_claim_l1_v2_zkbin = ZkBinary::decode(claim_l1_v2_bin, false).unwrap();
        let create_claim_multi_zkbin = ZkBinary::decode(claim_multi_bin, false).unwrap();
        let create_claim_ratio_zkbin = ZkBinary::decode(claim_ratio_bin, false).unwrap();
        let issue_credential_zkbin = ZkBinary::decode(issue_bin, false).unwrap();
        let verify_capability_zkbin = ZkBinary::decode(verify_bin, false).unwrap();

        let claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_claim_zkbin).unwrap(),
            &create_claim_zkbin,
        );
        let claim_dag_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_claim_dag_zkbin).unwrap(),
            &create_claim_dag_zkbin,
        );
        let claim_l1_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_claim_l1_zkbin).unwrap(),
            &create_claim_l1_zkbin,
        );
        let claim_l1_v2_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_claim_l1_v2_zkbin).unwrap(),
            &create_claim_l1_v2_zkbin,
        );
        let claim_multi_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_claim_multi_zkbin).unwrap(),
            &create_claim_multi_zkbin,
        );
        let claim_ratio_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_claim_ratio_zkbin).unwrap(),
            &create_claim_ratio_zkbin,
        );
        let issue_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&issue_credential_zkbin).unwrap(),
            &issue_credential_zkbin,
        );
        let verify_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&verify_capability_zkbin).unwrap(),
            &verify_capability_zkbin,
        );

        let create_claim_pk = ProvingKey::build(create_claim_zkbin.k, &claim_circuit);
        let create_claim_dag_pk = ProvingKey::build(create_claim_dag_zkbin.k, &claim_dag_circuit);
        let create_claim_l1_pk = ProvingKey::build(create_claim_l1_zkbin.k, &claim_l1_circuit);
        let create_claim_l1_v2_pk = ProvingKey::build(create_claim_l1_v2_zkbin.k, &claim_l1_v2_circuit);
        let create_claim_multi_pk = ProvingKey::build(create_claim_multi_zkbin.k, &claim_multi_circuit);
        let create_claim_ratio_pk = ProvingKey::build(create_claim_ratio_zkbin.k, &claim_ratio_circuit);
        let issue_credential_pk = ProvingKey::build(issue_credential_zkbin.k, &issue_circuit);
        let verify_capability_pk = ProvingKey::build(verify_capability_zkbin.k, &verify_circuit);

        Self {
            create_claim_zkbin,
            create_claim_pk,
            create_claim_dag_zkbin,
            create_claim_dag_pk,
            create_claim_l1_zkbin,
            create_claim_l1_pk,
            create_claim_l1_v2_zkbin,
            create_claim_l1_v2_pk,
            create_claim_multi_zkbin,
            create_claim_multi_pk,
            create_claim_ratio_zkbin,
            create_claim_ratio_pk,
            issue_credential_zkbin,
            issue_credential_pk,
            verify_capability_zkbin,
            verify_capability_pk,
        }
    }
}

impl super::ContractHarness for IdentityHarness {
    fn name(&self) -> &str {
        "identity"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateClaimV1",
            "CreateClaimV1DAG",
            "CreateClaimV1L1",
            "CreateClaimV1L1V2",
            "CreateClaimV1Multi",
            "CreateClaimV1Ratio",
            "IssueCredentialV1",
            "VerifyCapabilityV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateClaimV1" => Some(&self.create_claim_zkbin),
            "CreateClaimV1DAG" => Some(&self.create_claim_dag_zkbin),
            "CreateClaimV1L1" => Some(&self.create_claim_l1_zkbin),
            "CreateClaimV1L1V2" => Some(&self.create_claim_l1_v2_zkbin),
            "CreateClaimV1Multi" => Some(&self.create_claim_multi_zkbin),
            "CreateClaimV1Ratio" => Some(&self.create_claim_ratio_zkbin),
            "IssueCredentialV1" => Some(&self.issue_credential_zkbin),
            "VerifyCapabilityV1" => Some(&self.verify_capability_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateClaimV1" => Some(&self.create_claim_pk),
            "CreateClaimV1DAG" => Some(&self.create_claim_dag_pk),
            "CreateClaimV1L1" => Some(&self.create_claim_l1_pk),
            "CreateClaimV1L1V2" => Some(&self.create_claim_l1_v2_pk),
            "CreateClaimV1Multi" => Some(&self.create_claim_multi_pk),
            "CreateClaimV1Ratio" => Some(&self.create_claim_ratio_pk),
            "IssueCredentialV1" => Some(&self.issue_credential_pk),
            "VerifyCapabilityV1" => Some(&self.verify_capability_pk),
            _ => None,
        }
    }
}