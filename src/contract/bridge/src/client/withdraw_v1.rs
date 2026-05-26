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

//! WithdrawV1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{crypto::poseidon_hash, pasta::pallas};
use rand::rngs::OsRng;

/// WithdrawV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct WithdrawPublicInputs {
    /// Nullifier (constrained instance 0)
    pub nullifier: pallas::Base,
    /// Deposit leaf = poseidon_hash(secret, amount) (constrained instance 1)
    pub deposit_leaf: pallas::Base,
    /// Derived recipient hash (constrained instance 2)
    pub derived_recipient: pallas::Base,
    /// Recipient address hash on external chain (for contract params)
    pub recipient_hash: pallas::Base,
    /// Amount being withdrawn
    pub amount: pallas::Base,
    /// Bridge address this withdrawal is from
    pub bridge_address: pallas::Base,
    /// Merkle root of the deposit tree
    pub merkle_root: pallas::Base,
    /// Commitment being spent
    pub commitment: pallas::Base,
}

impl WithdrawPublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in withdraw_v1.zk:
    /// constrain_instance(computed_nullifier), constrain_instance(deposit_leaf), constrain_instance(derived_recipient)
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.nullifier, self.deposit_leaf, self.derived_recipient]
    }
}

/// Input data for Withdraw proof generation
#[derive(Debug, Clone)]
pub struct WithdrawCallData {
    /// User's secret for the deposit
    pub secret: pallas::Base,
    /// Amount being withdrawn
    pub amount: u64,
    /// Recipient address hash on external chain
    pub recipient_hash: pallas::Base,
    /// Bridge address this withdrawal is from
    pub bridge_address: pallas::Base,
    /// Merkle root of deposit tree
    pub merkle_root: pallas::Base,
    /// Merkle proof path (4 elements)
    pub merkle_proof: [pallas::Base; 4],
    /// Leaf index in Merkle tree
    pub leaf_index: u64,
}

impl WithdrawCallData {
    /// Create new call data
    pub fn new(
        secret: pallas::Base,
        amount: u64,
        recipient_hash: pallas::Base,
        bridge_address: pallas::Base,
        merkle_root: pallas::Base,
        merkle_proof: [pallas::Base; 4],
        leaf_index: u64,
    ) -> Self {
        Self {
            secret,
            amount,
            recipient_hash,
            bridge_address,
            merkle_root,
            merkle_proof,
            leaf_index,
        }
    }

    /// Compute nullifier: poseidon_hash(secret)
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([self.secret])
    }

    /// Compute commitment: H(secret, amount, bridge_address)
    pub fn compute_commitment(&self) -> pallas::Base {
        poseidon_hash([self.secret, pallas::Base::from(self.amount), self.bridge_address])
    }

    /// Compute deposit leaf: H(secret, amount)
    pub fn compute_deposit_leaf(&self) -> pallas::Base {
        poseidon_hash([self.secret, pallas::Base::from(self.amount)])
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> WithdrawPublicInputs {
        WithdrawPublicInputs {
            nullifier: self.compute_nullifier(),
            deposit_leaf: self.compute_deposit_leaf(),
            derived_recipient: poseidon_hash([self.recipient_hash]),
            recipient_hash: self.recipient_hash,
            amount: pallas::Base::from(self.amount),
            bridge_address: self.bridge_address,
            merkle_root: self.merkle_root,
            commitment: self.compute_commitment(),
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();

        vec![
            // Must match circuit witness order:
            // nullifier, recipient_hash, amount, bridge_address, merkle_root, commitment,
            // secret, merkle_proof_0..3, leaf_index
            Witness::Base(Value::known(public_inputs.nullifier)),
            Witness::Base(Value::known(public_inputs.recipient_hash)),
            Witness::Base(Value::known(public_inputs.amount)),
            Witness::Base(Value::known(public_inputs.bridge_address)),
            Witness::Base(Value::known(public_inputs.merkle_root)),
            Witness::Base(Value::known(public_inputs.commitment)),
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(self.merkle_proof[0])),
            Witness::Base(Value::known(self.merkle_proof[1])),
            Witness::Base(Value::known(self.merkle_proof[2])),
            Witness::Base(Value::known(self.merkle_proof[3])),
            Witness::Base(Value::known(pallas::Base::from(self.leaf_index))),
        ]
    }
}

/// Create a Withdraw ZK proof
pub fn create_withdraw_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &WithdrawCallData,
) -> Result<(Proof, WithdrawPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}