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

//! Subscription verify_access ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// VerifyAccessV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct VerifyAccessPublicInputs {
    pub expected_capability: pallas::Base,
    pub subscription_id: pallas::Base,
    pub current_block: u64,
    pub subscriber_pub_x: pallas::Base,
    pub subscriber_pub_y: pallas::Base,
    pub plan_id: u32,
    pub lock_until_block: u64,
    pub uses_allowed: u64,
    pub rate_period: u64,
    pub period_uses: u64,
    pub last_access_block: u64,
    pub uses_remaining: u64,
    pub subscription_state_root: pallas::Base,
}

impl VerifyAccessPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Circuit has no constrain_instance calls — zero public inputs.
        vec![]
    }
}

/// Input data for verify_access proof generation
#[derive(Debug, Clone)]
pub struct VerifyAccessCallData {
    pub subscriber_secret: pallas::Base,
    pub nonce: pallas::Base,
    pub permissions_claimed: u8,
    pub subscription_leaf_pos: u32,
    pub subscription_path: Vec<MerkleNode>,
    pub subscription_state: pallas::Base,
    pub subscription_spent_nullifier: pallas::Base,
    // Public inputs
    pub expected_capability: pallas::Base,
    pub subscription_id: pallas::Base,
    pub current_block: u64,
    pub subscriber_pub_x: pallas::Base,
    pub subscriber_pub_y: pallas::Base,
    pub plan_id: u32,
    pub lock_until_block: u64,
    pub uses_allowed: u64,
    pub rate_period: u64,
    pub period_uses: u64,
    pub last_access_block: u64,
    pub uses_remaining: u64,
    pub subscription_state_root: pallas::Base,
}

impl VerifyAccessCallData {
    pub fn new(
        subscriber_secret: pallas::Base,
        nonce: pallas::Base,
        permissions_claimed: u8,
        subscription_leaf_pos: u32,
        subscription_path: Vec<MerkleNode>,
        subscription_state: pallas::Base,
        subscription_spent_nullifier: pallas::Base,
        expected_capability: pallas::Base,
        subscription_id: pallas::Base,
        current_block: u64,
        subscriber_pub_x: pallas::Base,
        subscriber_pub_y: pallas::Base,
        plan_id: u32,
        lock_until_block: u64,
        uses_allowed: u64,
        rate_period: u64,
        period_uses: u64,
        last_access_block: u64,
        uses_remaining: u64,
        subscription_state_root: pallas::Base,
    ) -> Self {
        Self {
            subscriber_secret,
            nonce,
            permissions_claimed,
            subscription_leaf_pos,
            subscription_path,
            subscription_state,
            subscription_spent_nullifier,
            expected_capability,
            subscription_id,
            current_block,
            subscriber_pub_x,
            subscriber_pub_y,
            plan_id,
            lock_until_block,
            uses_allowed,
            rate_period,
            period_uses,
            last_access_block,
            uses_remaining,
            subscription_state_root,
        }
    }

    pub fn compute_public_inputs(&self) -> VerifyAccessPublicInputs {
        VerifyAccessPublicInputs {
            expected_capability: self.expected_capability,
            subscription_id: self.subscription_id,
            current_block: self.current_block,
            subscriber_pub_x: self.subscriber_pub_x,
            subscriber_pub_y: self.subscriber_pub_y,
            plan_id: self.plan_id,
            lock_until_block: self.lock_until_block,
            uses_allowed: self.uses_allowed,
            rate_period: self.rate_period,
            period_uses: self.period_uses,
            last_access_block: self.last_access_block,
            uses_remaining: self.uses_remaining,
            subscription_state_root: self.subscription_state_root,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Must match circuit witness order (all Base):
            // expected_capability, subscription_id, subscriber_pub_x,
            // subscriber_pub_y, plan_id, lock_until_block,
            // subscriber_secret, nonce
            Witness::Base(Value::known(self.expected_capability)),
            Witness::Base(Value::known(self.subscription_id)),
            Witness::Base(Value::known(self.subscriber_pub_x)),
            Witness::Base(Value::known(self.subscriber_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.plan_id as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.lock_until_block))),
            Witness::Base(Value::known(self.subscriber_secret)),
            Witness::Base(Value::known(self.nonce)),
        ]
    }
}

/// Create a VerifyAccess ZK proof
pub fn create_verify_access_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VerifyAccessCallData,
) -> Result<(Proof, VerifyAccessPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}