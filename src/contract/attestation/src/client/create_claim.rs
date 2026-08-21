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

//! Attestation create_claim_v1 ZK proof generation

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
use rand::SeedableRng;

/// CreateClaimV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreateClaimV1PublicInputs {
    pub attestation_id: pallas::Base,
    pub claimant_pub_x: pallas::Base,
    pub claimant_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce]
    }
}

/// Input data for create_claim proof generation
#[derive(Debug, Clone)]
pub struct CreateClaimV1CallData {
    pub attestation_id: pallas::Base,
    pub claimant_secret: pallas::Base,
    // Public inputs
    pub claimant_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimV1CallData {
    pub fn new(attestation_id: pallas::Base, claimant_secret: pallas::Base, claimant_public: PublicKey) -> Self {
        Self { attestation_id, claimant_secret, claimant_public, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> CreateClaimV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.claimant_public.xy().expect("pk not identity");
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        CreateClaimV1PublicInputs { attestation_id: self.attestation_id, claimant_pub_x: ix, claimant_pub_y: iy, tx_binding, tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.claimant_public.xy().expect("pk not identity");
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            // Must match circuit witness order:
            // attestation_id, claimant_secret, claimant_pub_x, claimant_pub_y
            Witness::Base(Value::known(self.attestation_id)),
            Witness::Base(Value::known(self.claimant_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ]
    }
}

/// Create a CreateClaim ZK proof
pub fn create_claim_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateClaimV1CallData,
) -> Result<(Proof, CreateClaimV1PublicInputs)> {
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