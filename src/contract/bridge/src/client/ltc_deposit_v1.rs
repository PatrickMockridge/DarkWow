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

//! LtcDepositV1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, MerkleNode, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// LtcDepositV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct LtcDepositPublicInputs {
    pub tx_hash_0: pallas::Base,
    pub tx_hash_1: pallas::Base,
    pub output_index: pallas::Base,
    pub amount: pallas::Base,
    pub block_merkle_root: pallas::Base,
    pub block_height: pallas::Base,
    pub confirmations: pallas::Base,
    pub recipient_pub_x: pallas::Base,
    pub recipient_pub_y: pallas::Base,
    pub bridge_nonce: pallas::Base,
    pub user_commitment: pallas::Base,
    pub confidential_commitment: pallas::Base,
}

impl LtcDepositPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.tx_hash_0,
            self.tx_hash_1,
            self.output_index,
            self.amount,
            self.block_merkle_root,
            self.block_height,
            self.confirmations,
            self.recipient_pub_x,
            self.recipient_pub_y,
            self.bridge_nonce,
            self.user_commitment,
            self.confidential_commitment,
        ]
    }
}

/// Input data for Litecoin deposit proof generation
#[derive(Debug, Clone)]
pub struct LtcDepositCallData {
    pub secret: pallas::Base,
    pub amount: u64,
    pub recipient_public: PublicKey,
    pub bridge_nonce: u64,
    pub tx_hash_0: pallas::Base,
    pub tx_hash_1: pallas::Base,
    pub output_index: u64,
    pub block_merkle_root: pallas::Base,
    pub block_height: u64,
    pub confirmations: u64,
    pub confidential_commitment: pallas::Base,
    pub blinding_factor: pallas::Base,
    pub range_proof_bytes: pallas::Base,
    pub leaf_pos: u64,
    pub merkle_path: Vec<MerkleNode>,
}

impl LtcDepositCallData {
    pub fn new(
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        tx_hash_0: pallas::Base,
        tx_hash_1: pallas::Base,
        output_index: u64,
        block_merkle_root: pallas::Base,
        block_height: u64,
        confirmations: u64,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Self {
        Self {
            secret,
            amount,
            recipient_public,
            bridge_nonce,
            tx_hash_0,
            tx_hash_1,
            output_index,
            block_merkle_root,
            block_height,
            confirmations,
            confidential_commitment: pallas::Base::zero(),
            blinding_factor: pallas::Base::zero(),
            range_proof_bytes: pallas::Base::zero(),
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

    /// Compute commitment: H(secret, amount, bridge_address)
    pub fn compute_commitment(&self) -> pallas::Base {
        let bridge_address = self.derive_bridge_address();
        poseidon_hash([self.secret, pallas::Base::from(self.amount), bridge_address])
    }

    pub fn compute_public_inputs(&self) -> LtcDepositPublicInputs {
        LtcDepositPublicInputs {
            tx_hash_0: self.tx_hash_0,
            tx_hash_1: self.tx_hash_1,
            output_index: pallas::Base::from(self.output_index),
            amount: pallas::Base::from(self.amount),
            block_merkle_root: self.block_merkle_root,
            block_height: pallas::Base::from(self.block_height),
            confirmations: pallas::Base::from(self.confirmations),
            recipient_pub_x: self.recipient_public.x(),
            recipient_pub_y: self.recipient_public.y(),
            bridge_nonce: pallas::Base::from(self.bridge_nonce),
            user_commitment: self.compute_commitment(),
            confidential_commitment: self.confidential_commitment,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs
            Witness::Base(Value::known(self.tx_hash_0)),
            Witness::Base(Value::known(self.tx_hash_1)),
            Witness::Uint32(Value::known(self.output_index as u32)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(self.block_merkle_root)),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Uint32(Value::known(self.confirmations as u32)),
            Witness::Base(Value::known(self.recipient_public.x())),
            Witness::Base(Value::known(self.recipient_public.y())),
            Witness::Base(Value::known(pallas::Base::from(self.bridge_nonce))),
            Witness::Base(Value::known(self.compute_commitment())),
            Witness::Base(Value::known(self.confidential_commitment)),
            // Merkle proof
            Witness::Uint32(Value::known(self.leaf_pos as u32)),
            Witness::MerklePath(Value::known(self.merkle_path.clone().try_into().unwrap())),
            // Private inputs
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(self.blinding_factor)),
            Witness::Base(Value::known(self.range_proof_bytes)),
        ]
    }
}

/// Create an LtcDeposit ZK proof
pub fn create_ltc_deposit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &LtcDepositCallData,
) -> Result<(Proof, LtcDepositPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}