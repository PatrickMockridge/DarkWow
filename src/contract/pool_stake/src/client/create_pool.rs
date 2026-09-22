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

//! Pool Stake CreatePool ZK proof generation

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

/// CreatePoolV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CreatePoolV1PublicInputs {
    pub creator_pub_x: pallas::Base,
    pub creator_pub_y: pallas::Base,
    pub pool_config_hash: pallas::Base,
    pub nonce: pallas::Base,
    pub derived_pool_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreatePoolV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Circuit order (`create_pool.zk`, the `constrain_instance` sequence):
        //   tx_binding, tx_nonce, derived_pool_id
        // This was `[derived_pool_id, tx_binding, tx_nonce]` — transposed. The proof is created
        // against this vector and the verifier uses the host's, so the two disagreed in *position*
        // and no proof could verify. `check-circuit-metadata-alignment.sh` compares counts and
        // cannot see it; print the circuit before changing this again.
        vec![self.tx_binding, self.tx_nonce, self.derived_pool_id]
    }
}

/// Input data for CreatePool proof generation
#[derive(Debug, Clone)]
pub struct CreatePoolV1CallData {
    pub creator_pub_x: pallas::Base,
    pub creator_pub_y: pallas::Base,
    pub pool_config_hash: pallas::Base,
    pub nonce: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreatePoolV1CallData {
    pub fn new(
        creator_public: PublicKey,
        pool_config_hash: pallas::Base,
        nonce: u64,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (cx, cy) = creator_public.xy().expect("pk not identity");
        Self { creator_pub_x: cx, creator_pub_y: cy, pool_config_hash, nonce, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    /// The circuit's `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` with
    /// `DOMAIN_TX_BINDING = witness_base(3)`. This was a literal `Base::zero()` written as **both**
    /// the public input and the witness, while the circuit constrains the witness to equal this
    /// hash — so the proof was unsatisfiable, not merely unbound.
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> CreatePoolV1PublicInputs {
        let derived_pool_id = poseidon_hash([
            pallas::Base::from(4),
            self.creator_pub_x,
            self.creator_pub_y,
            self.pool_config_hash,
            pallas::Base::from(self.nonce),
        ]);
        CreatePoolV1PublicInputs {
            creator_pub_x: self.creator_pub_x,
            creator_pub_y: self.creator_pub_y,
            pool_config_hash: self.pool_config_hash,
            nonce: pallas::Base::from(self.nonce),
            derived_pool_id,
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.creator_pub_x)),
            Witness::Base(Value::known(self.creator_pub_y)),
            Witness::Base(Value::known(self.pool_config_hash)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
        ]
    }
}

/// Create a CreatePool ZK proof
pub fn create_pool_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreatePoolV1CallData,
) -> Result<(Proof, CreatePoolV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}