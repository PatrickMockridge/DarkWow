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

//! Deployooor `lock_v1` ZK proof generation.
//!
//! The `LockV1` doc promised a deployer-only lock and the code enforced nothing (docs-review
//! F-511). Signatures are prohibited in contract metadata here, so the gate is a proof of the
//! deployer's secret: the `LockV2` circuit derive-and-exposes the public-key coordinates from the
//! secret, and the host derives the same `ContractId` from those coordinates that `DeployV1`
//! stored — so only a caller who can open the key can lock the contract.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, Keypair, PublicKey},
    pasta::pallas,
};

use crate::model::LockParamsV1;

/// `LockV2` public inputs, in circuit `constrain_instance` order: the deployer's public-key
/// coordinates, then the tx pair last (`OBL-C198`).
#[derive(Debug, Clone)]
pub struct LockV1PublicInputs {
    pub public_key_x: pallas::Base,
    pub public_key_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl LockV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.public_key_x, self.public_key_y, self.tx_binding, self.tx_nonce]
    }
}

/// Witnesses for the `LockV2` proof.
#[derive(Debug, Clone)]
pub struct LockV1CallData {
    pub deployer_secret: pallas::Base,
    // Public inputs
    pub deployer_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl LockV1CallData {
    pub fn new(deployer_secret: pallas::Base, deployer_public: PublicKey, tx_nonce: pallas::Base) -> Self {
        Self { deployer_secret, deployer_public, tx_commitment: pallas::Base::zero(), tx_nonce }
    }

    pub fn compute_public_inputs(&self) -> LockV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (ix, iy) = self.deployer_public.xy().expect("pk not identity");
        // Circuit: DOMAIN_TX_BINDING = witness_base(3)
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        LockV1PublicInputs { public_key_x: ix, public_key_y: iy, tx_binding, tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (ix, iy) = self.deployer_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        // Must match `lock.zk`'s witness order:
        // deployer_secret, public_key_x, public_key_y, tx_commitment, tx_nonce, tx_binding
        vec![
            Witness::Base(Value::known(self.deployer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ]
    }
}

/// A prepared `LockV1` call: the params and witnesses are built, but the proof waits for the tx
/// commitment — the frame the call lands in, which cannot be known until the call exists.
pub struct LockPlan {
    input: LockV1CallData,
    params: LockParamsV1,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl LockPlan {
    pub fn params(&self) -> &LockParamsV1 {
        &self.params
    }

    /// Prove against the frame commitment, and return the call debris.
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<LockCallDebris> {
        let mut input = self.input;
        input.tx_commitment = tx_commitment;
        let public_inputs = input.compute_public_inputs();
        let witnesses = input.to_witnesses();

        let circuit = ZkCircuit::new(witnesses, &self.zkbin);
        let proof = Proof::create(&self.pk, &[circuit], &public_inputs.to_vec(), &mut rand::rngs::OsRng)?;

        Ok(LockCallDebris { params: self.params, proof, public_inputs })
    }
}

/// A built `Deployooor::LockV1` call: the params to encode, the proof, and its public inputs.
pub struct LockCallDebris {
    pub params: LockParamsV1,
    pub proof: Proof,
    pub public_inputs: LockV1PublicInputs,
}

/// Prepare a `Deployooor::LockV1` call over the deployer's keypair. `deployer_secret` is the key
/// `LockV2` proves knowledge of; `tx_nonce` binds the proof to the transaction.
pub fn build_lock_call(
    deploy_keypair: Keypair,
    deployer_secret: pallas::Base,
    tx_nonce: pallas::Base,
    zkbin: ZkBinary,
    pk: ProvingKey,
) -> Result<LockPlan> {
    tracing::debug!(target: "contract::deployooor::client::lock", "Preparing Deployooor::LockV1 contract call");

    let input = LockV1CallData::new(deployer_secret, deploy_keypair.public, tx_nonce);
    let params = LockParamsV1 { public_key: deploy_keypair.public, tx_nonce };

    Ok(LockPlan { input, params, zkbin, pk })
}
