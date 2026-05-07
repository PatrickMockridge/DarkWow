/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Subscription update_usage ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::poseidon_hash,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// UpdateUsageV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct UpdateUsagePublicInputs {
    pub derived_id: pallas::Base,
}

impl UpdateUsagePublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_id]
    }
}

/// Input data for update_usage proof generation
#[derive(Debug, Clone)]
pub struct UpdateUsageCallData {
    pub subscription_id: pallas::Base,
    pub subscriber_pub_x: pallas::Base,
    pub subscriber_pub_y: pallas::Base,
    pub usage_timestamp: pallas::Base,
    pub nonce: pallas::Base,
}

impl UpdateUsageCallData {
    pub fn new(
        subscription_id: pallas::Base,
        subscriber_pub_x: pallas::Base,
        subscriber_pub_y: pallas::Base,
        usage_timestamp: pallas::Base,
        nonce: pallas::Base,
    ) -> Self {
        Self { subscription_id, subscriber_pub_x, subscriber_pub_y, usage_timestamp, nonce }
    }

    pub fn compute_public_inputs(&self) -> UpdateUsagePublicInputs {
        let derived_id = poseidon_hash([
            self.subscription_id,
            self.subscriber_pub_x,
            self.subscriber_pub_y,
            self.usage_timestamp,
            self.nonce,
        ]);
        UpdateUsagePublicInputs { derived_id }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Must match circuit witness order (all Base):
            // subscription_id, subscriber_pub_x, subscriber_pub_y, usage_timestamp, nonce
            Witness::Base(Value::known(self.subscription_id)),
            Witness::Base(Value::known(self.subscriber_pub_x)),
            Witness::Base(Value::known(self.subscriber_pub_y)),
            Witness::Base(Value::known(self.usage_timestamp)),
            Witness::Base(Value::known(self.nonce)),
        ]
    }
}

/// Create an UpdateUsage ZK proof
pub fn create_update_usage_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UpdateUsageCallData,
) -> Result<(Proof, UpdateUsagePublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
