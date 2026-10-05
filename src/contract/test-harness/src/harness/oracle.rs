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
use dwow_sdk::{
    crypto::pasta_prelude::PrimeField,
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_oracle_contract::client::{
    aggregate::{AggregateV1CallData, AggregateV1PublicInputs, aggregate_v1_proof},
    attest_value::{AttestValueV1CallData, AttestValueV1PublicInputs, attest_value_v1_proof},
    push_value_commitment::{
        PushValueCommitmentV1CallData, PushValueCommitmentV1PublicInputs,
        push_value_commitment_v1_proof,
    },
    push_value::{PushValueV1CallData, PushValueV1PublicInputs, push_value_v1_proof},
    register_oracle::{
        RegisterOracleV1CallData, register_oracle_v1_proof,
    },
    set_oracle_active::{
        SetOracleActiveV1CallData, SetOracleActiveV1PublicInputs, set_oracle_active_v1_proof,
    },
};
use dwow_oracle_contract::model::{
    AggregateParamsV1, AttestValueParamsV1, PushValueCommitmentParamsV1, PushValueParamsV1,
    RegisterOracleParamsV1, SetOracleActiveParamsV1, OracleId, AttestationId,
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
    /// SetOracleActive_V1 ZkBinary
    set_oracle_active_zkbin: ZkBinary,
    /// SetOracleActive_V1 ProvingKey
    set_oracle_active_pk: ProvingKey,
    /// The contract's deployed id (`OBL-C198`): the transaction commitment is derived over the
    /// call set, and a call carries this id, so the harness has to know it.
    contract_id: dwow_sdk::crypto::ContractId,
}

/// The transaction commitment over an ordered call set — the same derivation the node recomputes
/// (`dwow_sdk::crypto::util::tx_commitment`). The order is the one `DarkForest::build_vec` emits:
/// DFS post-order, children before parents.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

impl OracleHarness {
    /// Spawn a new Oracle harness with pre-loaded circuits
    pub fn spawn(contract_id: dwow_sdk::crypto::ContractId) -> Self {
        dwow_oracle_contract::enable_deterministic_zk();
        let register_oracle_bin =
            include_bytes!("../../../oracle/proof/register_oracle.zk.bin");
        let push_value_commitment_bin =
            include_bytes!("../../../oracle/proof/push_value_commitment.zk.bin");
        let aggregate_bin =
            include_bytes!("../../../oracle/proof/aggregate.zk.bin");
        let attest_value_bin =
            include_bytes!("../../../oracle/proof/attest_value.zk.bin");
        let push_value_bin =
            include_bytes!("../../../oracle/proof/push_value.zk.bin");
        let set_oracle_active_bin =
            include_bytes!("../../../oracle/proof/set_oracle_active.zk.bin");

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
        let set_oracle_active_zkbin =
            ZkBinary::decode(set_oracle_active_bin, false).unwrap();

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
        let set_oracle_active_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&set_oracle_active_zkbin).unwrap(),
            &set_oracle_active_zkbin,
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
        let set_oracle_active_pk =
            ProvingKey::build(set_oracle_active_zkbin.k, &set_oracle_active_circuit).expect("ProvingKey::build failed");

        Self {
            register_oracle_zkbin, register_oracle_pk,
            push_value_commitment_zkbin, push_value_commitment_pk,
            aggregate_zkbin, aggregate_pk,
            attest_value_zkbin, attest_value_pk,
            push_value_zkbin, push_value_pk,
            set_oracle_active_zkbin, set_oracle_active_pk,
            contract_id,
        }
    }

    /// The commitment over `children` followed by this call — the ordered set the node hashes.
    fn commitment_over(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        call_data: &[u8],
    ) -> pallas::Base {
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.to_vec() });
        commitment_of(&calls)
    }

    /// Register an oracle
    pub fn register_oracle(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        oracle_secret: pallas::Base,
        oracle_id: pallas::Base,
        name: String,
        data_type: String,
    ) -> Result<RegisterOracleResult, Box<dyn std::error::Error>> {
        let mut input = RegisterOracleV1CallData::new(oracle_id, oracle_secret);
        // `OBL-C198`: the public inputs are a pure function of the call data, so they come first —
        // which is what lets the call data (and the commitment over it) exist before the proof.
        let public_inputs = input.compute_public_inputs();

        // Build RegisterOracleParamsV1 for call_data
        let params = RegisterOracleParamsV1 {
            proof: vec![],
            oracle_id: dwow_oracle_contract::model::OracleId(oracle_id),
            oracle_commitment: public_inputs.oracle_commitment,
            name,
            data_type,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode()?);

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = register_oracle_v1_proof(
            &self.register_oracle_zkbin,
            &self.register_oracle_pk,
            &input,
        )?;

        Ok(RegisterOracleResult {
            call_data,
            oracle_commitment: public_inputs.oracle_commitment,
            proof,
            commitment,
        })
    }

    /// Push a value to an oracle (function code 0x01)
    pub fn push_value(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        oracle_id: pallas::Base,
        oracle_secret: pallas::Base,
        value: pallas::Base,
    ) -> Result<PushValueResult, Box<dyn std::error::Error>> {
        let mut input = PushValueV1CallData::new(oracle_id, oracle_secret, value);
        // `OBL-C198`: public inputs first — see `register_oracle`. And the params carry **no
        // proof**: the commitment covers the call data and the proof is made after it is known, so
        // a params-carried proof would be covered by the commitment the proof itself publishes.
        let public_inputs = input.compute_public_inputs();

        let params = PushValueParamsV1 {
            proof: vec![],
            oracle_id: OracleId(public_inputs.oracle_id),
            oracle_commitment: public_inputs.oracle_commitment,
            value: public_inputs.value,
            nullifier: public_inputs.nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode()?);

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = push_value_v1_proof(
            &self.push_value_zkbin, &self.push_value_pk, &input,
        )?;

        Ok(PushValueResult { call_data, proof, public_inputs, commitment })
    }

    /// Attest to a value with a predicate (function code 0x02)
    #[allow(clippy::too_many_arguments)]
    pub fn attest_value(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        oracle_id: pallas::Base,
        attestation_id: pallas::Base,
        oracle_secret: pallas::Base,
        predicate: pallas::Base,
        threshold: pallas::Base,
        value: pallas::Base,
    ) -> Result<AttestValueResult, Box<dyn std::error::Error>> {
        let mut input = AttestValueV1CallData::new(
            oracle_id, attestation_id, oracle_secret, predicate, threshold, value,
        );
        // `OBL-C198`: public inputs first — see `register_oracle`.
        let public_inputs = input.compute_public_inputs();

        let params = AttestValueParamsV1 {
            proof: vec![],
            oracle_id: OracleId(public_inputs.oracle_id),
            oracle_commitment: public_inputs.oracle_commitment,
            attestation_id: AttestationId(public_inputs.attestation_id),
            predicate: predicate.to_repr()[0], // u8 from field element
            threshold: public_inputs.threshold,
            nullifier: public_inputs.nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode()?);

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = attest_value_v1_proof(
            &self.attest_value_zkbin, &self.attest_value_pk, &input,
        )?;

        Ok(AttestValueResult { call_data, proof, public_inputs, commitment })
    }

    /// Push a value commitment (function code 0x03)
    pub fn push_value_commitment(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        oracle_id: pallas::Base,
        staker_secret: pallas::Base,
        value: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<PushValueCommitmentResult, Box<dyn std::error::Error>> {
        // Circuit constrains commitment = poseidon_hash(DOMAIN_COMMITMENT=4, value, nonce)
        // and the operator commitment. No Merkle membership (the oracle has no data tree).
        let mut input = PushValueCommitmentV1CallData::new(oracle_id, staker_secret, value, nonce);
        // `OBL-C198`: public inputs first — see `register_oracle`.
        let public_inputs = input.compute_public_inputs();

        let params = PushValueCommitmentParamsV1 {
            proof: vec![],
            oracle_id: OracleId(public_inputs.oracle_id),
            oracle_commitment: public_inputs.oracle_commitment,
            commitment: public_inputs.commitment,
            nullifier: public_inputs.nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode()?);

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = push_value_commitment_v1_proof(
            &self.push_value_commitment_zkbin, &self.push_value_commitment_pk, &input,
        )?;

        Ok(PushValueCommitmentResult { call_data, proof, public_inputs, commitment })
    }

    /// Aggregate values from multiple oracles (function code 0x04)
    #[allow(clippy::too_many_arguments)]
    pub fn aggregate(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        oracle_id: pallas::Base,
        oracle_secret: pallas::Base,
        values: [pallas::Base; 4],
        weights: [pallas::Base; 4],
        sum_weights: pallas::Base,
        result: pallas::Base,
        min_result: pallas::Base,
        max_result: pallas::Base,
    ) -> Result<AggregateResult, Box<dyn std::error::Error>> {
        let mut input = AggregateV1CallData::new(
            oracle_id, oracle_secret,
            values[0], values[1], values[2], values[3],
            weights[0], weights[1], weights[2], weights[3],
            sum_weights, result, min_result, max_result,
        );
        // `OBL-C198`: public inputs first — see `register_oracle`.
        let public_inputs = input.compute_public_inputs();

        let params = AggregateParamsV1 {
            proof: vec![],
            oracle_id: OracleId(public_inputs.oracle_id),
            oracle_commitment: public_inputs.oracle_commitment,
            result: public_inputs.result,
            min_result: public_inputs.min_result,
            max_result: public_inputs.max_result,
            nullifier: public_inputs.nullifier,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode()?);

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = aggregate_v1_proof(
            &self.aggregate_zkbin, &self.aggregate_pk, &input,
        )?;

        Ok(AggregateResult { call_data, proof, public_inputs, commitment })
    }

    /// Set oracle active flag (function code 0x05). ZK since OBL-Z10.
    pub fn set_oracle_active(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        oracle_id: pallas::Base,
        oracle_secret: pallas::Base,
        is_active: bool,
    ) -> Result<SetOracleActiveResult, Box<dyn std::error::Error>> {
        let mut input = SetOracleActiveV1CallData::new(oracle_id, oracle_secret, is_active);

        // `OBL-C198`: the public inputs are a pure function of the call data, so they come before
        // the proof — which is what lets the call data (and the commitment over it) exist first.
        let public_inputs = input.compute_public_inputs();

        // The params carry **no proof**: the commitment covers the call data, and the proof is made
        // after that value is known — so a params-carried proof would be covered by the very
        // commitment the proof publishes. The real proof rides the transaction's proof vector, and
        // `zk_verifier`'s per-call count guard is what enforces its presence.
        let params = SetOracleActiveParamsV1 {
            proof: vec![],
            oracle_id: OracleId(public_inputs.oracle_id),
            oracle_commitment: public_inputs.oracle_commitment,
            is_active,
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode()?);

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = set_oracle_active_v1_proof(
            &self.set_oracle_active_zkbin, &self.set_oracle_active_pk, &input,
        )?;

        Ok(SetOracleActiveResult { call_data, proof, public_inputs, commitment })
    }
}

impl super::ContractHarness for OracleHarness {
    fn name(&self) -> &str {
        "oracle"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["RegisterOracleV2", "PushValueCommitmentV2", "AggregateV2", "AttestValueV2", "PushValueV2", "SetOracleActiveV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "RegisterOracleV2" => Some(&self.register_oracle_zkbin),
            "PushValueCommitmentV2" => Some(&self.push_value_commitment_zkbin),
            "AggregateV2" => Some(&self.aggregate_zkbin),
            "AttestValueV2" => Some(&self.attest_value_zkbin),
            "PushValueV2" => Some(&self.push_value_zkbin),
            "SetOracleActiveV2" => Some(&self.set_oracle_active_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "RegisterOracleV2" => Some(&self.register_oracle_pk),
            "PushValueCommitmentV2" => Some(&self.push_value_commitment_pk),
            "AggregateV2" => Some(&self.aggregate_pk),
            "AttestValueV2" => Some(&self.attest_value_pk),
            "PushValueV2" => Some(&self.push_value_pk),
            "SetOracleActiveV2" => Some(&self.set_oracle_active_pk),
            _ => None,
        }
    }
}

/// Result of register_oracle
pub struct RegisterOracleResult {
    pub call_data: Vec<u8>,
    pub oracle_commitment: pallas::Base,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of push_value
pub struct PushValueResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: PushValueV1PublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of attest_value
pub struct AttestValueResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AttestValueV1PublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of push_value_commitment
pub struct PushValueCommitmentResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: PushValueCommitmentV1PublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of aggregate
pub struct AggregateResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: AggregateV1PublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

pub struct SetOracleActiveResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SetOracleActiveV1PublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}
