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

//! DAO-Escrow PayPremium ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// PayPremiumV2 circuit public inputs (2 — matching PayPremiumV2 circuit constrain_instance order)
/// Circuit order: [tx_binding, tx_nonce]
#[derive(Debug, Clone)]
pub struct PayPremiumV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PayPremiumV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce]
    }
}

/// Input data for PayPremium proof generation
#[derive(Debug, Clone)]
pub struct PayPremiumV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub dao_escrow_bulla: pallas::Base,
    pub current_block: u64,
    pub member_secret: pallas::Base,
    pub value: u64,
    pub token_id: pallas::Base,
    pub expiry: u64,
    pub membership_blind: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub mpc_secret_1: pallas::Base,
    pub mpc_secret_2: pallas::Base,
    pub mpc_secret_3: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PayPremiumV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        current_block: u64,
        member_secret: pallas::Base,
        value: u64,
        token_id: pallas::Base,
        expiry: u64,
        membership_blind: pallas::Base,
        value_blind: pallas::Scalar,
        mpc_secret_1: pallas::Base,
        mpc_secret_2: pallas::Base,
        mpc_secret_3: pallas::Base,
    ) -> Self {
        // Derive member public key from secret
        let member_pub = PublicKey::from_secret(
            dwow_sdk::crypto::SecretKey::from_bytes(member_secret.to_repr()).unwrap()
        );
        let (mx, my) = member_pub.xy().expect("pk not identity");
        Self {
            nullifier_k,
            dao_escrow_bulla,
            current_block,
            member_secret,
            value,
            token_id,
            expiry,
            membership_blind,
            value_blind,
            mpc_secret_1,
            mpc_secret_2,
            mpc_secret_3,
            member_pub_x: mx,
            member_pub_y: my,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> PayPremiumV1PublicInputs {
        // Circuit constrain_instance order: [tx_binding, tx_nonce]
        PayPremiumV1PublicInputs {
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // nullifier_k is a CONSTANT in circuit - do NOT pass as witness
            // max_membership_blocks/max_expiry are INTERNAL to circuit (witness_base, base_add)
            // member_pub_x/y are DERIVED inside circuit from member_secret and NULLIFIER_K
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(pallas::Base::from(self.current_block))),
            Witness::Base(Value::known(self.member_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.expiry))),
            Witness::Base(Value::known(self.membership_blind)),
            Witness::Scalar(Value::known(self.value_blind)),
            // mpc_secret_* are declared as Base in the circuit
            Witness::Base(Value::known(self.mpc_secret_1)),
            Witness::Base(Value::known(self.mpc_secret_2)),
            Witness::Base(Value::known(self.mpc_secret_3)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a PayPremium ZK proof
pub fn pay_premium_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PayPremiumV1CallData,
) -> Result<(Proof, PayPremiumV1PublicInputs)> {
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