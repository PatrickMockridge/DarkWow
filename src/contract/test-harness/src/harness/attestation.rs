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

//! Attestation Test Harness
//!
//! Provides isolated testing for Attestation contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use darkfi_attestation_contract::client::{
    consume_claim_v1::{ConsumeClaimV1CallData, consume_claim_v1_proof},
    create_attestation_v1::{CreateAttestationV1CallData, create_attestation_v1_proof},
    create_claim_v1::{CreateClaimV1CallData, create_claim_v1_proof},
    delegate_attestation_v1::{DelegateAttestationV1CallData, delegate_attestation_v1_proof},
    verify_claim_v1::{VerifyClaimV1CallData, verify_claim_v1_proof},
};

/// Attestation Harness for isolated testing
pub struct AttestationHarness {
    /// CreateAttestation_V1 ZkBinary
    create_attestation_zkbin: ZkBinary,
    /// CreateAttestation_V1 ProvingKey
    create_attestation_pk: ProvingKey,
    /// CreateClaim_V1 ZkBinary
    create_claim_zkbin: ZkBinary,
    /// CreateClaim_V1 ProvingKey
    create_claim_pk: ProvingKey,
    /// VerifyClaim_V1 ZkBinary
    verify_claim_zkbin: ZkBinary,
    /// VerifyClaim_V1 ProvingKey
    verify_claim_pk: ProvingKey,
    /// ConsumeClaim_V1 ZkBinary
    consume_claim_zkbin: ZkBinary,
    /// ConsumeClaim_V1 ProvingKey
    consume_claim_pk: ProvingKey,
    /// DelegateAttestation_V1 ZkBinary
    delegate_attestation_zkbin: ZkBinary,
    /// DelegateAttestation_V1 ProvingKey
    delegate_attestation_pk: ProvingKey,
}

impl AttestationHarness {
    /// Spawn a new Attestation harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_att_bin = include_bytes!("../../../attestation/proof/create_attestation_v1.zk.bin");
        let create_claim_bin = include_bytes!("../../../attestation/proof/create_claim_v1.zk.bin");
        let verify_claim_bin = include_bytes!("../../../attestation/proof/verify_claim_v1.zk.bin");
        let consume_claim_bin = include_bytes!("../../../attestation/proof/consume_claim_v1.zk.bin");
        let delegate_bin = include_bytes!("../../../attestation/proof/delegate_attestation_v1.zk.bin");

        let create_attestation_zkbin = ZkBinary::decode(create_att_bin, false).unwrap();
        let create_claim_zkbin = ZkBinary::decode(create_claim_bin, false).unwrap();
        let verify_claim_zkbin = ZkBinary::decode(verify_claim_bin, false).unwrap();
        let consume_claim_zkbin = ZkBinary::decode(consume_claim_bin, false).unwrap();
        let delegate_attestation_zkbin = ZkBinary::decode(delegate_bin, false).unwrap();

        let create_att_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_attestation_zkbin).unwrap(),
            &create_attestation_zkbin,
        );
        let create_claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_claim_zkbin).unwrap(),
            &create_claim_zkbin,
        );
        let verify_claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&verify_claim_zkbin).unwrap(),
            &verify_claim_zkbin,
        );
        let consume_claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&consume_claim_zkbin).unwrap(),
            &consume_claim_zkbin,
        );
        let delegate_attestation_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&delegate_attestation_zkbin).unwrap(),
            &delegate_attestation_zkbin,
        );

        let create_attestation_pk = ProvingKey::build(create_attestation_zkbin.k, &create_att_circuit);
        let create_claim_pk = ProvingKey::build(create_claim_zkbin.k, &create_claim_circuit);
        let verify_claim_pk = ProvingKey::build(verify_claim_zkbin.k, &verify_claim_circuit);
        let consume_claim_pk = ProvingKey::build(consume_claim_zkbin.k, &consume_claim_circuit);
        let delegate_attestation_pk = ProvingKey::build(delegate_attestation_zkbin.k, &delegate_attestation_circuit);

        Self {
            create_attestation_zkbin,
            create_attestation_pk,
            create_claim_zkbin,
            create_claim_pk,
            verify_claim_zkbin,
            verify_claim_pk,
            consume_claim_zkbin,
            consume_claim_pk,
            delegate_attestation_zkbin,
            delegate_attestation_pk,
        }
    }
}

impl super::ContractHarness for AttestationHarness {
    fn name(&self) -> &str {
        "attestation"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateAttestationV1",
            "CreateClaimV1",
            "VerifyClaimV1",
            "ConsumeClaimV1",
            "DelegateAttestationV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateAttestationV1" => Some(&self.create_attestation_zkbin),
            "CreateClaimV1" => Some(&self.create_claim_zkbin),
            "VerifyClaimV1" => Some(&self.verify_claim_zkbin),
            "ConsumeClaimV1" => Some(&self.consume_claim_zkbin),
            "DelegateAttestationV1" => Some(&self.delegate_attestation_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateAttestationV1" => Some(&self.create_attestation_pk),
            "CreateClaimV1" => Some(&self.create_claim_pk),
            "VerifyClaimV1" => Some(&self.verify_claim_pk),
            "ConsumeClaimV1" => Some(&self.consume_claim_pk),
            "DelegateAttestationV1" => Some(&self.delegate_attestation_pk),
            _ => None,
        }
    }
}