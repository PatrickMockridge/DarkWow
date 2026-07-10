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

//! Attestation consume_claim_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ConsumeClaimV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ConsumeClaimV1PublicInputs {
    pub claim_id: pallas::Base,
    pub claimant_pub_x: pallas::Base,
    pub claimant_pub_y: pallas::Base,
    pub nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ConsumeClaimV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.claim_id,
            self.claimant_pub_x,
            self.claimant_pub_y,
            self.nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for consume_claim proof generation
#[derive(Debug, Clone)]
pub struct ConsumeClaimV1CallData {
    pub claim_id: pallas::Base,
    pub nullifier: pallas::Base,
    pub claimant_secret: pallas::Base,
    // Public inputs
    pub claimant_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ConsumeClaimV1CallData {
    pub fn new(
        claim_id: pallas::Base,
        nullifier: pallas::Base,
        claimant_secret: pallas::Base,
        claimant_public: PublicKey,
    ) -> Self {
        Self { claim_id, nullifier, claimant_secret, claimant_public, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> ConsumeClaimV1PublicInputs {
        let (ix, iy) = self.claimant_public.xy().expect("pk not identity");
        ConsumeClaimV1PublicInputs {
            claim_id: self.claim_id,
            claimant_pub_x: ix,
            claimant_pub_y: iy,
            nullifier: self.nullifier,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.claimant_public.xy().expect("pk not identity");
        vec![
            // Must match circuit witness order:
            // claim_id, nullifier, claimant_secret, claimant_pub_x, claimant_pub_y
            Witness::Base(Value::known(self.claim_id)),
            Witness::Base(Value::known(self.nullifier)),
            Witness::Base(Value::known(self.claimant_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a ConsumeClaim ZK proof
pub fn consume_claim_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ConsumeClaimV1CallData,
) -> Result<(Proof, ConsumeClaimV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}