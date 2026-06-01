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

//! DepositV1 ZK proof generation

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

/// DepositV1 circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct DepositPublicInputs {
    /// Commitment the user claims this deposit creates
    pub commitment: pallas::Base,
    /// Recipient's DarkWow public key X coordinate
    pub recipient_pub_x: pallas::Base,
    /// Recipient's DarkWow public key Y coordinate
    pub recipient_pub_y: pallas::Base,
    /// Fresh nonce for this deposit (unlinkability)
    pub bridge_nonce: pallas::Base,
    /// Hash of external chain block containing deposit
    pub external_block_hash: pallas::Base,
    /// Merkle root of external chain's deposit tree
    pub merkle_root_input: pallas::Base,
}

impl DepositPublicInputs {
    /// Convert to vector for ZK proof creation (instance column values).
    /// Must match the circuit's `constrain_instance` calls: exactly 1.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.commitment,
        ]
    }
}

/// Input data for Deposit proof generation
#[derive(Debug, Clone)]
pub struct DepositCallData {
    /// User's secret for this deposit
    pub secret: pallas::Base,
    /// Deposit amount in external chain unit
    pub amount: u64,
    /// Recipient's public key on DarkWow
    pub recipient_public: PublicKey,
    /// Fresh nonce for temporal privacy
    pub bridge_nonce: u64,
    /// External block hash containing deposit
    pub external_block_hash: pallas::Base,
    /// Merkle root of deposit tree
    pub merkle_root: pallas::Base,
    /// Merkle proof leaf position
    pub leaf_pos: u64,
    /// Merkle proof path
    pub merkle_path: Vec<MerkleNode>,
}

impl DepositCallData {
    /// Create new call data
    pub fn new(
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        external_block_hash: pallas::Base,
        merkle_root: pallas::Base,
        leaf_pos: u64,
        merkle_path: Vec<MerkleNode>,
    ) -> Self {
        Self {
            secret,
            amount,
            recipient_public,
            bridge_nonce,
            external_block_hash,
            merkle_root,
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

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> DepositPublicInputs {
        let commitment = self.compute_commitment();
        let (recipient_pub_x, recipient_pub_y) = self.recipient_public.xy();

        DepositPublicInputs {
            commitment,
            recipient_pub_x,
            recipient_pub_y,
            bridge_nonce: pallas::Base::from(self.bridge_nonce),
            external_block_hash: self.external_block_hash,
            merkle_root_input: self.merkle_root,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();

        vec![
            // Public inputs
            Witness::Base(Value::known(public_inputs.commitment)),
            Witness::Base(Value::known(public_inputs.recipient_pub_x)),
            Witness::Base(Value::known(public_inputs.recipient_pub_y)),
            Witness::Base(Value::known(public_inputs.bridge_nonce)),
            Witness::Base(Value::known(public_inputs.external_block_hash)),
            Witness::Base(Value::known(public_inputs.merkle_root_input)),
            // Merkle proof
            Witness::Uint32(Value::known(self.leaf_pos as u32)),
            Witness::MerklePath(Value::known(self.merkle_path.clone().try_into().unwrap())),
            // Private inputs
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
        ]
    }
}

/// Create a Deposit ZK proof
pub fn create_deposit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &DepositCallData,
) -> Result<(Proof, DepositPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}