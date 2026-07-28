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

//! Purse Test Harness
//!
//! Provides isolated testing for Purse contract (balance/deposit/withdraw).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, pedersen_commitment_u64, Nullifier, PublicKey, SecretKey, ScalarBlind},
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;

pub struct PurseHarness {
    balance_zkbin: ZkBinary, balance_pk: ProvingKey,
    deposit_zkbin: ZkBinary, deposit_pk: ProvingKey,
    withdraw_zkbin: ZkBinary, withdraw_pk: ProvingKey,
}

impl PurseHarness {
    pub fn spawn() -> Self {
        let balance_bin = include_bytes!("../../../purse/proof/balance_v1.zk.bin");
        let deposit_bin = include_bytes!("../../../purse/proof/deposit_v1.zk.bin");
        let withdraw_bin = include_bytes!("../../../purse/proof/withdraw_v1.zk.bin");

        let balance_zkbin = ZkBinary::decode(balance_bin, false).unwrap();
        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let withdraw_zkbin = ZkBinary::decode(withdraw_bin, false).unwrap();

        let balance_pk = ProvingKey::build(balance_zkbin.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&balance_zkbin).unwrap(), &balance_zkbin)).expect("pk");
        let deposit_pk = ProvingKey::build(deposit_zkbin.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&deposit_zkbin).unwrap(), &deposit_zkbin)).expect("pk");
        let withdraw_pk = ProvingKey::build(withdraw_zkbin.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&withdraw_zkbin).unwrap(), &withdraw_zkbin)).expect("pk");

        Self { balance_zkbin, balance_pk, deposit_zkbin, deposit_pk, withdraw_zkbin, withdraw_pk }
    }

    pub fn deposit(&self, amount: u64) -> Result<DepositPurseResult> {
        let owner_secret = pallas::Base::from(42u64);
        let owner_pub = poseidon_hash([owner_secret]);
        let purse_id = pallas::Base::from(1u64);
        let old_balance = 1000u64;
        let old_blind = ScalarBlind::random(&mut OsRng);
        let deposit_amount = amount;
        let deposit_blind = ScalarBlind::random(&mut OsRng);
        let new_balance = old_balance + amount;
        let new_blind = &old_blind + &deposit_blind;

        let old_commit = pedersen_commitment_u64(old_balance, old_blind.clone());
        let new_commit = pedersen_commitment_u64(new_balance, new_blind.clone());
        let old_coords = old_commit.to_affine().coordinates().unwrap();
        let new_coords = new_commit.to_affine().coordinates().unwrap();
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);

        let witnesses = vec![
            Witness::Base(Value::known(purse_id)),
            Witness::Base(Value::known(pallas::Base::from(old_balance))),
            Witness::Scalar(Value::known(old_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(deposit_amount))),
            Witness::Scalar(Value::known(deposit_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(new_balance))),
            Witness::Scalar(Value::known(new_blind.inner())),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub)),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        // Order MUST match circuit constrain_instance:
        // purse_id, old_x, old_y, new_x, tx_binding, tx_nonce, new_y
        let public_inputs = vec![
            purse_id,
            *old_coords.x(), *old_coords.y(),
            *new_coords.x(),
            tx_binding, tx_nonce,
            *new_coords.y(),
        ];

        let circuit = ZkCircuit::new(witnesses, &self.deposit_zkbin);
        let proof = Proof::create(&self.deposit_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {:?}", e)))?;

        let proof_bytes = dwow_serial::serialize(&proof);
        let params = dwow_purse_contract::model::DepositParamsV1 {
            purse_id: dwow_purse_contract::model::PurseId(purse_id),
            deposit_amount: amount,
            old_balance_commit: old_commit,
            new_balance_commit: new_commit,
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            proof: proof_bytes,
            tx_binding, tx_nonce,
        };
        let mut call_data = vec![0x01u8];
        call_data.extend_from_slice(&params.encode());
        Ok(DepositPurseResult { call_data, proof })
    }

    pub fn withdraw(&self, amount: u64) -> Result<WithdrawPurseResult> {
        let owner_secret = pallas::Base::from(42u64);
        let owner_pub = poseidon_hash([owner_secret]);
        let purse_id = pallas::Base::from(1u64);
        let old_balance = 2000u64;  // must be >= amount
        let old_blind = ScalarBlind::random(&mut OsRng);
        let withdraw_amount = amount;
        let withdraw_blind = ScalarBlind::random(&mut OsRng);
        let new_balance = old_balance - amount;
        let new_blind = ScalarBlind::random(&mut OsRng);
        let operation_nonce = pallas::Base::from(99u64);

        let old_commit = pedersen_commitment_u64(old_balance, old_blind.clone());
        let new_commit = pedersen_commitment_u64(new_balance, new_blind.clone());
        let old_coords = old_commit.to_affine().coordinates().unwrap();
        let new_coords = new_commit.to_affine().coordinates().unwrap();
        let nullifier_val = poseidon_hash([owner_secret, purse_id, operation_nonce]);
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);

        let witnesses = vec![
            Witness::Base(Value::known(purse_id)),
            Witness::Base(Value::known(pallas::Base::from(old_balance))),
            Witness::Scalar(Value::known(old_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(withdraw_amount))),
            Witness::Scalar(Value::known(withdraw_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(new_balance))),
            Witness::Scalar(Value::known(new_blind.inner())),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub)),
            Witness::Base(Value::known(operation_nonce)),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        let public_inputs = vec![
            nullifier_val,
            purse_id,
            *old_coords.x(), *old_coords.y(),
            *new_coords.x(), *new_coords.y(),
            tx_binding, tx_nonce,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.withdraw_zkbin);
        let proof = Proof::create(&self.withdraw_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {:?}", e)))?;

        let proof_bytes = dwow_serial::serialize(&proof);
        let params = dwow_purse_contract::model::WithdrawParamsV1 {
            purse_id: dwow_purse_contract::model::PurseId(purse_id),
            withdraw_amount: amount,
            old_balance_commit: old_commit,
            new_balance_commit: new_commit,
            nullifier: Nullifier::from_bytes(nullifier_val.to_repr()).unwrap(),
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            proof: proof_bytes,
            tx_binding, tx_nonce,
        };
        let mut call_data = vec![0x02u8];
        call_data.extend_from_slice(&params.encode());
        Ok(WithdrawPurseResult { call_data, proof })
    }

    pub fn balance(&self) -> Result<BalancePurseResult> {
        let owner_secret = pallas::Base::from(42u64);
        let owner_pub = poseidon_hash([owner_secret]);
        let purse_id_val = pallas::Base::from(1u64);
        let token_id = pallas::Base::from(10u64);
        let balance = 1000u64;
        let balance_blind = ScalarBlind::random(&mut OsRng);
        let token_blind = pallas::Base::from(50u64);

        let balance_commit = pedersen_commitment_u64(balance, balance_blind.clone());
        let bal_coords = balance_commit.to_affine().coordinates().unwrap();
        let token_commit = poseidon_hash([token_id, token_blind]);
        let derived_purse_id = poseidon_hash([owner_pub, token_id, purse_id_val]);
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);

        let witnesses = vec![
            Witness::Base(Value::known(purse_id_val)),
            Witness::Base(Value::known(token_id)),
            Witness::Base(Value::known(pallas::Base::from(balance))),
            Witness::Scalar(Value::known(balance_blind.inner())),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub)),
            Witness::Base(Value::known(token_blind)),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        let public_inputs = vec![
            derived_purse_id,
            *bal_coords.x(), *bal_coords.y(),
            token_commit,
            tx_binding, tx_nonce,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.balance_zkbin);
        let proof = Proof::create(&self.balance_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {:?}", e)))?;

        let proof_bytes = dwow_serial::serialize(&proof);
        let params = dwow_purse_contract::model::BalanceParamsV1 {
            purse_id: dwow_purse_contract::model::PurseId(purse_id_val),
            token_id,
            balance_commit,
            token_commit,
            owner: PublicKey::from_secret(SecretKey::from_base(owner_secret)),
            proof: proof_bytes,
            tx_binding, tx_nonce,
        };
        let mut call_data = vec![0x03u8];
        call_data.extend_from_slice(&params.encode());
        Ok(BalancePurseResult { call_data, proof })
    }
}

impl super::ContractHarness for PurseHarness {
    fn name(&self) -> &str { "purse" }
    fn circuits(&self) -> Vec<&'static str> { vec!["BalanceV1", "DepositV1", "WithdrawV1"] }
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns { "BalanceV1" => Some(&self.balance_zkbin), "DepositV1" => Some(&self.deposit_zkbin), "WithdrawV1" => Some(&self.withdraw_zkbin), _ => None }
    }
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns { "BalanceV1" => Some(&self.balance_pk), "DepositV1" => Some(&self.deposit_pk), "WithdrawV1" => Some(&self.withdraw_pk), _ => None }
    }
}

pub struct DepositPurseResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct WithdrawPurseResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct BalancePurseResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
