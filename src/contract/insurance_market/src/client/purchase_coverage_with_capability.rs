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

//! Insurance Market PurchaseCoverageWithCapability ZK proof generation

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

/// PurchaseCoverageWithCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PurchaseCoverageWithCapabilityV1PublicInputs {
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub buyer_nullifier: pallas::Base,
}

impl PurchaseCoverageWithCapabilityV1PublicInputs {
    /// The six values the circuit's `constrain_instance` calls publish, in its order:
    /// `buyer_pub_x, buyer_pub_y, tx_binding, tx_nonce, required_capability_id, buyer_nullifier`.
    ///
    /// This emitted **eight** values until 2026-09-24, four of which belonged to a V1 design that no
    /// longer exists — `capability_predicate_result` and `derived_pub_x/y` are not instances of the
    /// V2 circuit, and the two positions they occupied displaced `tx_binding`, `tx_nonce` and
    /// `buyer_nullifier`. So a proof would have been created over a vector the verifier never asks
    /// for, which is the failure the metadata gate names in its own words ("the proof would be
    /// created over a different public-input vector than the verifier uses") and the class the
    /// V1-vs-V2 hazard memory records for auction and dao_escrow.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.tx_binding,
            self.tx_nonce,
            self.required_capability_id,
            self.buyer_nullifier,
        ]
    }
}

/// Input data for PurchaseCoverageWithCapability proof generation
#[derive(Debug, Clone)]
pub struct PurchaseCoverageWithCapabilityV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub buyer_secret: pallas::Base,
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub required_capability_id: pallas::Base,
    pub capability_predicate_result: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PurchaseCoverageWithCapabilityV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        buyer_secret: pallas::Base,
        buyer_public: PublicKey,
        required_capability_id: pallas::Base,
        capability_predicate_result: pallas::Base,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (bx, by) = buyer_public.xy().expect("pk not identity");
        Self {
            nullifier_k,
            buyer_secret,
            buyer_pub_x: bx,
            buyer_pub_y: by,
            required_capability_id,
            capability_predicate_result,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// The circuit's own derivations, host-side: `buyer_nullifier` is
    /// `poseidon_hash([4, buyer_pub_x, buyer_pub_y, buyer_secret])` (`DOMAIN_COMMITMENT =
    /// witness_base(4)` in the `.zk`) and `tx_binding` is `poseidon_hash([3, tx_commitment,
    /// tx_nonce])`. The circuit *binds* the nullifier with `constrain_equal_base`, so a client that
    /// computed it differently would produce an unsatisfiable proof rather than a wrong one.
    pub fn compute_public_inputs(&self) -> PurchaseCoverageWithCapabilityV1PublicInputs {
        let buyer_nullifier = poseidon_hash([
            pallas::Base::from(4u64),
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.buyer_secret,
        ]);
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        PurchaseCoverageWithCapabilityV1PublicInputs {
            buyer_pub_x: self.buyer_pub_x,
            buyer_pub_y: self.buyer_pub_y,
            tx_binding,
            tx_nonce: self.tx_nonce,
            required_capability_id: self.required_capability_id,
            buyer_nullifier,
        }
    }

    /// The circuit's witnesses, in the order its `witness` block declares them:
    /// `buyer_secret, buyer_pub_x, buyer_pub_y, required_capability_id, capability_predicate_result,
    /// buyer_nullifier, tx_commitment, tx_nonce, tx_binding`.
    ///
    /// The list used to lead with `Witness::Scalar(nullifier_k)`, which the circuit does not declare —
    /// `NULLIFIER_K` is a `constant` there, not a witness — so every witness after it was offset by
    /// one and the wrong type. `nullifier_k` stays a parameter of `new` for its callers (the test
    /// harness passes it positionally) and is no longer read, which is the honest half of a
    /// signature this unit could not change without editing a directory another session holds.
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();
        vec![
            // Private inputs
            Witness::Base(Value::known(self.buyer_secret)),
            Witness::Base(Value::known(self.buyer_pub_x)),
            Witness::Base(Value::known(self.buyer_pub_y)),
            Witness::Base(Value::known(self.required_capability_id)),
            Witness::Base(Value::known(self.capability_predicate_result)),
            Witness::Base(Value::known(public_inputs.buyer_nullifier)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(public_inputs.tx_binding)),
        ]
    }
}

/// Create a PurchaseCoverageWithCapability ZK proof
pub fn purchase_coverage_with_capability_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PurchaseCoverageWithCapabilityV1CallData,
) -> Result<(Proof, PurchaseCoverageWithCapabilityV1PublicInputs)> {
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