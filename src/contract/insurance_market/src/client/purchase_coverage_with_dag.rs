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

//! Insurance Market PurchaseCoverageWithDAG (selector 0x0b) ZK proof generation
//!
//! Written to match `purchase_coverage_with_dag.zk`. It is `purchase_coverage.rs`'s sibling minus one
//! value: this circuit has **no** `purchase_nonce` witness, and its nullifier is
//! `poseidon_hash([4, buyer_pub_x, buyer_pub_y, buyer_secret])` where the 0x04 circuit uses domain 1
//! and appends a nonce. The instances are the same five in the same order.

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

/// PurchaseCoverageWithDAGV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PurchaseCoverageWithDAGV1PublicInputs {
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub buyer_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PurchaseCoverageWithDAGV1PublicInputs {
    /// The five values the circuit's `constrain_instance` calls publish, in its order
    /// (`purchase_coverage_with_dag.zk:19-22`).
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

/// Input data for PurchaseCoverageWithDAG proof generation
#[derive(Debug, Clone)]
pub struct PurchaseCoverageWithDAGV1CallData {
    pub buyer_secret: pallas::Base,
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PurchaseCoverageWithDAGV1CallData {
    pub fn new(buyer_secret: pallas::Base, buyer_public: PublicKey) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (bx, by) = buyer_public.xy().expect("pk not identity");
        Self {
            buyer_secret,
            buyer_pub_x: bx,
            buyer_pub_y: by,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> PurchaseCoverageWithDAGV1PublicInputs {
        let buyer_nullifier = poseidon_hash([
            pallas::Base::from(4u64),
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.buyer_secret,
        ]);
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        PurchaseCoverageWithDAGV1PublicInputs {
            buyer_pub_x: self.buyer_pub_x,
            buyer_pub_y: self.buyer_pub_y,
            buyer_nullifier,
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    /// The circuit's witnesses, in the order its `witness` block declares them
    /// (`purchase_coverage_with_dag.zk:4-9`): `buyer_secret, buyer_pub_x, buyer_pub_y,
    /// buyer_nullifier, tx_commitment, tx_nonce, tx_binding` — **seven**, one fewer than 0x04.
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
        ]
    }
}

/// Create a PurchaseCoverageWithDAG ZK proof
pub fn purchase_coverage_with_dag_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PurchaseCoverageWithDAGV1CallData,
) -> Result<(Proof, PurchaseCoverageWithDAGV1PublicInputs)> {
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
