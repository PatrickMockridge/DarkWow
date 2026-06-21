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
    crypto::{pedersen_commitment_u64, pasta_prelude::{Curve, CurveAffine}, poseidon_hash, Blind, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

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
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_deployment_id, self.value_commit_x, self.value_commit_y, self.tx_binding,
            self.tx_nonce]
    }
}

/// Input data for deploy_capital proof generation
#[derive(Debug, Clone)]
pub struct DeployCapitalV1CallData {
    pub endowment_id: pallas::Base,
    pub backer_pub_x: pallas::Base,
    pub backer_pub_y: pallas::Base,
    pub deploy_amount: pallas::Base,
    pub deploy_amount_u64: u64,
    pub token_id: pallas::Base,
    pub nonce: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DeployCapitalV1CallData {
    pub fn new(
        endowment_id: pallas::Base,
        backer_public: PublicKey,
        deploy_amount: u64,
        token_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        let (bx, by) = backer_public.xy();
        Self {
            endowment_id,
            backer_pub_x: bx,
            backer_pub_y: by,
            deploy_amount: pallas::Base::from(deploy_amount),
            deploy_amount_u64: deploy_amount,
            token_id,
            nonce: pallas::Base::from(nonce),
            value_blind,
            tx_commitment: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> DeployCapitalV1PublicInputs {
        let derived_deployment_id = poseidon_hash([
            self.endowment_id,
            self.backer_pub_x,
            self.backer_pub_y,
            self.deploy_amount,
            self.nonce,
        ]);

        let value_commit = pedersen_commitment_u64(self.deploy_amount_u64, Blind(self.value_blind));
        let value_coords = value_commit.to_affine().coordinates().expect("Value commitment cannot be the identity element");

        DeployCapitalV1PublicInputs {
            derived_deployment_id,
            value_commit_x: *value_coords.x(),
            value_commit_y: *value_coords.y(),
            tx_binding: poseidon_hash([self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.endowment_id)),
            Witness::Base(Value::known(self.backer_pub_x)),
            Witness::Base(Value::known(self.backer_pub_y)),
            Witness::Base(Value::known(self.deploy_amount)),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Scalar(Value::known(self.value_blind)),
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
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
