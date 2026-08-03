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

//! Oracle Test Harness
//!
//! Provides isolated testing for Oracle contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{crypto::{MerkleNode, PublicKey}, pasta::pallas};
use dwow_serial::Encodable;

use dwow_oracle_contract::client::{
    aggregate_v1::{AggregateV1CallData, AggregateV1PublicInputs, aggregate_v1_proof},
    attest_value_v1::{AttestValueV1CallData, AttestValueV1PublicInputs, attest_value_v1_proof},
    push_value_commitment_v1::{
        PushValueCommitmentV1CallData, PushValueCommitmentV1PublicInputs,
        push_value_commitment_v1_proof,
    },
    push_value_v1::{PushValueV1CallData, PushValueV1PublicInputs, push_value_v1_proof},
    register_oracle_v1::{
        RegisterOracleV1CallData, register_oracle_v1_proof,
    },
};
use dwow_oracle_contract::model::{
    AggregateParamsV1, AttestValueParamsV1, PushValueCommitmentParamsV1, PushValueParamsV1,
    RegisterOracleParamsV1, OracleId, AttestationId,
};

/// Oracle Harness for isolated testing
pub struct OracleHarness {
    /// RegisterOracle_V1 ZkBinary
    register_oracle_zkbin: ZkBinary,
    /// RegisterOracle_V1 ProvingKey
    register_oracle_pk: ProvingKey,
    /// PushValueCommitment_V1 ZkBinary
    push_value_commitment_zkbin: ZkBinary,
    /// PushValueCommitment_V1 ProvingKey
    push_value_commitment_pk: ProvingKey,
    /// Aggregate_V1 ZkBinary
    aggregate_zkbin: ZkBinary,
    /// Aggregate_V1 ProvingKey
    aggregate_pk: ProvingKey,
    /// AttestValue_V1 ZkBinary
    attest_value_zkbin: ZkBinary,
    /// AttestValue_V1 ProvingKey
    attest_value_pk: ProvingKey,
    /// PushValue_V1 ZkBinary
    push_value_zkbin: ZkBinary,
    /// PushValue_V1 ProvingKey
    push_value_pk: ProvingKey,
}

impl OracleHarness {
    /// Spawn a new Oracle harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let register_oracle_bin =
            include_bytes!("../../../oracle/proof/register_oracle_v2.zk.bin");
        let push_value_commitment_bin =
            include_bytes!("../../../oracle/proof/push_value_commitment_v2.zk.bin");
        let aggregate_bin =
            include_bytes!("../../../oracle/proof/aggregate_v2.zk.bin");
        let attest_value_bin =
            include_bytes!("../../../oracle/proof/attest_value_v2.zk.bin");
        let push_value_bin =
            include_bytes!("../../../oracle/proof/push_value_v2.zk.bin");

        let register_oracle_zkbin =
            ZkBinary::decode(register_oracle_bin, false).unwrap();
        let push_value_commitment_zkbin =
            ZkBinary::decode(push_value_commitment_bin, false).unwrap();
        let aggregate_zkbin =
            ZkBinary::decode(aggregate_bin, false).unwrap();
        let attest_value_zkbin =
            ZkBinary::decode(attest_value_bin, false).unwrap();
        let push_value_zkbin =
            ZkBinary::decode(push_value_bin, false).unwrap();

        let register_oracle_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&register_oracle_zkbin).unwrap(),
            &register_oracle_zkbin,
        );
        let push_value_commitment_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&push_value_commitment_zkbin).unwrap(),
            &push_value_commitment_zkbin,
        );
        let aggregate_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&aggregate_zkbin).unwrap(),
            &aggregate_zkbin,
        );
        let attest_value_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&attest_value_zkbin).unwrap(),
            &attest_value_zkbin,
        );
        let push_value_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&push_value_zkbin).unwrap(),
            &push_value_zkbin,
        );

        let register_oracle_pk =
            ProvingKey::build(register_oracle_zkbin.k, &register_oracle_circuit).expect("ProvingKey::build failed");
        let push_value_commitment_pk =
            ProvingKey::build(push_value_commitment_zkbin.k, &push_value_commitment_circuit).expect("ProvingKey::build failed");
        let aggregate_pk =
            ProvingKey::build(aggregate_zkbin.k, &aggregate_circuit).expect("ProvingKey::build failed");
        let attest_value_pk =
            ProvingKey::build(attest_value_zkbin.k, &attest_value_circuit).expect("ProvingKey::build failed");
        let push_value_pk =
            ProvingKey::build(push_value_zkbin.k, &push_value_circuit).expect("ProvingKey::build failed");

        Self {
            register_oracle_zkbin, register_oracle_pk,
            push_value_commitment_zkbin, push_value_commitment_pk,
            aggregate_zkbin, aggregate_pk,
            attest_value_zkbin, attest_value_pk,
            push_value_zkbin, push_value_pk,
        }
    }

    /// Register an oracle
    pub fn register_oracle(
        &self,
        oracle_secret: pallas::Base,
        oracle_public: PublicKey,
        oracle_id: pallas::Base,
        name: String,
        data_type: String,
    ) -> Result<RegisterOracleResult, Box<dyn std::error::Error>> {
        let input = RegisterOracleV1CallData::new(oracle_secret, oracle_public);

        let (proof, public_inputs) = register_oracle_v1_proof(
            &self.register_oracle_zkbin,
            &self.register_oracle_pk,
            &input,
        )?;

        // Build RegisterOracleParamsV1 for call_data
        let params = RegisterOracleParamsV1 {
            proof: vec![],
            oracle_id: dwow_oracle_contract::model::OracleId(oracle_id),
            oracle_pub: oracle_public,
            name,
            data_type,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(RegisterOracleResult {
            call_data,
            oracle_pub_x: public_inputs.oracle_pub_x,
            oracle_pub_y: public_inputs.oracle_pub_y,
            proof,
        })
    }

    /// Push a value to an oracle (function code 0x01)
    pub fn push_value(
        &self,
        oracle_id: pallas::Base,
        oracle_secret: pallas::Base,
        oracle_public: PublicKey,
        value: pallas::Base,
    ) -> Result<PushValueResult, Box<dyn std::error::Error>> {
        let input = PushValueV1CallData::new(oracle_id, oracle_secret, oracle_public, value);
        let (proof, public_inputs) = push_value_v1_proof(
            &self.push_value_zkbin, &self.push_value_pk, &input,
        )?;

        let params = PushValueParamsV1 {
            proof: proof.as_ref().to_vec(),
            oracle_id: OracleId(public_inputs.oracle_id),
            value: public_inputs.value,
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(PushValueResult { call_data, proof, public_inputs })
    }

    /// Attest to a value with a predicate (function code 0x02)
    #[allow(clippy::too_many_arguments)]
    pub fn attest_value(
        &self,
        oracle_id: pallas::Base,
        attestation_id: pallas::Base,
        oracle_secret: pallas::Base,
        predicate: pallas::Base,
        threshold: pallas::Base,
        value: pallas::Base,
        oracle_public: PublicKey,
    ) -> Result<AttestValueResult, Box<dyn std::error::Error>> {
        let input = AttestValueV1CallData::new(
            oracle_id, attestation_id, oracle_secret, predicate, threshold, value, oracle_public,
        );
        let (proof, public_inputs) = attest_value_v1_proof(
            &self.attest_value_zkbin, &self.attest_value_pk, &input,
        )?;

        let params = AttestValueParamsV1 {
            proof: proof.as_ref().to_vec(),
            oracle_id: OracleId(public_inputs.oracle_id),
            attestation_id: AttestationId(public_inputs.attestation_id),
            predicate: 0, // Matches
            threshold: public_inputs.threshold,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        Ok(AttestValueResult { call_data, proof, public_inputs })
    }

    /// Push a value commitment (function code 0x03)
    #[allow(clippy::too_many_arguments)]
    pub fn push_value_commitment(
        &self,
        oracle_id: pallas::Base,
        staker_secret: pallas::Base,
        pos: u64,
        path: Vec<MerkleNode>,
        value: pallas::Base,
        nonce: pallas::Base,
        staker_public: PublicKey,
        commitment: pallas::Base,
        data_root: pallas::Base,
    ) -> Result<PushValueCommitmentResult, Box<dyn std::error::Error>> {
        let input = PushValueCommitmentV1CallData::new(
            oracle_id, staker_secret, pos, path.clone(), value, nonce, staker_public, commitment, data_root,
        );
        let (proof, public_inputs) = push_value_commitment_v1_proof(
            &self.push_value_commitment_zkbin, &self.push_value_commitment_pk, &input,
        )?;

        let params = PushValueCommitmentParamsV1 {
            proof: proof.as_ref().to_vec(),
            oracle_id: OracleId(public_inputs.oracle_id),
            commitment: public_inputs.commitment,
            data_root: public_inputs.data_root,
            pos: pallas::Base::from(pos),
            path: path.iter().map(|n| n.inner()).collect(),
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(PushValueCommitmentResult { call_data, proof, public_inputs })
    }

    /// Aggregate values from multiple oracles (function code 0x04)
    #[allow(clippy::too_many_arguments)]
    pub fn aggregate(
        &self,
        oracle_id: pallas::Base,
        values: [pallas::Base; 4],
        weights: [pallas::Base; 4],
        sum_weights: pallas::Base,
        result: pallas::Base,
        min_result: pallas::Base,
        max_result: pallas::Base,
    ) -> Result<AggregateResult, Box<dyn std::error::Error>> {
        let input = AggregateV1CallData::new(
            oracle_id,
            values[0], values[1], values[2], values[3],
            weights[0], weights[1], weights[2], weights[3],
            sum_weights, result, min_result, max_result,
        );
        let (proof, public_inputs) = aggregate_v1_proof(
            &self.aggregate_zkbin, &self.aggregate_pk, &input,
        )?;

        let params = AggregateParamsV1 {
            proof: proof.as_ref().to_vec(),
            oracle_id: OracleId(public_inputs.oracle_id),
            result: public_inputs.result,
            min_result: public_inputs.min_result,
            max_result: public_inputs.max_result,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        Ok(AggregateResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for OracleHarness {
    fn name(&self) -> &str {
        "oracle"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["RegisterOracleV2", "PushValueCommitment", "Aggregate", "AttestValue", "PushValue"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "RegisterOracleV2" => Some(&self.register_oracle_zkbin),
            "PushValueCommitment" => Some(&self.push_value_commitment_zkbin),
            "Aggregate" => Some(&self.aggregate_zkbin),
            "AttestValue" => Some(&self.attest_value_zkbin),
            "PushValue" => Some(&self.push_value_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "RegisterOracleV2" => Some(&self.register_oracle_pk),
            "PushValueCommitment" => Some(&self.push_value_commitment_pk),
            "Aggregate" => Some(&self.aggregate_pk),
            "AttestValue" => Some(&self.attest_value_pk),
            "PushValue" => Some(&self.push_value_pk),
            _ => None,
        }
    }
}

/// Result of register_oracle
pub struct RegisterOracleResult {
    pub call_data: Vec<u8>,
    pub oracle_pub_x: pallas::Base,
    pub oracle_pub_y: pallas::Base,
    pub proof: dwow_core::zk::Proof,
}

/// Result of push_value
pub struct PushValueResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: PushValueV1PublicInputs,
}

/// Result of attest_value
pub struct AttestValueResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AttestValueV1PublicInputs,
}

/// Result of push_value_commitment
pub struct PushValueCommitmentResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: PushValueCommitmentV1PublicInputs,
}

/// Result of aggregate
pub struct AggregateResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AggregateV1PublicInputs,
}
