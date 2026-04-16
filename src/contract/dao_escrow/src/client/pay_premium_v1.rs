/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// PayPremiumV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct PayPremiumV1PublicInputs {
    pub dao_escrow_bulla: pallas::Base,
    pub current_block: pallas::Base,
    pub value: pallas::Base,
    pub token_id: pallas::Base,
    pub expiry: pallas::Base,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub computed_commit_1_x: pallas::Base,
    pub computed_commit_2_x: pallas::Base,
    pub computed_commit_3_x: pallas::Base,
    pub computed_bulla: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
}

impl PayPremiumV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.dao_escrow_bulla,
            self.current_block,
            self.value,
            self.token_id,
            self.expiry,
            self.member_pub_x,
            self.member_pub_y,
            self.computed_commit_1_x,
            self.computed_commit_2_x,
            self.computed_commit_3_x,
            self.computed_bulla,
            self.value_commit_x,
            self.value_commit_y,
        ]
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
    pub membership_blind: pallas::Scalar,
    pub value_blind: pallas::Scalar,
    pub mpc_secret_1: pallas::Scalar,
    pub mpc_secret_2: pallas::Scalar,
    pub mpc_secret_3: pallas::Scalar,
    pub max_membership_blocks: u64,
    pub max_expiry: u64,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
}

impl PayPremiumV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        current_block: u64,
        member_public: PublicKey,
        value: u64,
        token_id: pallas::Base,
        expiry: u64,
        membership_blind: pallas::Scalar,
        value_blind: pallas::Scalar,
        mpc_secret_1: pallas::Scalar,
        mpc_secret_2: pallas::Scalar,
        mpc_secret_3: pallas::Scalar,
        max_membership_blocks: u64,
        max_expiry: u64,
    ) -> Self {
        let (mx, my) = member_public.xy();
        Self {
            nullifier_k,
            dao_escrow_bulla,
            current_block,
            member_secret: pallas::Base::zero(),
            value,
            token_id,
            expiry,
            membership_blind,
            value_blind,
            mpc_secret_1,
            mpc_secret_2,
            mpc_secret_3,
            max_membership_blocks,
            max_expiry,
            member_pub_x: mx,
            member_pub_y: my,
        }
    }

    pub fn compute_public_inputs(&self) -> PayPremiumV1PublicInputs {
        // These computed values are derived inside the circuit from private witnesses
        // Using placeholder hashes here - the actual computation happens in the ZK circuit
        let computed_commit_1_x = poseidon_hash([
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(1),
        ]);
        let computed_commit_2_x = poseidon_hash([
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(2),
        ]);
        let computed_commit_3_x = poseidon_hash([
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(3),
        ]);
        let computed_bulla = poseidon_hash([
            self.dao_escrow_bulla,
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(self.value),
            pallas::Base::from(self.expiry),
        ]);
        PayPremiumV1PublicInputs {
            dao_escrow_bulla: self.dao_escrow_bulla,
            current_block: pallas::Base::from(self.current_block),
            value: pallas::Base::from(self.value),
            token_id: self.token_id,
            expiry: pallas::Base::from(self.expiry),
            member_pub_x: self.member_pub_x,
            member_pub_y: self.member_pub_y,
            computed_commit_1_x,
            computed_commit_2_x,
            computed_commit_3_x,
            computed_bulla,
            value_commit_x: pallas::Base::zero(),
            value_commit_y: pallas::Base::zero(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Private inputs
            Witness::Scalar(Value::known(self.nullifier_k)),
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(pallas::Base::from(self.current_block))),
            Witness::Base(Value::known(self.member_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.expiry))),
            Witness::Scalar(Value::known(self.membership_blind)),
            Witness::Scalar(Value::known(self.value_blind)),
            Witness::Scalar(Value::known(self.mpc_secret_1)),
            Witness::Scalar(Value::known(self.mpc_secret_2)),
            Witness::Scalar(Value::known(self.mpc_secret_3)),
            Witness::Base(Value::known(pallas::Base::from(self.max_membership_blocks))),
            Witness::Base(Value::known(pallas::Base::from(self.max_expiry))),
            Witness::Base(Value::known(self.member_pub_x)),
            Witness::Base(Value::known(self.member_pub_y)),
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
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}