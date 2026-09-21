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

//! Oracle push_value_commitment_v1 ZK proof generation

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
use rand::SeedableRng;

/// PushValueCommitmentV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PushValueCommitmentV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub oracle_commitment: pallas::Base,
    pub commitment: pallas::Base,
    pub nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PushValueCommitmentV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.oracle_id,
            self.oracle_commitment,
            self.commitment,
            self.nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for push_value_commitment proof generation
#[derive(Debug, Clone)]
pub struct PushValueCommitmentV1CallData {
    pub oracle_id: pallas::Base,
    pub staker_secret: pallas::Base,
    pub value: pallas::Base,
    pub nonce: pallas::Base,
    // Public inputs
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PushValueCommitmentV1CallData {
    pub fn new(
        oracle_id: pallas::Base,
        staker_secret: pallas::Base,
        value: pallas::Base,
        nonce: pallas::Base,
    ) -> Self {
        Self {
            oracle_id,
            staker_secret,
            value,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute commitment from value and nonce (matching circuit: poseidon_hash(DOMAIN_COMMITMENT, value, nonce))
    /// where DOMAIN_COMMITMENT = witness_base(4) = 4
    pub fn compute_data_commitment(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(4u64), self.value, self.nonce])
    }

    /// `H(DOMAIN_OPERATOR_COMMITMENT, staker_secret, oracle_id)` — must equal the registered record.
    /// `DOMAIN_OPERATOR_COMMITMENT` is also `witness_base(4)`.
    pub fn compute_oracle_commitment(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(4u64), self.staker_secret, self.oracle_id])
    }

    /// `H(DOMAIN_NULLIFIER, staker_secret, oracle_id, commitment)` — one push per data commitment.
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([
            pallas::Base::from(1u64),
            self.staker_secret,
            self.oracle_id,
            self.compute_data_commitment(),
        ])
    }

    pub fn compute_public_inputs(&self) -> PushValueCommitmentV1PublicInputs {
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        PushValueCommitmentV1PublicInputs {
            oracle_id: self.oracle_id,
            oracle_commitment: self.compute_oracle_commitment(),
            commitment: self.compute_data_commitment(),
            nullifier: self.compute_nullifier(),
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);

        vec![
            // Circuit order: oracle_id, staker_secret, value, nonce, commitment,
            //   tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.oracle_id)),
            Witness::Base(Value::known(self.staker_secret)),
            Witness::Base(Value::known(self.value)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.compute_data_commitment())),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ]
    }
}

/// Create a PushValueCommitment ZK proof
pub fn push_value_commitment_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PushValueCommitmentV1CallData,
) -> Result<(Proof, PushValueCommitmentV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };

    Ok((proof, public_inputs))
}
