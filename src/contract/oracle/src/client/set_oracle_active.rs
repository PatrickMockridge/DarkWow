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

//! Oracle set_oracle_active_v1 ZK proof generation
//!
//! `set_oracle_active_v1` had no circuit at all before OBL-Z10: it was dispatched as a plain
//! instruction whose only check compared the stored operator key against one the caller supplied in
//! the call payload, so anyone could deactivate any feed. It now proves the same operator
//! commitment the other operations do.

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

/// SetOracleActiveV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SetOracleActiveV1PublicInputs {
    pub oracle_id: pallas::Base,
    pub oracle_commitment: pallas::Base,
    pub is_active: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SetOracleActiveV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.oracle_id,
            self.oracle_commitment,
            self.is_active,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for set_oracle_active proof generation
#[derive(Debug, Clone)]
pub struct SetOracleActiveV1CallData {
    pub oracle_id: pallas::Base,
    pub oracle_secret: pallas::Base,
    pub is_active: bool,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SetOracleActiveV1CallData {
    pub fn new(oracle_id: pallas::Base, oracle_secret: pallas::Base, is_active: bool) -> Self {
        Self {
            oracle_id,
            oracle_secret,
            is_active,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// `H(DOMAIN_OPERATOR_COMMITMENT, oracle_secret, oracle_id)` — must equal the registered record.
    pub fn compute_commitment(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(4u64), self.oracle_secret, self.oracle_id])
    }

    pub fn compute_public_inputs(&self) -> SetOracleActiveV1PublicInputs {
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        SetOracleActiveV1PublicInputs {
            oracle_id: self.oracle_id,
            oracle_commitment: self.compute_commitment(),
            is_active: pallas::Base::from(self.is_active as u64),
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            // Circuit order: oracle_id, oracle_secret, is_active, tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.oracle_id)),
            Witness::Base(Value::known(self.oracle_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.is_active as u64))),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)), // tx_binding (computed by circuit)
        ]
    }
}

/// Create a SetOracleActive ZK proof
pub fn set_oracle_active_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SetOracleActiveV1CallData,
) -> Result<(Proof, SetOracleActiveV1PublicInputs)> {
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
