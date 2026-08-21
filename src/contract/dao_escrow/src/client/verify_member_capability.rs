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

/// VerifyMemberCapabilityV2 circuit public inputs (3 — matching V2 circuit constrain_instance order)
/// Circuit order: [tx_binding, tx_nonce, capability_commit]
#[derive(Debug, Clone)]
pub struct VerifyMemberCapabilityV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub capability_commit: pallas::Base,
}

impl VerifyMemberCapabilityV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce, self.capability_commit]
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
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyMemberCapabilityV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        capability_id: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        capability_secret: pallas::Base,
        holder_secret: pallas::Base,
    ) -> Self {
        let holder_pub = PublicKey::from_secret(dwow_sdk::crypto::SecretKey::from_base(holder_secret));
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (hx, hy) = holder_pub.xy().expect("pk not identity");
        Self {
            nullifier_k,
            capability_id,
            dao_escrow_bulla,
            capability_secret,
            holder_secret,
            holder_pub_x: hx,
            holder_pub_y: hy,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> VerifyMemberCapabilityV1PublicInputs {
        // capability_commit = poseidon_hash(DOMAIN_COIN_COMMIT, capability_id,
        //                                    capability_secret, dao_escrow_bulla)
        let capability_commit = poseidon_hash([
            pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
            self.capability_id,
            self.capability_secret,
            self.dao_escrow_bulla,
        ]);

        // Circuit constrain_instance order: [tx_binding, tx_nonce, capability_commit]
        VerifyMemberCapabilityV1PublicInputs {
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
            capability_commit,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.capability_id)),
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(self.capability_secret)),
            Witness::Base(Value::known(self.holder_secret)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
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
