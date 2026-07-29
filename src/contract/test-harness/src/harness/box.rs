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

//! Box Test Harness (L1)
//!
//! Provides isolated testing for the Box contract with Merkle inclusion proofs.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::PrimeField, poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;

pub struct BoxHarness {
    put_zkbin: ZkBinary,
    put_pk: ProvingKey,
    take_zkbin: ZkBinary,
    take_pk: ProvingKey,
}

impl BoxHarness {
    pub fn spawn() -> Self {
        let put_bin = include_bytes!("../../../box/proof/put.zk.bin");
        let take_bin = include_bytes!("../../../box/proof/take.zk.bin");

        let put_zkbin = ZkBinary::decode(put_bin, false).unwrap();
        let take_zkbin = ZkBinary::decode(take_bin, false).unwrap();

        let put_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&put_zkbin).unwrap(), &put_zkbin);
        let take_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&take_zkbin).unwrap(), &take_zkbin);

        let put_pk = ProvingKey::build(put_zkbin.k, &put_circuit).expect("ProvingKey::build failed");
        let take_pk = ProvingKey::build(take_zkbin.k, &take_circuit).expect("ProvingKey::build failed");

        Self { put_zkbin, put_pk, take_zkbin, take_pk }
    }

    pub fn circuits(&self) -> Vec<String> {
        vec!["Put".to_string(), "Take".to_string()]
    }

    pub fn put(&self) -> Result<BoxPutResult> {
        // Witness layout (Put circuit):
        //   0: box_id
        //   1: old_state_nonce
        //   2: new_state_nonce
        //   3: old_contents_commit
        //   4: new_contents_commit
        //   5: owner_secret
        //   6: owner_pub
        //   7: leaf_pos (Uint32)
        //   8: path (MerklePath)
        //   9: tx_commitment
        //  10: tx_nonce
        //  11: tx_binding
        //
        // Public inputs: nullifier_old, root, new_contents_commit, tx_binding, tx_nonce

        let owner_secret = pallas::Base::from(42u64);
        let owner_pub = poseidon_hash([owner_secret]);
        let box_id = pallas::Base::from(1u64);
        let old_state_nonce = pallas::Base::zero();
        let new_state_nonce = pallas::Base::from(1u64);
        let old_contents_commit = pallas::Base::zero();
        let capability_data = pallas::Base::from(100u64);
        let new_contents_commit = poseidon_hash([capability_data]);
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);

        // Nullifier for old state
        let nullifier_old = poseidon_hash([box_id, old_state_nonce]);

        // Build Merkle tree: sentinel leaf + old state leaf
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let old_leaf = poseidon_hash([box_id, old_contents_commit, old_state_nonce]);
        tree.append(MerkleNode::from_base(old_leaf));
        let leaf_pos_mark = tree.mark().unwrap();
        let path: Vec<MerkleNode> = tree.witness(leaf_pos_mark, 0).unwrap();
        let leaf_pos = u32::try_from(u64::from(leaf_pos_mark)).unwrap();
        let root = tree.root(0).unwrap();

        let witnesses = vec![
            Witness::Base(Value::known(box_id)),
            Witness::Base(Value::known(old_state_nonce)),
            Witness::Base(Value::known(new_state_nonce)),
            Witness::Base(Value::known(old_contents_commit)),
            Witness::Base(Value::known(new_contents_commit)),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub)),
            Witness::Uint32(Value::known(leaf_pos)),
            Witness::MerklePath(Value::known(path.clone().try_into().unwrap())),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        let public_inputs = vec![
            nullifier_old,
            root.inner(),
            new_contents_commit,
            tx_binding,
            tx_nonce,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.put_zkbin);
        let proof = Proof::create(&self.put_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create failed: {:?}", e)))?;

        let proof_bytes: Vec<u8> = dwow_serial::serialize(&proof);
        let params = dwow_box_contract::model::PutParams {
            box_id: dwow_box_contract::model::BoxId(box_id),
            old_state_nonce,
            new_state_nonce,
            old_contents_commit,
            new_contents_commit,
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            leaf_pos,
            merkle_path: path.iter().map(|n| n.inner()).collect::<Vec<_>>().try_into().unwrap(),
            proof: proof_bytes,
            tx_binding,
            tx_nonce,
        };
        let mut call_data = vec![0x01u8]; // Put function selector
        call_data.extend_from_slice(&params.encode());

        Ok(BoxPutResult { call_data, proof })
    }

    pub fn take(&self) -> Result<BoxTakeResult> {
        let owner_secret = pallas::Base::from(42u64);
        let owner_pub = poseidon_hash([owner_secret]);
        let box_id = pallas::Base::from(1u64);
        let state_nonce = pallas::Base::from(1u64);
        let contents_commit = pallas::Base::from(100u64);
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);

        let nullifier_val = poseidon_hash([box_id, state_nonce]);

        // Build Merkle tree: sentinel + filled state leaf
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let state_leaf = poseidon_hash([box_id, contents_commit, state_nonce]);
        tree.append(MerkleNode::from_base(state_leaf));
        let leaf_pos_mark = tree.mark().unwrap();
        let path: Vec<MerkleNode> = tree.witness(leaf_pos_mark, 0).unwrap();
        let leaf_pos = u32::try_from(u64::from(leaf_pos_mark)).unwrap();
        let root = tree.root(0).unwrap();

        let witnesses = vec![
            Witness::Base(Value::known(box_id)),
            Witness::Base(Value::known(contents_commit)),
            Witness::Base(Value::known(state_nonce)),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub)),
            Witness::Uint32(Value::known(leaf_pos)),
            Witness::MerklePath(Value::known(path.clone().try_into().unwrap())),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        let public_inputs = vec![
            nullifier_val,
            root.inner(),
            tx_binding,
            tx_nonce,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.take_zkbin);
        let proof = Proof::create(&self.take_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create failed: {:?}", e)))?;

        let proof_bytes: Vec<u8> = dwow_serial::serialize(&proof);
        let params = dwow_box_contract::model::TakeParams {
            box_id: dwow_box_contract::model::BoxId(box_id),
            contents_commit,
            state_nonce,
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            leaf_pos,
            merkle_path: path.iter().map(|n| n.inner()).collect::<Vec<_>>().try_into().unwrap(),
            proof: proof_bytes,
            tx_binding,
            tx_nonce,
        };
        let mut call_data = vec![0x02u8]; // Take function selector
        call_data.extend_from_slice(&params.encode());

        Ok(BoxTakeResult { call_data, proof })
    }
}

pub struct BoxPutResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

pub struct BoxTakeResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}
