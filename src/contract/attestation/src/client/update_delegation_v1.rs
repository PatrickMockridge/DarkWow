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

//! Attestation update_delegation_v1 ZK proof generation

use dwow::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;

/// UpdateDelegationV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct UpdateDelegationV1PublicInputs {
    pub original_attestation_id: pallas::Base,
    pub delegation_type: pallas::Base,
    pub current_depth: pallas::Base,
    pub max_depth: pallas::Base,
    pub delegator_stake: pallas::Base,
    pub delegatee_stake: pallas::Base,
    pub max_ratio: pallas::Base,
}

impl UpdateDelegationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.original_attestation_id,
            self.delegation_type,
            self.current_depth,
            self.max_depth,
            self.delegator_stake,
            self.delegatee_stake,
            self.max_ratio,
        ]
    }
}

/// Input data for update_delegation proof generation
#[derive(Debug, Clone)]
pub struct UpdateDelegationV1CallData {
    pub original_attestation_id: pallas::Base,
    pub delegation_type: pallas::Base,
    pub current_depth: pallas::Base,
    pub max_depth: pallas::Base,
    pub delegator_stake: pallas::Base,
    pub delegatee_stake: pallas::Base,
    pub max_ratio: pallas::Base,
}

impl UpdateDelegationV1CallData {
    pub fn new(
        original_attestation_id: pallas::Base,
        delegation_type: pallas::Base,
        current_depth: pallas::Base,
        max_depth: pallas::Base,
        delegator_stake: pallas::Base,
        delegatee_stake: pallas::Base,
        max_ratio: pallas::Base,
    ) -> Self {
        Self {
            original_attestation_id,
            delegation_type,
            current_depth,
            max_depth,
            delegator_stake,
            delegatee_stake,
            max_ratio,
        }
    }

    pub fn compute_public_inputs(&self) -> UpdateDelegationV1PublicInputs {
        UpdateDelegationV1PublicInputs {
            original_attestation_id: self.original_attestation_id,
            delegation_type: self.delegation_type,
            current_depth: self.current_depth,
            max_depth: self.max_depth,
            delegator_stake: self.delegator_stake,
            delegatee_stake: self.delegatee_stake,
            max_ratio: self.max_ratio,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.original_attestation_id)),
            Witness::Base(Value::known(self.delegation_type)),
            Witness::Base(Value::known(self.current_depth)),
            Witness::Base(Value::known(self.max_depth)),
            Witness::Base(Value::known(self.delegator_stake)),
            Witness::Base(Value::known(self.delegatee_stake)),
            Witness::Base(Value::known(self.max_ratio)),
            // Delegation type constants as witnesses
            Witness::Base(Value::known(pallas::Base::zero())), // DELEGATION_TYPE_NONE
            Witness::Base(Value::known(pallas::Base::one())),  // DELEGATION_TYPE_FULL
            Witness::Base(Value::known(pallas::Base::from(2))), // DELEGATION_TYPE_RESTRICTED
        ]
    }
}

/// Create an UpdateDelegation ZK proof
pub fn update_delegation_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UpdateDelegationV1CallData,
) -> Result<(Proof, UpdateDelegationV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}