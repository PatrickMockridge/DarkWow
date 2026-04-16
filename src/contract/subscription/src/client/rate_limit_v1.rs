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

//! Subscription rate_limit ZK proof generation

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

/// RateLimitV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RateLimitPublicInputs {
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
    pub subscription_state_root: pallas::Base,
}

impl RateLimitPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.expected_capability,
            self.subscription_id,
            pallas::Base::from(self.current_block),
            self.subscriber_pub_x,
            self.subscriber_pub_y,
            pallas::Base::from(self.plan_id as u64),
            pallas::Base::from(self.lock_until_block),
            pallas::Base::from(self.uses_allowed),
            pallas::Base::from(self.rate_period),
            pallas::Base::from(self.period_uses),
            pallas::Base::from(self.last_access_block),
            self.subscription_state_root,
        ]
    }
}

/// Input data for rate_limit proof generation
#[derive(Debug, Clone)]
pub struct RateLimitCallData {
    pub subscriber_secret: pallas::Base,
    pub nonce: pallas::Base,
    pub permissions_claimed: u8,
    pub subscription_leaf_pos: u32,
    pub subscription_path: Vec<MerkleNode>,
    pub subscription_state: pallas::Base,
    pub subscription_spent_nullifier: pallas::Base,
    pub uses_remaining: u64,
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
    pub subscription_state_root: pallas::Base,
}

impl RateLimitCallData {
    pub fn new(
        subscriber_secret: pallas::Base,
        nonce: pallas::Base,
        permissions_claimed: u8,
        subscription_leaf_pos: u32,
        subscription_path: Vec<MerkleNode>,
        subscription_state: pallas::Base,
        subscription_spent_nullifier: pallas::Base,
        uses_remaining: u64,
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
            uses_remaining,
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
            subscription_state_root,
        }
    }

    pub fn compute_public_inputs(&self) -> RateLimitPublicInputs {
        RateLimitPublicInputs {
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
            subscription_state_root: self.subscription_state_root,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.expected_capability)),
            Witness::Base(Value::known(self.subscription_id)),
            Witness::Uint64(Value::known(self.current_block)),
            Witness::Base(Value::known(self.subscriber_pub_x)),
            Witness::Base(Value::known(self.subscriber_pub_y)),
            Witness::Uint32(Value::known(self.plan_id)),
            Witness::Uint64(Value::known(self.lock_until_block)),
            Witness::Uint64(Value::known(self.uses_allowed)),
            Witness::Uint64(Value::known(self.rate_period)),
            Witness::Uint64(Value::known(self.period_uses)),
            Witness::Uint64(Value::known(self.last_access_block)),
            Witness::Base(Value::known(self.subscription_state_root)),
            // Private inputs
            Witness::Base(Value::known(self.subscriber_secret)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Uint32(Value::known(self.permissions_claimed as u32)),
            Witness::Uint32(Value::known(self.subscription_leaf_pos)),
            Witness::MerklePath(Value::known(
                self.subscription_path.clone().try_into().unwrap(),
            )),
            Witness::Base(Value::known(self.subscription_state)),
            Witness::Base(Value::known(self.subscription_spent_nullifier)),
            Witness::Uint64(Value::known(self.uses_remaining)),
        ]
    }
}

/// Create a RateLimit ZK proof
pub fn create_rate_limit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RateLimitCallData,
) -> Result<(Proof, RateLimitPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}