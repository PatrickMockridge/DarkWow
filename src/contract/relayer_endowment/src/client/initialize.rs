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

//! Relayer Endowment initialize_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{crypto::PublicKey, pasta::pallas};

use crate::model::{derive_config_hash, derive_endowment_id, derive_tx_binding};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// InitializeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct InitializeV1PublicInputs {
    pub endowment_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl InitializeV1PublicInputs {
    /// The instances in `constrain_instance` order, which is what `Proof::create`
    /// and `verify_proof` index the instance column by.
    ///
    /// `initialize.zk` constrains `tx_binding`, then `tx_nonce`, then
    /// `derived_endowment_id` — and the contract's `…_initialize_get_metadata_v1`
    /// publishes them in that same order. This vector used to be
    /// `[endowment_id, tx_binding, tx_nonce]`, which is not that order; it is the
    /// order the *circuit's own witness block* is written in, which is a different
    /// list. A proof built from it fails verify with `invalid proof`, naming neither
    /// the ordering nor the circuit.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce, self.endowment_id]
    }
}

/// Input data for initialize proof generation
#[derive(Debug, Clone)]
pub struct InitializeV1CallData {
    pub relayer_public: PublicKey,
    pub default_backer_cut_bp: u32,
    /// The **verifying block height** — the height of the block this call will land in. The
    /// `InitializeV2` instance contains it, so a client must know it: the chain tip plus one
    /// when the transaction is built. It is not a free nonce and cannot be chosen.
    pub nonce: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl InitializeV1CallData {
    pub fn new(relayer_public: PublicKey, default_backer_cut_bp: u32, nonce: u64) -> Self {
        Self {
            relayer_public,
            default_backer_cut_bp,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> InitializeV1PublicInputs {
        // Both ids come from `crate::model`, which is the same code the contract's metadata
        // and its exec path run — deriving them here is what keeps the three in step.
        let endowment_id =
            derive_endowment_id(&self.relayer_public, self.default_backer_cut_bp, self.nonce);
        InitializeV1PublicInputs {
            endowment_id,
            tx_binding: derive_tx_binding(self.tx_commitment, self.tx_nonce),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (px, py) = self.relayer_public.xy().expect("pk not identity");
        vec![
            Witness::Base(Value::known(px)),
            Witness::Base(Value::known(py)),
            Witness::Base(Value::known(derive_config_hash(self.default_backer_cut_bp))),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(derive_tx_binding(self.tx_commitment, self.tx_nonce))), // tx_binding
        ]
    }
}

/// Create a Initialize ZK proof
pub fn initialize_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &InitializeV1CallData,
) -> Result<(Proof, InitializeV1PublicInputs)> {
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
