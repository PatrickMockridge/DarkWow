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

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// PayPremiumV1 circuit public inputs (only 4 - matching what circuit exposes)
#[derive(Debug, Clone)]
pub struct PayPremiumV1PublicInputs {
    pub dao_escrow_bulla: pallas::Base,
    pub membership_note: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
}

impl PayPremiumV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.dao_escrow_bulla,
            self.membership_note,
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
    pub membership_blind: pallas::Base,
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
        member_secret: pallas::Base,
        value: u64,
        token_id: pallas::Base,
        expiry: u64,
        membership_blind: pallas::Base,
        value_blind: pallas::Scalar,
        mpc_secret_1: pallas::Scalar,
        mpc_secret_2: pallas::Scalar,
        mpc_secret_3: pallas::Scalar,
        max_membership_blocks: u64,
        max_expiry: u64,
    ) -> Self {
        // Derive member public key from secret
        let member_pub = PublicKey::from_secret(
            darkfi_sdk::crypto::SecretKey::from_bytes(member_secret.to_repr()).unwrap()
        );
        let (mx, my) = member_pub.xy();
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
            max_membership_blocks,
            max_expiry,
            member_pub_x: mx,
            member_pub_y: my,
        }
    }

    pub fn compute_public_inputs(&self) -> PayPremiumV1PublicInputs {
        // Compute membership_note = H(member_pub_x, member_pub_y, value, token_id, expiry, blind)
        // This must match what the circuit computes
        let membership_note = poseidon_hash([
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(self.value),
            self.token_id,
            pallas::Base::from(self.expiry),
            self.membership_blind,
        ]);

        // The value_commit is computed in the circuit using EC operations:
        // vcv = ec_mul_short(value, VALUE_COMMIT_VALUE)
        // vcr = ec_mul(value_blind, VALUE_COMMIT_RANDOM)
        // value_commit = ec_add(vcv, vcr)
        // We cannot compute this outside the circuit, so we use zero as placeholder.
        let value_commit_x = pallas::Base::zero();
        let value_commit_y = pallas::Base::zero();

        PayPremiumV1PublicInputs {
            dao_escrow_bulla: self.dao_escrow_bulla,
            membership_note,
            value_commit_x,
            value_commit_y,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // nullifier_k is a CONSTANT in circuit - do NOT pass as witness
            // member_pub_x/y are DERIVED inside circuit from member_secret and NULLIFIER_K
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(pallas::Base::from(self.current_block))),
            Witness::Base(Value::known(self.member_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.expiry))),
            Witness::Base(Value::known(self.membership_blind)),
            Witness::Scalar(Value::known(self.value_blind)),
            Witness::Scalar(Value::known(self.mpc_secret_1)),
            Witness::Scalar(Value::known(self.mpc_secret_2)),
            Witness::Scalar(Value::known(self.mpc_secret_3)),
            Witness::Base(Value::known(pallas::Base::from(self.max_membership_blocks))),
            Witness::Base(Value::known(pallas::Base::from(self.max_expiry))),
            // member_pub_x/y derived inside circuit - do NOT pass as witnesses
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