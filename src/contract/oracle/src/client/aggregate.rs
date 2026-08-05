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

//! Oracle aggregate_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::poseidon_hash,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// AggregateV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct AggregateV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub result: pallas::Base,
    pub min_result: pallas::Base,
    pub max_result: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AggregateV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.oracle_id, self.result, self.min_result, self.max_result, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for aggregate proof generation
#[derive(Debug, Clone)]
pub struct AggregateV1CallData {
    pub oracle_id: pallas::Base,
    pub value_0: pallas::Base,
    pub value_1: pallas::Base,
    pub value_2: pallas::Base,
    pub value_3: pallas::Base,
    pub weight_0: pallas::Base,
    pub weight_1: pallas::Base,
    pub weight_2: pallas::Base,
    pub weight_3: pallas::Base,
    pub sum_weights: pallas::Base,
    pub result: pallas::Base,
    pub min_result: pallas::Base,
    pub max_result: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AggregateV1CallData {
    pub fn new(
        oracle_id: pallas::Base,
        value_0: pallas::Base,
        value_1: pallas::Base,
        value_2: pallas::Base,
        value_3: pallas::Base,
        weight_0: pallas::Base,
        weight_1: pallas::Base,
        weight_2: pallas::Base,
        weight_3: pallas::Base,
        sum_weights: pallas::Base,
        result: pallas::Base,
        min_result: pallas::Base,
        max_result: pallas::Base,
    ) -> Self {
        Self {
            oracle_id,
            value_0,
            value_1,
            value_2,
            value_3,
            weight_0,
            weight_1,
            weight_2,
            weight_3,
            sum_weights,
            result,
            min_result,
            max_result,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> AggregateV1PublicInputs {
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        AggregateV1PublicInputs {
            oracle_id: self.oracle_id,
            result: self.result,
            min_result: self.min_result,
            max_result: self.max_result,
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            // Circuit order: oracle_id, value_0, value_1, value_2, value_3,
            //   weight_0, weight_1, weight_2, weight_3, sum_weights, result,
            //   min_result, max_result, tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.oracle_id)),
            Witness::Base(Value::known(self.value_0)),
            Witness::Base(Value::known(self.value_1)),
            Witness::Base(Value::known(self.value_2)),
            Witness::Base(Value::known(self.value_3)),
            Witness::Base(Value::known(self.weight_0)),
            Witness::Base(Value::known(self.weight_1)),
            Witness::Base(Value::known(self.weight_2)),
            Witness::Base(Value::known(self.weight_3)),
            Witness::Base(Value::known(self.sum_weights)),
            Witness::Base(Value::known(self.result)),
            Witness::Base(Value::known(self.min_result)),
            Witness::Base(Value::known(self.max_result)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)), // tx_binding (computed by circuit)
        ]
    }
}

/// Create an Aggregate ZK proof
pub fn aggregate_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AggregateV1CallData,
) -> Result<(Proof, AggregateV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}