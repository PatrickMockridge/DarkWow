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

//! ZecDepositV1 ZK proof generation

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

/// ZecDepositV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ZecDepositPublicInputs {
    pub nullifier: pallas::Base,
    pub commitment: pallas::Base,
    pub anchor: pallas::Base,
    pub block_height: pallas::Base,
    pub amount: pallas::Base,
    pub randomized_pub_key_x: pallas::Base,
    pub randomized_pub_key_y: pallas::Base,
    pub randomness: pallas::Base,
    pub confirmations: pallas::Base,
    pub recipient_pub_x: pallas::Base,
    pub recipient_pub_y: pallas::Base,
    pub bridge_nonce: pallas::Base,
    pub user_commitment: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ZecDepositPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier,
            self.commitment,
            self.anchor,
            self.block_height,
            self.amount,
            self.randomized_pub_key_x,
            self.randomized_pub_key_y,
            self.randomness,
            self.confirmations,
            self.recipient_pub_x,
            self.recipient_pub_y,
            self.bridge_nonce,
            self.user_commitment,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for Zcash deposit proof generation
#[derive(Debug, Clone)]
pub struct ZecDepositCallData {
    pub secret: pallas::Base,
    pub position: pallas::Base,
    pub note_encryption: pallas::Base,
    pub spend_proof_bytes: pallas::Base,
    pub output_proof_bytes: pallas::Base,
    pub amount: u64,
    pub recipient_public: PublicKey,
    pub bridge_nonce: u64,
    pub nullifier: pallas::Base,
    pub commitment: pallas::Base,
    pub anchor: pallas::Base,
    pub block_height: u64,
    pub randomized_pub_key_x: pallas::Base,
    pub randomized_pub_key_y: pallas::Base,
    pub randomness: pallas::Base,
    pub confirmations: u64,
    pub leaf_pos: u64,
    pub merkle_path: Vec<MerkleNode>,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ZecDepositCallData {
    pub fn new(
        secret: pallas::Base,
        position: pallas::Base,
        note_encryption: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        nullifier: pallas::Base,
        commitment: pallas::Base,
        anchor: pallas::Base,
        block_height: u64,
        randomized_pub_key_x: pallas::Base,
        randomized_pub_key_y: pallas::Base,
        randomness: pallas::Base,
        confirmations: u64,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Self {
        Self {
            secret,
            position,
            note_encryption,
            spend_proof_bytes: pallas::Base::zero(),
            output_proof_bytes: pallas::Base::zero(),
            amount,
            recipient_public,
            bridge_nonce,
            nullifier,
            commitment,
            anchor,
            block_height,
            randomized_pub_key_x,
            randomized_pub_key_y,
            randomness,
            confirmations,
            leaf_pos,
            merkle_path,
            tx_commitment: pallas::Base::zero(),
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

    /// Compute commitment: H(secret, amount, bridge_address)
    pub fn compute_commitment(&self) -> pallas::Base {
        let bridge_address = self.derive_bridge_address();
        poseidon_hash([self.secret, pallas::Base::from(self.amount), bridge_address])
    }

    pub fn compute_public_inputs(&self) -> ZecDepositPublicInputs {
        ZecDepositPublicInputs {
            nullifier: self.nullifier,
            commitment: self.commitment,
            anchor: self.anchor,
            block_height: pallas::Base::from(self.block_height),
            amount: pallas::Base::from(self.amount),
            randomized_pub_key_x: self.randomized_pub_key_x,
            randomized_pub_key_y: self.randomized_pub_key_y,
            randomness: self.randomness,
            confirmations: pallas::Base::from(self.confirmations),
            recipient_pub_x: self.recipient_public.x(),
            recipient_pub_y: self.recipient_public.y(),
            bridge_nonce: pallas::Base::from(self.bridge_nonce),
            user_commitment: self.compute_commitment(),
            tx_binding: poseidon_hash([self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs
            Witness::Base(Value::known(self.nullifier)),
            Witness::Base(Value::known(self.commitment)),
            Witness::Base(Value::known(self.anchor)),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(self.randomized_pub_key_x)),
            Witness::Base(Value::known(self.randomized_pub_key_y)),
            Witness::Base(Value::known(self.randomness)),
            Witness::Uint32(Value::known(self.confirmations as u32)),
            Witness::Base(Value::known(self.recipient_public.x())),
            Witness::Base(Value::known(self.recipient_public.y())),
            Witness::Base(Value::known(pallas::Base::from(self.bridge_nonce))),
            Witness::Base(Value::known(self.compute_commitment())),
            // Merkle proof
            Witness::Uint32(Value::known(self.leaf_pos as u32)),
            Witness::MerklePath(Value::known(self.merkle_path.clone().try_into().unwrap())),
            // Private inputs
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(self.position)),
            Witness::Base(Value::known(self.note_encryption)),
            Witness::Base(Value::known(self.spend_proof_bytes)),
            Witness::Base(Value::known(self.output_proof_bytes)),
        ]
    }
}

/// Create a ZecDeposit ZK proof
pub fn create_zec_deposit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ZecDepositCallData,
) -> Result<(Proof, ZecDepositPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}