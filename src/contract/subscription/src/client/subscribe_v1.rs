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

//! Subscription subscribe ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, MerkleNode, MerklePath, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// SubscribeV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SubscribePublicInputs {
    pub subscription_id: pallas::Base,
    pub subscriber_pub_x: pallas::Base,
    pub subscriber_pub_y: pallas::Base,
    pub plan_id: u32,
    pub deposit: pallas::Base,
    pub token_id: pallas::Base,
    pub lock_until_block: u64,
    pub plan_merkle_root: pallas::Base,
    pub current_block: u64,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub dao_membership_note: pallas::Base,
    pub dao_escrow_merkle_root: pallas::Base,
}

impl SubscribePublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.subscription_id,
            self.subscriber_pub_x,
            self.subscriber_pub_y,
            pallas::Base::from(self.plan_id),
            self.deposit,
            self.token_id,
            pallas::Base::from(self.lock_until_block),
            self.plan_merkle_root,
            pallas::Base::from(self.current_block),
            self.value_commit_x,
            self.value_commit_y,
            self.dao_escrow_bulla,
            self.dao_membership_note,
            self.dao_escrow_merkle_root,
        ]
    }
}

/// Input data for subscribe proof generation
#[derive(Debug, Clone)]
pub struct SubscribeCallData {
    pub subscriber_secret: pallas::Base,
    pub nonce: pallas::Base,
    pub plan_merkle_proof: Vec<MerkleNode>,
    pub value_blind: pallas::Base,
    pub dao_member_pub_x: pallas::Base,
    pub dao_member_pub_y: pallas::Base,
    pub dao_membership_expiry: u64,
    pub dao_membership_value: pallas::Base,
    pub dao_leaf_pos: u32,
    pub dao_path: Vec<MerkleNode>,
    pub plan_leaf_pos: u32,
    pub plan_path: Vec<MerkleNode>,
    // Public inputs
    pub subscription_id: pallas::Base,
    pub subscriber_public: PublicKey,
    pub plan_id: u32,
    pub deposit: u64,
    pub token_id: pallas::Base,
    pub lock_until_block: u64,
    pub plan_merkle_root: pallas::Base,
    pub current_block: u64,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub dao_membership_note: pallas::Base,
    pub dao_escrow_merkle_root: pallas::Base,
}

impl SubscribeCallData {
    pub fn new(
        subscriber_secret: pallas::Base,
        nonce: pallas::Base,
        plan_merkle_proof: Vec<MerkleNode>,
        value_blind: pallas::Base,
        dao_member_pub_x: pallas::Base,
        dao_member_pub_y: pallas::Base,
        dao_membership_expiry: u64,
        dao_membership_value: pallas::Base,
        dao_leaf_pos: u32,
        dao_path: Vec<MerkleNode>,
        plan_leaf_pos: u32,
        plan_path: Vec<MerkleNode>,
        subscription_id: pallas::Base,
        subscriber_public: PublicKey,
        plan_id: u32,
        deposit: u64,
        token_id: pallas::Base,
        lock_until_block: u64,
        plan_merkle_root: pallas::Base,
        current_block: u64,
        value_commit_x: pallas::Base,
        value_commit_y: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        dao_membership_note: pallas::Base,
        dao_escrow_merkle_root: pallas::Base,
    ) -> Self {
        Self {
            subscriber_secret,
            nonce,
            plan_merkle_proof,
            value_blind,
            dao_member_pub_x,
            dao_member_pub_y,
            dao_membership_expiry,
            dao_membership_value,
            dao_leaf_pos,
            dao_path,
            plan_leaf_pos,
            plan_path,
            subscription_id,
            subscriber_public,
            plan_id,
            deposit,
            token_id,
            lock_until_block,
            plan_merkle_root,
            current_block,
            value_commit_x,
            value_commit_y,
            dao_escrow_bulla,
            dao_membership_note,
            dao_escrow_merkle_root,
        }
    }

    pub fn compute_public_inputs(&self) -> SubscribePublicInputs {
        SubscribePublicInputs {
            subscription_id: self.subscription_id,
            subscriber_pub_x: self.subscriber_public.x(),
            subscriber_pub_y: self.subscriber_public.y(),
            plan_id: self.plan_id,
            deposit: pallas::Base::from(self.deposit),
            token_id: self.token_id,
            lock_until_block: self.lock_until_block,
            plan_merkle_root: self.plan_merkle_root,
            current_block: self.current_block,
            value_commit_x: self.value_commit_x,
            value_commit_y: self.value_commit_y,
            dao_escrow_bulla: self.dao_escrow_bulla,
            dao_membership_note: self.dao_membership_note,
            dao_escrow_merkle_root: self.dao_escrow_merkle_root,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let plan_proof: Vec<pallas::Base> = self.plan_merkle_proof.iter().map(|n| n.0).collect();
        let dao_proof: Vec<pallas::Base> = self.dao_path.iter().map(|n| n.0).collect();

        vec![
            // Public inputs as witnesses (order matches circuit witness section)
            Witness::Base(Value::known(self.subscription_id)),
            Witness::Base(Value::known(self.subscriber_public.x())),
            Witness::Base(Value::known(self.subscriber_public.y())),
            Witness::Uint32(Value::known(self.plan_id)),
            Witness::Base(Value::known(pallas::Base::from(self.deposit))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Uint64(Value::known(self.lock_until_block)),
            Witness::Base(Value::known(self.plan_merkle_root)),
            Witness::Uint64(Value::known(self.current_block)),
            Witness::Base(Value::known(self.value_commit_x)),
            Witness::Base(Value::known(self.value_commit_y)),
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(self.dao_membership_note)),
            Witness::Base(Value::known(self.dao_escrow_merkle_root)),
            // Private inputs
            Witness::Base(Value::known(self.subscriber_secret)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(plan_proof[0])),
            Witness::Base(Value::known(plan_proof[1])),
            Witness::Base(Value::known(plan_proof[2])),
            Witness::Base(Value::known(self.value_blind)),
            Witness::Base(Value::known(self.dao_member_pub_x)),
            Witness::Base(Value::known(self.dao_member_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.dao_membership_expiry))),
            Witness::Base(Value::known(self.dao_membership_value)),
            Witness::Uint32(Value::known(self.dao_leaf_pos)),
            Witness::MerklePath(Value::known(
                self.dao_path.clone().try_into().unwrap(),
            )),
            Witness::Uint32(Value::known(self.plan_leaf_pos)),
            Witness::MerklePath(Value::known(
                self.plan_path.clone().try_into().unwrap(),
            )),
        ]
    }
}

/// Create a Subscribe ZK proof
pub fn create_subscribe_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SubscribeCallData,
) -> Result<(Proof, SubscribePublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}