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

//! DAO-Escrow VerifyMemberCapability ZK proof generation

use dwow::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// VerifyMemberCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct VerifyMemberCapabilityV1PublicInputs {
    pub capability_id: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub holder_commit: pallas::Base,
}

impl VerifyMemberCapabilityV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.capability_id,
            self.dao_escrow_bulla,
            self.holder_commit,
        ]
    }
}

/// Input data for VerifyMemberCapability proof generation
#[derive(Debug, Clone)]
pub struct VerifyMemberCapabilityV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub capability_id: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub capability_secret: pallas::Base,
    pub holder_secret: pallas::Base,
    pub holder_pub_x: pallas::Base,
    pub holder_pub_y: pallas::Base,
}

impl VerifyMemberCapabilityV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        capability_id: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        capability_secret: pallas::Base,
        holder_secret: pallas::Base,
    ) -> Self {
        let holder_pub = PublicKey::from_secret(
            dwow_sdk::crypto::SecretKey::from_bytes(holder_secret.to_repr()).unwrap()
        );
        let (hx, hy) = holder_pub.xy();
        Self {
            nullifier_k,
            capability_id,
            dao_escrow_bulla,
            capability_secret,
            holder_secret,
            holder_pub_x: hx,
            holder_pub_y: hy,
        }
    }

    pub fn compute_public_inputs(&self) -> VerifyMemberCapabilityV1PublicInputs {
        let holder_commit = poseidon_hash([
            self.holder_pub_x,
            self.holder_pub_y,
            self.capability_secret,
        ]);

        VerifyMemberCapabilityV1PublicInputs {
            capability_id: self.capability_id,
            dao_escrow_bulla: self.dao_escrow_bulla,
            holder_commit,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.capability_id)),
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(self.capability_secret)),
            Witness::Base(Value::known(self.holder_secret)),
        ]
    }
}

/// Create a VerifyMemberCapability ZK proof
pub fn verify_member_capability_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VerifyMemberCapabilityV1CallData,
) -> Result<(Proof, VerifyMemberCapabilityV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
