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

//! Insurance Market PurchaseCoverage (selector 0x04) ZK proof generation
//!
//! Written to match `purchase_coverage.zk` exactly; the sibling
//! `purchase_coverage_with_capability.rs` is the file this was copied from, and it works from the same
//! shape. Two things are specific to THIS circuit and easy to get wrong:
//!
//! * the nullifier's domain constant is `DOMAIN_NULLIFIER = witness_base(1)` — the literal **1** —
//!   and the preimage carries **five** values, ending in `purchase_nonce`. The sibling uses domain 4
//!   and four values, so neither the constant nor the arity transfers.
//! * the instances are published in the order `buyer_pub_x, buyer_pub_y, buyer_nullifier, tx_binding,
//!   tx_nonce` — the nullifier is third here and last in the sibling's list.
//!
//! `witness_base(N)` is a literal constant, not a witness index (`src/zkas/parser.rs`), which is why
//! the domain is 1 even though `buyer_secret` occupies witness index 0.

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

/// PurchaseCoverageV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PurchaseCoverageV1PublicInputs {
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub buyer_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PurchaseCoverageV1PublicInputs {
    /// The five values the circuit's `constrain_instance` calls publish, in its order
    /// (`purchase_coverage.zk:23-26`). `purchase_coverage_get_metadata_v1` publishes the same five in
    /// the same order, so a proof whose vector disagrees with this one is a proof over a vector the
    /// verifier never asks for.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.buyer_nullifier,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for PurchaseCoverage proof generation
#[derive(Debug, Clone)]
pub struct PurchaseCoverageV1CallData {
    pub buyer_secret: pallas::Base,
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub purchase_nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PurchaseCoverageV1CallData {
    /// No `nullifier_k` parameter, unlike the two sibling clients: those carry one for legacy callers
    /// that pass it positionally, and the circuit declares `NULLIFIER_K` as a constant rather than a
    /// witness. This file has no callers to keep compatible.
    pub fn new(
        buyer_secret: pallas::Base,
        buyer_public: PublicKey,
        purchase_nonce: pallas::Base,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (bx, by) = buyer_public.xy().expect("pk not identity");
        Self {
            buyer_secret,
            buyer_pub_x: bx,
            buyer_pub_y: by,
            purchase_nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// The circuit's own derivations, host-side. The circuit *binds* the nullifier with
    /// `constrain_equal_base(computed_nullifier, buyer_nullifier)`, so a client that computed it
    /// differently would produce a proof that cannot satisfy the circuit rather than a wrong one.
    pub fn compute_public_inputs(&self) -> PurchaseCoverageV1PublicInputs {
        let buyer_nullifier = poseidon_hash([
            pallas::Base::from(1u64),
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.buyer_secret,
            self.purchase_nonce,
        ]);
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        PurchaseCoverageV1PublicInputs {
            buyer_pub_x: self.buyer_pub_x,
            buyer_pub_y: self.buyer_pub_y,
            buyer_nullifier,
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    /// The circuit's witnesses, in the order its `witness` block declares them
    /// (`purchase_coverage.zk:4-11`): `buyer_secret, buyer_pub_x, buyer_pub_y, buyer_nullifier,
    /// tx_commitment, tx_nonce, tx_binding, purchase_nonce` — **eight**, with the nullifier third and
    /// `purchase_nonce` last.
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();
        vec![
            Witness::Base(Value::known(self.buyer_secret)),
            Witness::Base(Value::known(self.buyer_pub_x)),
            Witness::Base(Value::known(self.buyer_pub_y)),
            Witness::Base(Value::known(public_inputs.buyer_nullifier)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(public_inputs.tx_binding)),
            Witness::Base(Value::known(self.purchase_nonce)),
        ]
    }
}

/// Create a PurchaseCoverage ZK proof
pub fn purchase_coverage_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PurchaseCoverageV1CallData,
) -> Result<(Proof, PurchaseCoverageV1PublicInputs)> {
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
