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

//! XmrDepositV1 ZK proof generation

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

/// XmrDepositV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct XmrDepositPublicInputs {
    pub tx_hash: pallas::Base,
    pub block_height: pallas::Base,
    pub output_index: pallas::Base,
    pub amount: pallas::Base,
    pub ephemeral_pub_x: pallas::Base,
    pub ephemeral_pub_y: pallas::Base,
    pub confirmations: pallas::Base,
    pub merkle_root_input: pallas::Base,
    pub commitment: pallas::Base,
    pub recipient_pub_x: pallas::Base,
    pub recipient_pub_y: pallas::Base,
    pub bridge_nonce: pallas::Base,
    pub dleq_challenge: pallas::Base,
    pub dleq_response_1: pallas::Base,
    pub dleq_response_2: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl XmrDepositPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.tx_hash,
            self.block_height,
            self.output_index,
            self.amount,
            self.ephemeral_pub_x,
            self.ephemeral_pub_y,
            self.confirmations,
            self.merkle_root_input,
            self.commitment,
            self.recipient_pub_x,
            self.recipient_pub_y,
            self.bridge_nonce,
            self.dleq_challenge,
            self.dleq_response_1,
            self.dleq_response_2,
            self.tx_commitment,
        ]
    }
}

/// Input data for Monero deposit proof generation
#[derive(Debug, Clone)]
pub struct XmrDepositCallData {
    pub secret: pallas::Base,
    pub one_time_addr_secret: pallas::Base,
    pub amount: u64,
    pub recipient_public: PublicKey,
    pub bridge_nonce: u64,
    pub tx_hash: pallas::Base,
    pub block_height: u64,
    pub output_index: u64,
    pub ephemeral_pub_x: pallas::Base,
    pub ephemeral_pub_y: pallas::Base,
    pub confirmations: u64,
    pub merkle_root: pallas::Base,
    pub dleq_challenge: pallas::Base,
    pub dleq_response_1: pallas::Base,
    pub dleq_response_2: pallas::Base,
    pub leaf_pos: u64,
    pub merkle_path: Vec<MerkleNode>,
    pub tx_commitment: pallas::Base,
}

impl XmrDepositCallData {
    pub fn new(
        secret: pallas::Base,
        one_time_addr_secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        tx_hash: pallas::Base,
        block_height: u64,
        output_index: u64,
        ephemeral_pub_x: pallas::Base,
        ephemeral_pub_y: pallas::Base,
        confirmations: u64,
        merkle_root: pallas::Base,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Self {
        Self {
            secret,
            one_time_addr_secret,
            amount,
            recipient_public,
            bridge_nonce,
            tx_hash,
            block_height,
            output_index,
            ephemeral_pub_x,
            ephemeral_pub_y,
            confirmations,
            merkle_root,
            dleq_challenge: pallas::Base::zero(),
            dleq_response_1: pallas::Base::zero(),
            dleq_response_2: pallas::Base::zero(),
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

    pub fn compute_public_inputs(&self) -> XmrDepositPublicInputs {
        XmrDepositPublicInputs {
            tx_hash: self.tx_hash,
            block_height: pallas::Base::from(self.block_height),
            output_index: pallas::Base::from(self.output_index),
            amount: pallas::Base::from(self.amount),
            ephemeral_pub_x: self.ephemeral_pub_x,
            ephemeral_pub_y: self.ephemeral_pub_y,
            confirmations: pallas::Base::from(self.confirmations),
            merkle_root_input: self.merkle_root,
            commitment: self.compute_commitment(),
            recipient_pub_x: self.recipient_public.x(),
            recipient_pub_y: self.recipient_public.y(),
            bridge_nonce: pallas::Base::from(self.bridge_nonce),
            dleq_challenge: self.dleq_challenge,
            dleq_response_1: self.dleq_response_1,
            dleq_response_2: self.dleq_response_2,
            tx_commitment: self.tx_commitment,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs
            Witness::Base(Value::known(self.tx_hash)),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Uint32(Value::known(self.output_index as u32)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(self.ephemeral_pub_x)),
            Witness::Base(Value::known(self.ephemeral_pub_y)),
            Witness::Uint32(Value::known(self.confirmations as u32)),
            Witness::Base(Value::known(self.merkle_root)),
            Witness::Base(Value::known(self.compute_commitment())),
            Witness::Base(Value::known(self.recipient_public.x())),
            Witness::Base(Value::known(self.recipient_public.y())),
            Witness::Base(Value::known(pallas::Base::from(self.bridge_nonce))),
            Witness::Base(Value::known(self.dleq_challenge)),
            Witness::Base(Value::known(self.dleq_response_1)),
            Witness::Base(Value::known(self.dleq_response_2)),
            // Merkle proof
            Witness::Uint32(Value::known(self.leaf_pos as u32)),
            Witness::MerklePath(Value::known(self.merkle_path.clone().try_into().unwrap())),
            // Private inputs
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(self.one_time_addr_secret)),
        ]
    }
}

/// Create an XmrDeposit ZK proof
pub fn create_xmr_deposit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &XmrDepositCallData,
) -> Result<(Proof, XmrDepositPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}