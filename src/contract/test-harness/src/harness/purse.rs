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

//! Purse Test Harness (L1)
//!
//! Provides isolated testing for the Purse contract with Merkle inclusion proofs
//! and Pedersen balance commitments.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use crate::harness::ContractHarness;
use dwow_sdk::{
    crypto::{
        blind::ScalarBlind,
        pasta_prelude::{CurveAffine, PrimeField},
        pedersen_commitment_u64, poseidon_hash,
        MerkleNode, MerkleTree, PublicKey, SecretKey,
    },
    pasta::{group::{Curve, GroupEncoding}, pallas},
};
use rand::rngs::OsRng;

pub struct PurseHarness {
    balance_zkbin: ZkBinary,
    balance_pk: ProvingKey,
    deposit_zkbin: ZkBinary,
    deposit_pk: ProvingKey,
    withdraw_zkbin: ZkBinary,
    withdraw_pk: ProvingKey,
}

impl PurseHarness {
    pub fn spawn() -> Self {
        let balance_bin = include_bytes!("../../../purse/proof/balance.zk.bin");
        let deposit_bin = include_bytes!("../../../purse/proof/deposit.zk.bin");
        let withdraw_bin = include_bytes!("../../../purse/proof/withdraw.zk.bin");

        let balance_zkbin = ZkBinary::decode(balance_bin, false).unwrap();
        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let withdraw_zkbin = ZkBinary::decode(withdraw_bin, false).unwrap();

        let balance_circuit = ZkCircuit::new(dwow_core::zk::empty_witnesses(&balance_zkbin).unwrap(), &balance_zkbin);
        let deposit_circuit = ZkCircuit::new(dwow_core::zk::empty_witnesses(&deposit_zkbin).unwrap(), &deposit_zkbin);
        let withdraw_circuit = ZkCircuit::new(dwow_core::zk::empty_witnesses(&withdraw_zkbin).unwrap(), &withdraw_zkbin);

        let balance_pk = ProvingKey::build(balance_zkbin.k, &balance_circuit).expect("ProvingKey::build failed");
        let deposit_pk = ProvingKey::build(deposit_zkbin.k, &deposit_circuit).expect("ProvingKey::build failed");
        let withdraw_pk = ProvingKey::build(withdraw_zkbin.k, &withdraw_circuit).expect("ProvingKey::build failed");

        Self { balance_zkbin, balance_pk, deposit_zkbin, deposit_pk, withdraw_zkbin, withdraw_pk }
    }

    pub fn circuits(&self) -> Vec<&'static str> {
        vec!["Balance", "Deposit", "Withdraw"]
    }

    pub fn deposit(&self, amount: u64) -> Result<PurseDepositResult> {
        let owner_secret = pallas::Base::from(42u64);
        // DOMAIN_SIGNATURE_SECRET = 7
        let owner_pub = poseidon_hash([pallas::Base::from(7), owner_secret]);
        let purse_id = pallas::Base::from(1u64);
        let state_nonce = pallas::Base::zero();
        let old_balance: u64 = 0;
        let new_balance: u64 = amount;
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        // DOMAIN_TX_BINDING = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3), tx_commitment, tx_nonce]);

        let nullifier_old = poseidon_hash([purse_id, state_nonce]);

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let old_leaf = poseidon_hash([purse_id, pallas::Base::from(old_balance), state_nonce]);
        tree.append(MerkleNode::from_base(old_leaf));
        let leaf_pos_mark = tree.mark().unwrap();
        let path: Vec<MerkleNode> = tree.witness(leaf_pos_mark, 0).unwrap();
        let leaf_pos = u32::try_from(u64::from(leaf_pos_mark)).unwrap();

        let old_blind = ScalarBlind::from(1u64);
        let dep_blind = ScalarBlind::from(2u64);
        let new_blind = ScalarBlind::from(3u64);
        let old_commit = pedersen_commitment_u64(old_balance, old_blind.clone());
        let new_commit = pedersen_commitment_u64(new_balance, new_blind.clone());
        let old_coords = old_commit.to_affine().coordinates().unwrap();
        let new_coords = new_commit.to_affine().coordinates().unwrap();

        let witnesses = vec![
            Witness::Base(Value::known(purse_id)),
            Witness::Base(Value::known(pallas::Base::from(old_balance))),
            Witness::Scalar(Value::known(old_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(amount))),
            Witness::Scalar(Value::known(dep_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(new_balance))),
            Witness::Scalar(Value::known(new_blind.inner())),
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
            nullifier_old,
            pallas::Base::zero(),
            *old_coords.x(), *old_coords.y(),
            *new_coords.x(),
            tx_binding, tx_nonce,
            *new_coords.y(),
        ];

        let circuit = ZkCircuit::new(witnesses, &self.deposit_zkbin);
        let proof = Proof::create(&self.deposit_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create failed: {:?}", e)))?;

        let proof_bytes: Vec<u8> = dwow_serial::serialize(&proof);
        let params = dwow_purse_contract::model::DepositParams {
            purse_id: dwow_purse_contract::model::PurseId(purse_id),
            old_balance,
            deposit_amount: amount,
            new_balance,
            state_nonce,
            leaf_pos,
            merkle_path: path.iter().map(|n| n.inner()).collect::<Vec<_>>().try_into().unwrap(),
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            proof: proof_bytes,
            tx_binding,
            tx_nonce,
        };
        let mut call_data = vec![0x01u8];
        call_data.extend_from_slice(&params.encode());

        Ok(PurseDepositResult { call_data, proof })
    }

    pub fn withdraw(&self, amount: u64) -> Result<PurseWithdrawResult> {
        let owner_secret = pallas::Base::from(42u64);
        // DOMAIN_SIGNATURE_SECRET = 7
        let owner_pub = poseidon_hash([pallas::Base::from(7), owner_secret]);
        let purse_id = pallas::Base::from(1u64);
        let state_nonce = pallas::Base::from(1u64);
        let old_balance: u64 = 100;
        let new_balance: u64 = old_balance - amount;
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        // DOMAIN_TX_BINDING = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3), tx_commitment, tx_nonce]);

        let nullifier_val = poseidon_hash([purse_id, state_nonce]);

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let old_leaf = poseidon_hash([purse_id, pallas::Base::from(old_balance), state_nonce]);
        tree.append(MerkleNode::from_base(old_leaf));
        let leaf_pos_mark = tree.mark().unwrap();
        let path: Vec<MerkleNode> = tree.witness(leaf_pos_mark, 0).unwrap();
        let leaf_pos = u32::try_from(u64::from(leaf_pos_mark)).unwrap();

        let old_blind = ScalarBlind::from(1u64);
        let new_blind = ScalarBlind::from(3u64);
        let old_commit = pedersen_commitment_u64(old_balance, old_blind.clone());
        let new_commit = pedersen_commitment_u64(new_balance, new_blind.clone());
        let old_coords = old_commit.to_affine().coordinates().unwrap();
        let new_coords = new_commit.to_affine().coordinates().unwrap();

        let witnesses = vec![
            Witness::Base(Value::known(purse_id)),
            Witness::Base(Value::known(pallas::Base::from(old_balance))),
            Witness::Scalar(Value::known(old_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(amount))),
            Witness::Scalar(Value::known(ScalarBlind::from(2u64).inner())),
            Witness::Base(Value::known(pallas::Base::from(new_balance))),
            Witness::Scalar(Value::known(new_blind.inner())),
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
            pallas::Base::zero(),
            *old_coords.x(), *old_coords.y(),
            *new_coords.x(), *new_coords.y(),
            tx_binding, tx_nonce,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.withdraw_zkbin);
        let proof = Proof::create(&self.withdraw_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create failed: {:?}", e)))?;

        let proof_bytes: Vec<u8> = dwow_serial::serialize(&proof);
        let params = dwow_purse_contract::model::WithdrawParams {
            purse_id: dwow_purse_contract::model::PurseId(purse_id),
            old_balance,
            withdraw_amount: amount,
            new_balance,
            state_nonce,
            leaf_pos,
            merkle_path: path.iter().map(|n| n.inner()).collect::<Vec<_>>().try_into().unwrap(),
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            proof: proof_bytes,
            tx_binding,
            tx_nonce,
        };
        let mut call_data = vec![0x02u8];
        call_data.extend_from_slice(&params.encode());

        Ok(PurseWithdrawResult { call_data, proof })
    }

    pub fn balance(&self) -> Result<PurseBalanceResult> {
        let owner_secret = pallas::Base::from(42u64);
        // DOMAIN_SIGNATURE_SECRET = 7
        let owner_pub = poseidon_hash([pallas::Base::from(7), owner_secret]);
        let purse_id = pallas::Base::from(1u64);
        let token_id = pallas::Base::from(1u64);
        let balance: u64 = 100;
        let state_nonce = pallas::Base::from(1u64);
        let token_blind = pallas::Base::from(5u64);
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        // DOMAIN_TX_BINDING = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3), tx_commitment, tx_nonce]);

        let balance_blind = ScalarBlind::from(1u64);
        let balance_commit = pedersen_commitment_u64(balance, balance_blind.clone());
        let coords = balance_commit.to_affine().coordinates().unwrap();
        let token_commit = poseidon_hash([token_id, token_blind]);
        let derived_purse_id = poseidon_hash([purse_id, token_id]);

        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let state_leaf = poseidon_hash([purse_id, pallas::Base::from(balance), state_nonce]);
        tree.append(MerkleNode::from_base(state_leaf));
        let leaf_pos_mark = tree.mark().unwrap();
        let path: Vec<MerkleNode> = tree.witness(leaf_pos_mark, 0).unwrap();
        let leaf_pos = u32::try_from(u64::from(leaf_pos_mark)).unwrap();

        let witnesses = vec![
            Witness::Base(Value::known(purse_id)),
            Witness::Base(Value::known(token_id)),
            Witness::Base(Value::known(pallas::Base::from(balance))),
            Witness::Scalar(Value::known(balance_blind.inner())),
            Witness::Base(Value::known(state_nonce)),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub)),
            Witness::Base(Value::known(token_blind)),
            Witness::Uint32(Value::known(leaf_pos)),
            Witness::MerklePath(Value::known(path.clone().try_into().unwrap())),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        let public_inputs = vec![
            derived_purse_id,
            pallas::Base::zero(),
            *coords.x(), *coords.y(),
            token_commit,
            tx_binding, tx_nonce,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.balance_zkbin);
        let proof = Proof::create(&self.balance_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create failed: {:?}", e)))?;

        let proof_bytes: Vec<u8> = dwow_serial::serialize(&proof);
        let params = dwow_purse_contract::model::BalanceParams {
            purse_id: dwow_purse_contract::model::PurseId(purse_id),
            token_id,
            balance,
            state_nonce,
            leaf_pos,
            merkle_path: path.iter().map(|n| n.inner()).collect::<Vec<_>>().try_into().unwrap(),
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            proof: proof_bytes,
            tx_binding,
            tx_nonce,
        };
        let mut call_data = vec![0x03u8];
        call_data.extend_from_slice(&params.encode());

        Ok(PurseBalanceResult { call_data, proof })
    }
}

impl ContractHarness for PurseHarness {
    fn name(&self) -> &str { "purse" }
    fn circuits(&self) -> Vec<&'static str> { self.circuits() }
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "Balance" => Some(&self.balance_zkbin),
            "Deposit" => Some(&self.deposit_zkbin),
            "Withdraw" => Some(&self.withdraw_zkbin),
            _ => None,
        }
    }
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "Balance" => Some(&self.balance_pk),
            "Deposit" => Some(&self.deposit_pk),
            "Withdraw" => Some(&self.withdraw_pk),
            _ => None,
        }
    }
}

pub struct PurseDepositResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

pub struct PurseWithdrawResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}

pub struct PurseBalanceResult {
    pub call_data: Vec<u8>,
    pub proof: Proof,
}
