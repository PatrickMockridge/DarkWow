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

//! Subscription renew ZK proof generation (`OBL-C106`).
//!
//! This module did not exist, and the harness proved `CancelV1` with `empty_witnesses` instead — a
//! fabricated proof carrying no instances, which the L2 verify refuses. `renew.zk` declares six
//! witnesses and four instances, and this client is the second half of the pair the circuit needs.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{crypto::PublicKey, pasta::pallas};
use rand::rngs::OsRng;
use rand::SeedableRng;

use crate::model::{nullifier_of, SubscriptionId};

/// `RenewV2`'s public inputs, in the circuit's `constrain_instance` order.
#[derive(Debug, Clone)]
pub struct RenewPublicInputs {
    pub subscription_id: pallas::Base,
    pub spent_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RenewPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // `renew.zk`'s order: subscription_id, spent_nullifier, tx_binding, tx_nonce.
        vec![self.subscription_id, self.spent_nullifier, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for cancel proof generation
#[derive(Debug, Clone)]
pub struct RenewCallData {
    pub subscription_id: pallas::Base,
    pub subscriber_secret: pallas::Base,
    pub new_lock_until_block: u64,
    pub value_commit: pallas::Point,
    pub merkle_proof: Vec<pallas::Base>,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RenewCallData {
    pub fn new(
        subscription_id: pallas::Base,
        subscriber_secret: pallas::Base,
        new_lock_until_block: u64,
        value_commit: pallas::Point,
    ) -> Self {
        Self {
            subscription_id,
            subscriber_secret,
            new_lock_until_block,
            value_commit,
            merkle_proof: vec![],
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Bind the proof to a transaction: the pair the witnesses and the public inputs both use, so the
    /// params and the proof agree (`OBL-C78`).
    pub fn tx_pair(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Self {
        self.tx_commitment = tx_commitment;
        self.tx_nonce = tx_nonce;
        self
    }

    pub fn compute_public_inputs(&self) -> RenewPublicInputs {
        RenewPublicInputs {
            subscription_id: self.subscription_id,
            // The derivation the circuit constrains and the host compares against — one function
            // (`model::nullifier_of`), so a fifth copy cannot disagree with it (`OBL-C106`).
            spent_nullifier: nullifier_of(
                SubscriptionId(self.subscription_id),
                self.subscriber_secret,
            ),
            tx_binding: super::tx_binding_of(&self.tx_commitment, &self.tx_nonce),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let inputs = self.compute_public_inputs();
        vec![
            // `renew.zk`'s declaration order.
            Witness::Base(Value::known(self.subscription_id)),
            Witness::Base(Value::known(self.subscriber_secret)),
            Witness::Base(Value::known(inputs.spent_nullifier)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            // The computed value, never a literal zero (`OBL-C107`): the circuit assigns
            // `tx_binding` from `tx_commitment` and `tx_nonce`, which constrains this witness.
            Witness::Base(Value::known(inputs.tx_binding)),
        ]
    }
}

/// Create a renew proof
pub fn create_renew_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RenewCallData,
) -> Result<(Proof, RenewPublicInputs)> {
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
