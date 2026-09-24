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

//! Insurance Market UnderwriteWithCapability ZK proof generation

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

/// UnderwriteWithCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct UnderwriteWithCapabilityV1PublicInputs {
    pub underwriter_pub_x: pallas::Base,
    pub underwriter_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub required_capability_id: pallas::Base,
}

impl UnderwriteWithCapabilityV1PublicInputs {
    /// The five values the circuit's `constrain_instance` calls publish, in its order:
    /// `underwriter_pub_x, underwriter_pub_y, tx_binding, tx_nonce, required_capability_id`.
    ///
    /// Eight until 2026-09-24, for the same reason as its `purchase_coverage_with_capability`
    /// sibling: `capability_predicate_result` and `derived_pub_x/y` are witnesses of the V2 circuit,
    /// not instances of it, and their presence displaced the three that are. See that file's note.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.underwriter_pub_x,
            self.underwriter_pub_y,
            self.tx_binding,
            self.tx_nonce,
            self.required_capability_id,
        ]
    }
}

/// Input data for UnderwriteWithCapability proof generation
#[derive(Debug, Clone)]
pub struct UnderwriteWithCapabilityV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub underwriter_secret: pallas::Base,
    pub underwriter_pub_x: pallas::Base,
    pub underwriter_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub capability_predicate_result: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UnderwriteWithCapabilityV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        underwriter_secret: pallas::Base,
        underwriter_public: PublicKey,
        required_capability_id: pallas::Base,
        capability_predicate_result: pallas::Base,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ux, uy) = underwriter_public.xy().expect("pk not identity");
        Self {
            nullifier_k,
            underwriter_secret,
            underwriter_pub_x: ux,
            underwriter_pub_y: uy,
            required_capability_id,
            capability_predicate_result,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// The circuit's own derivation, host-side: `tx_binding = poseidon_hash([3, tx_commitment,
    /// tx_nonce])` (`DOMAIN_TX_BINDING = witness_base(3)` in the `.zk`). The two `derived_pub_*`
    /// values this used to compute are the *circuit's* `ec_mul_base(underwriter_secret, NULLIFIER_K)`
    /// bound to `underwriter_pub_x/y` by `constrain_equal_base` — they are not public inputs, so
    /// computing them here produced two values the verifier never asks for.
    pub fn compute_public_inputs(&self) -> UnderwriteWithCapabilityV1PublicInputs {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        UnderwriteWithCapabilityV1PublicInputs {
            underwriter_pub_x: self.underwriter_pub_x,
            underwriter_pub_y: self.underwriter_pub_y,
            tx_binding,
            tx_nonce: self.tx_nonce,
            required_capability_id: self.required_capability_id,
        }
    }

    /// The circuit's witnesses, in the order its `witness` block declares them: `underwriter_secret,
    /// underwriter_pub_x, underwriter_pub_y, required_capability_id, capability_predicate_result,
    /// tx_commitment, tx_nonce, tx_binding` — eight, where this emitted nine beginning with a
    /// `Witness::Scalar(nullifier_k)` the circuit does not declare (`NULLIFIER_K` is a constant
    /// there). See the sibling client's note for why `nullifier_k` remains a parameter of `new`.
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();
        vec![
            // Private inputs
            Witness::Base(Value::known(self.underwriter_secret)),
            Witness::Base(Value::known(self.underwriter_pub_x)),
            Witness::Base(Value::known(self.underwriter_pub_y)),
            Witness::Base(Value::known(self.required_capability_id)),
            Witness::Base(Value::known(self.capability_predicate_result)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(public_inputs.tx_binding)),
        ]
    }
}

/// Create an UnderwriteWithCapability ZK proof
pub fn underwrite_with_capability_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &UnderwriteWithCapabilityV1CallData,
) -> Result<(Proof, UnderwriteWithCapabilityV1PublicInputs)> {
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