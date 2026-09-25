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

//! Relayer Endowment deploy_capital_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64, Blind, PublicKey,
    },
    pasta::pallas,
};

use crate::model::{derive_deployment_id, derive_endowment_id, derive_tx_binding};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// DeployCapitalV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct DeployCapitalV1PublicInputs {
    pub derived_deployment_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DeployCapitalV1PublicInputs {
    /// The instances in `constrain_instance` order. `deploy_capital.zk` constrains
    /// `derived_deployment_id`, the commitment's `x`, `tx_binding`, `tx_nonce`, then
    /// the commitment's `y` — the value commitment is **split by the binding pair**,
    /// not written as an `(x, y)` adjacency. The contract's
    /// `…_deploy_capital_get_metadata_v1` publishes the same order.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_deployment_id, self.value_commit_x, self.tx_binding, self.tx_nonce, self.value_commit_y]
    }
}

/// Input data for deploy_capital proof generation
#[derive(Debug, Clone)]
pub struct DeployCapitalV1CallData {
    pub relayer_public: PublicKey,
    pub backer_public: PublicKey,
    pub backer_cut_bp: u32,
    pub deploy_amount: u64,
    pub asset_id: pallas::Base,
    /// The **verifying block height** — see `InitializeV1CallData::nonce`.
    pub nonce: u64,
    pub value_blind: pallas::Scalar,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DeployCapitalV1CallData {
    /// `endowment_id` is **derived here, not taken as an argument**. It was a parameter, and a
    /// caller that passed the wrong one — a placeholder, or an id computed before the block it
    /// lands in was known — produced a proof for an endowment that does not exist, with no
    /// error naming the argument. It is a function of the relayer, the cut and the height, so
    /// the client derives it.
    pub fn new(
        relayer_public: PublicKey,
        backer_public: PublicKey,
        backer_cut_bp: u32,
        deploy_amount: u64,
        asset_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        Self {
            relayer_public,
            backer_public,
            backer_cut_bp,
            deploy_amount,
            asset_id,
            nonce,
            value_blind,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn endowment_id(&self) -> pallas::Base {
        derive_endowment_id(&self.relayer_public, self.backer_cut_bp, self.nonce)
    }

    pub fn compute_public_inputs(&self) -> DeployCapitalV1PublicInputs {
        // `crate::model`'s derivation, which is the one the exec path stores under and the
        // metadata publishes — see the section there.
        let derived_deployment_id = derive_deployment_id(
            &self.relayer_public,
            &self.backer_public,
            self.backer_cut_bp,
            self.deploy_amount,
            self.nonce,
        );

        let value_commit = pedersen_commitment_u64(self.deploy_amount, Blind(self.value_blind));
        let value_coords = value_commit.to_affine().coordinates().expect("Value commitment cannot be the identity element");

        DeployCapitalV1PublicInputs {
            derived_deployment_id,
            value_commit_x: *value_coords.x(),
            value_commit_y: *value_coords.y(),
            tx_binding: derive_tx_binding(self.tx_commitment, self.tx_nonce),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (bx, by) = self.backer_public.xy().expect("pk not identity");
        vec![
            Witness::Base(Value::known(self.endowment_id())),
            Witness::Base(Value::known(bx)),
            Witness::Base(Value::known(by)),
            Witness::Base(Value::known(pallas::Base::from(self.deploy_amount))),
            Witness::Base(Value::known(self.asset_id)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            Witness::Scalar(Value::known(self.value_blind)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(derive_tx_binding(self.tx_commitment, self.tx_nonce))), // tx_binding
        ]
    }
}

/// Create a DeployCapital ZK proof
pub fn deploy_capital_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &DeployCapitalV1CallData,
) -> Result<(Proof, DeployCapitalV1PublicInputs)> {
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
