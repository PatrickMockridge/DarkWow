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

//! Oracle register_oracle_v1 ZK proof generation

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

/// RegisterOracleV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RegisterOracleV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub oracle_commitment: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RegisterOracleV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.oracle_id, self.oracle_commitment, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for register_oracle proof generation
#[derive(Debug, Clone)]
pub struct RegisterOracleV1CallData {
    pub oracle_id: pallas::Base,
    pub oracle_secret: pallas::Base,
    // Public inputs
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RegisterOracleV1CallData {
    pub fn new(oracle_id: pallas::Base, oracle_secret: pallas::Base) -> Self {
        Self { oracle_id, oracle_secret, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    /// The registered operator identity: `H(DOMAIN_OPERATOR_COMMITMENT, oracle_secret, oracle_id)`,
    /// with `DOMAIN_OPERATOR_COMMITMENT = witness_base(4)`.
    ///
    /// This was `PublicKey::from_secret(SecretKey::from_base(oracle_secret))` — the operator's
    /// static key, computed the same way the circuit derived it, and published. It is now a hiding
    /// commitment, so nothing static is disclosed (OBL-Z9).
    pub fn compute_commitment(&self) -> pallas::Base {
        // `witness_base(8)`, not 4: the operator commitment has its own domain (OBL-Z19).
        poseidon_hash([pallas::Base::from(8u64), self.oracle_secret, self.oracle_id])
    }

    pub fn compute_public_inputs(&self) -> RegisterOracleV1PublicInputs {
        // Circuit: DOMAIN_TX_BINDING = witness_base(1) = 1
        let tx_binding = poseidon_hash([pallas::Base::from(1u64), self.tx_commitment, self.tx_nonce]);
        RegisterOracleV1PublicInputs {
            oracle_id: self.oracle_id,
            oracle_commitment: self.compute_commitment(),
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        // Circuit: DOMAIN_TX_BINDING = witness_base(1) = 1
        let tx_binding = poseidon_hash([pallas::Base::from(1u64), self.tx_commitment, self.tx_nonce]);
        vec![
            // Circuit order: oracle_secret(0), oracle_id(1),
            //   tx_commitment(2), tx_nonce(3), tx_binding(4)
            Witness::Base(Value::known(self.oracle_secret)),
            Witness::Base(Value::known(self.oracle_id)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ]
    }
}

/// Create a RegisterOracle ZK proof
pub fn register_oracle_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RegisterOracleV1CallData,
) -> Result<(Proof, RegisterOracleV1PublicInputs)> {
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