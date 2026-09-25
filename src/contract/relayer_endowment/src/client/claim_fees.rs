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

//! Relayer Endowment claim_fees_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{crypto::PublicKey, pasta::pallas};

use crate::model::{derive_claim_id, derive_tx_binding};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// ClaimFeesV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ClaimFeesV1PublicInputs {
    pub derived_claim_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ClaimFeesV1PublicInputs {
    /// The instances in `constrain_instance` order. `claim_fees.zk` constrains
    /// `tx_binding`, then `tx_nonce`, then `derived_claim_id` — the same order the
    /// contract's `…_claim_fees_get_metadata_v1` publishes.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce, self.derived_claim_id]
    }
}

/// Input data for claim_fees proof generation
#[derive(Debug, Clone)]
pub struct ClaimFeesV1CallData {
    pub deployment_id: pallas::Base,
    pub backer_public: PublicKey,
    pub fee_share: u64,
    /// The **verifying block height** — see `InitializeV1CallData::nonce`.
    pub nonce: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ClaimFeesV1CallData {
    pub fn new(
        deployment_id: pallas::Base,
        backer_public: PublicKey,
        fee_share: u64,
        nonce: u64,
    ) -> Self {
        Self {
            deployment_id,
            backer_public,
            fee_share,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// The backer's public key coordinates exactly as `ClaimFeesParamsV1` carries them — the
    /// contract's metadata reads them out of the params and hashes them, so the params and the
    /// proof must name the same point.
    pub fn backer_pub_xy(&self) -> (pallas::Base, pallas::Base) {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        self.backer_public.xy().expect("pk not identity")
    }

    pub fn compute_public_inputs(&self) -> ClaimFeesV1PublicInputs {
        let (bx, by) = self.backer_pub_xy();
        let derived_claim_id =
            derive_claim_id(self.deployment_id, bx, by, self.fee_share, self.nonce);
        ClaimFeesV1PublicInputs { derived_claim_id, tx_binding: derive_tx_binding(self.tx_commitment, self.tx_nonce), tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (bx, by) = self.backer_pub_xy();
        vec![
            Witness::Base(Value::known(self.deployment_id)),
            Witness::Base(Value::known(bx)),
            Witness::Base(Value::known(by)),
            Witness::Base(Value::known(pallas::Base::from(self.fee_share))),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(derive_tx_binding(self.tx_commitment, self.tx_nonce))), // tx_binding
        ]
    }
}

/// Create a ClaimFees ZK proof
pub fn claim_fees_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ClaimFeesV1CallData,
) -> Result<(Proof, ClaimFeesV1PublicInputs)> {
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
