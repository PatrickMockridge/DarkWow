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

//! Identity create_claim_v1 ZK proof generation

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

/// CreateClaimV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateClaimPublicInputs {
    pub nullifier: pallas::Base,
    pub claim_type: pallas::Base,
    pub issuer_pub_x: pallas::Base,
    pub issuer_pub_y: pallas::Base,
    pub schema_hash: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier,
            self.claim_type,
            self.issuer_pub_x,
            self.issuer_pub_y,
            self.schema_hash,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for create_claim proof generation
#[derive(Debug, Clone)]
pub struct CreateClaimCallData {
    pub credential_secret: pallas::Base,
    pub attribute_value: pallas::Base,
    pub threshold: pallas::Base,
    pub commitment: pallas::Base,
    // Public inputs
    pub issuer_public: PublicKey,
    pub schema_hash: pallas::Base,
    pub claim_type: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimCallData {
    pub fn new(
        credential_secret: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        commitment: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
    ) -> Self {
        Self {
            credential_secret,
            attribute_value,
            threshold,
            commitment,
            issuer_public,
            schema_hash,
            claim_type,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute nullifier from credential_secret and commitment
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.credential_secret, self.commitment])
    }

    pub fn compute_public_inputs(&self) -> CreateClaimPublicInputs {
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        CreateClaimPublicInputs {
            nullifier: self.compute_nullifier(),
            claim_type: self.claim_type,
            issuer_pub_x: ix,
            issuer_pub_y: iy,
            schema_hash: self.schema_hash,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.compute_nullifier())),
            Witness::Base(Value::known(self.claim_type)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.schema_hash)),
            // Private inputs
            Witness::Base(Value::known(self.credential_secret)),
            Witness::Base(Value::known(self.attribute_value)),
            Witness::Base(Value::known(self.threshold)),
            Witness::Base(Value::known(self.commitment)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a CreateClaim ZK proof
pub fn create_claim_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateClaimCallData,
) -> Result<(Proof, CreateClaimPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}