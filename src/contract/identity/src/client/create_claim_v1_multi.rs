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

//! Identity create_claim_v1_multi ZK proof generation (Multi-credential)

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// CreateClaimV1Multi circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateClaimMultiPublicInputs {
    pub nullifier: pallas::Base,
    pub claim_type: pallas::Base,
    pub issuer_pub_x: pallas::Base,
    pub issuer_pub_y: pallas::Base,
    pub schema_hash: pallas::Base,
    pub num_credentials: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimMultiPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier,
            self.claim_type,
            self.issuer_pub_x,
            self.issuer_pub_y,
            self.schema_hash,
            self.num_credentials,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for create_claim_multi proof generation
#[derive(Debug, Clone)]
pub struct CreateClaimMultiCallData {
    // Credential 1
    pub secret_1: pallas::Base,
    pub commitment_1: pallas::Base,
    pub attribute_1: pallas::Base,
    pub threshold_1: pallas::Base,
    // Credential 2
    pub secret_2: pallas::Base,
    pub commitment_2: pallas::Base,
    pub attribute_2: pallas::Base,
    pub threshold_2: pallas::Base,
    // Credential 3
    pub secret_3: pallas::Base,
    pub commitment_3: pallas::Base,
    pub attribute_3: pallas::Base,
    pub threshold_3: pallas::Base,
    // Public inputs
    pub issuer_public: PublicKey,
    pub schema_hash: pallas::Base,
    pub claim_type: pallas::Base,
    pub num_credentials: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimMultiCallData {
    pub fn new(
        secret_1: pallas::Base,
        commitment_1: pallas::Base,
        attribute_1: pallas::Base,
        threshold_1: pallas::Base,
        secret_2: pallas::Base,
        commitment_2: pallas::Base,
        attribute_2: pallas::Base,
        threshold_2: pallas::Base,
        secret_3: pallas::Base,
        commitment_3: pallas::Base,
        attribute_3: pallas::Base,
        threshold_3: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
        num_credentials: u64,
    ) -> Self {
        Self {
            secret_1,
            commitment_1,
            attribute_1,
            threshold_1,
            secret_2,
            commitment_2,
            attribute_2,
            threshold_2,
            secret_3,
            commitment_3,
            attribute_3,
            threshold_3,
            issuer_public,
            schema_hash,
            claim_type,
            num_credentials,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute combined nullifier
    pub fn compute_nullifier(&self) -> pallas::Base {
        let computed_nullifier_1 = poseidon_hash([self.secret_1, self.commitment_1]);
        let computed_nullifier_2 = poseidon_hash([self.secret_2, self.commitment_2]);
        let computed_nullifier_3 = poseidon_hash([self.secret_3, self.commitment_3]);
        let combined = poseidon_hash([computed_nullifier_1, computed_nullifier_2]);
        poseidon_hash([combined, computed_nullifier_3])
    }

    pub fn compute_public_inputs(&self) -> CreateClaimMultiPublicInputs {
        let (ix, iy) = self.issuer_public.xy();
        CreateClaimMultiPublicInputs {
            nullifier: self.compute_nullifier(),
            claim_type: self.claim_type,
            issuer_pub_x: ix,
            issuer_pub_y: iy,
            schema_hash: self.schema_hash,
            num_credentials: pallas::Base::from(self.num_credentials),
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.issuer_public.xy();
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.compute_nullifier())),
            Witness::Base(Value::known(self.claim_type)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.schema_hash)),
            Witness::Base(Value::known(pallas::Base::from(self.num_credentials))),
            // Credential 1
            Witness::Base(Value::known(self.secret_1)),
            Witness::Base(Value::known(self.commitment_1)),
            Witness::Base(Value::known(self.attribute_1)),
            Witness::Base(Value::known(self.threshold_1)),
            // Credential 2
            Witness::Base(Value::known(self.secret_2)),
            Witness::Base(Value::known(self.commitment_2)),
            Witness::Base(Value::known(self.attribute_2)),
            Witness::Base(Value::known(self.threshold_2)),
            // Credential 3
            Witness::Base(Value::known(self.secret_3)),
            Witness::Base(Value::known(self.commitment_3)),
            Witness::Base(Value::known(self.attribute_3)),
            Witness::Base(Value::known(self.threshold_3)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a CreateClaimMulti ZK proof
pub fn create_claim_multi_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateClaimMultiCallData,
) -> Result<(Proof, CreateClaimMultiPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}