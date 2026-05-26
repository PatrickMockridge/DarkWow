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

//! AztDepositV1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, MerkleNode, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// AztDepositV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct AztDepositPublicInputs {
    pub nullifier: pallas::Base,
    pub commitment: pallas::Base,
    pub anchor: pallas::Base,
    pub value: pallas::Base,
    pub asset_id: pallas::Base,
    pub rollup_height: pallas::Base,
    pub eth_block_height: pallas::Base,
    pub confirmations: pallas::Base,
    pub rollup_tx_hash_0: pallas::Base,
    pub rollup_tx_hash_1: pallas::Base,
    pub recipient_pub_x: pallas::Base,
    pub recipient_pub_y: pallas::Base,
    pub bridge_nonce: pallas::Base,
    pub user_commitment: pallas::Base,
}

impl AztDepositPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier,
            self.commitment,
            self.anchor,
            self.value,
            self.asset_id,
            self.rollup_height,
            self.eth_block_height,
            self.confirmations,
            self.rollup_tx_hash_0,
            self.rollup_tx_hash_1,
            self.recipient_pub_x,
            self.recipient_pub_y,
            self.bridge_nonce,
            self.user_commitment,
        ]
    }
}

/// Input data for Aztec deposit proof generation
#[derive(Debug, Clone)]
pub struct AztDepositCallData {
    pub secret: pallas::Base,
    pub note_secret: pallas::Base,
    pub blinding_factor: pallas::Base,
    pub value: u64,
    pub asset_id: u64,
    pub recipient_public: PublicKey,
    pub bridge_nonce: u64,
    pub nullifier: pallas::Base,
    pub commitment: pallas::Base,
    pub anchor: pallas::Base,
    pub rollup_height: u64,
    pub eth_block_height: u64,
    pub confirmations: u64,
    pub rollup_tx_hash_0: pallas::Base,
    pub rollup_tx_hash_1: pallas::Base,
    pub leaf_pos: u64,
    pub merkle_path: Vec<MerkleNode>,
}

impl AztDepositCallData {
    pub fn new(
        secret: pallas::Base,
        note_secret: pallas::Base,
        blinding_factor: pallas::Base,
        value: u64,
        asset_id: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        nullifier: pallas::Base,
        commitment: pallas::Base,
        anchor: pallas::Base,
        rollup_height: u64,
        eth_block_height: u64,
        confirmations: u64,
        rollup_tx_hash_0: pallas::Base,
        rollup_tx_hash_1: pallas::Base,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Self {
        Self {
            secret,
            note_secret,
            blinding_factor,
            value,
            asset_id,
            recipient_public,
            bridge_nonce,
            nullifier,
            commitment,
            anchor,
            rollup_height,
            eth_block_height,
            confirmations,
            rollup_tx_hash_0,
            rollup_tx_hash_1,
            leaf_pos,
            merkle_path,
        }
    }

    /// Derive bridge address from recipient identity and nonce
    pub fn derive_bridge_address(&self) -> pallas::Base {
        let (pub_x, pub_y) = self.recipient_public.xy();
        let bridge_secret = poseidon_hash([pub_x, pub_y, pallas::Base::from(self.bridge_nonce)]);
        let bridge_pub = PublicKey::from_secret(SecretKey::from(bridge_secret));
        let (bridge_pub_x, bridge_pub_y) = bridge_pub.xy();
        poseidon_hash([bridge_pub_x, bridge_pub_y])
    }

    /// Compute commitment: H(secret, value, bridge_address)
    pub fn compute_commitment(&self) -> pallas::Base {
        let bridge_address = self.derive_bridge_address();
        poseidon_hash([self.secret, pallas::Base::from(self.value), bridge_address])
    }

    pub fn compute_public_inputs(&self) -> AztDepositPublicInputs {
        AztDepositPublicInputs {
            nullifier: self.nullifier,
            commitment: self.commitment,
            anchor: self.anchor,
            value: pallas::Base::from(self.value),
            asset_id: pallas::Base::from(self.asset_id),
            rollup_height: pallas::Base::from(self.rollup_height),
            eth_block_height: pallas::Base::from(self.eth_block_height),
            confirmations: pallas::Base::from(self.confirmations),
            rollup_tx_hash_0: self.rollup_tx_hash_0,
            rollup_tx_hash_1: self.rollup_tx_hash_1,
            recipient_pub_x: self.recipient_public.x(),
            recipient_pub_y: self.recipient_public.y(),
            bridge_nonce: pallas::Base::from(self.bridge_nonce),
            user_commitment: self.compute_commitment(),
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs
            Witness::Base(Value::known(self.nullifier)),
            Witness::Base(Value::known(self.commitment)),
            Witness::Base(Value::known(self.anchor)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Uint32(Value::known(self.asset_id as u32)),
            Witness::Base(Value::known(pallas::Base::from(self.rollup_height))),
            Witness::Base(Value::known(pallas::Base::from(self.eth_block_height))),
            Witness::Uint32(Value::known(self.confirmations as u32)),
            Witness::Base(Value::known(self.rollup_tx_hash_0)),
            Witness::Base(Value::known(self.rollup_tx_hash_1)),
            Witness::Base(Value::known(self.recipient_public.x())),
            Witness::Base(Value::known(self.recipient_public.y())),
            Witness::Base(Value::known(pallas::Base::from(self.bridge_nonce))),
            Witness::Base(Value::known(self.compute_commitment())),
            // Merkle proof
            Witness::Uint32(Value::known(self.leaf_pos as u32)),
            Witness::MerklePath(Value::known(self.merkle_path.clone().try_into().unwrap())),
            // Private inputs
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(self.note_secret)),
            Witness::Base(Value::known(self.blinding_factor)),
        ]
    }
}

/// Create an AztDeposit ZK proof
pub fn create_azt_deposit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AztDepositCallData,
) -> Result<(Proof, AztDepositPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}