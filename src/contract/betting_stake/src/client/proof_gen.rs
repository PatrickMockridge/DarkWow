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

//! BettingStake ZK Proof Generation — all circuits with ZK identity proofs.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

// ============================================================================
// InitV1
// ============================================================================

#[derive(Debug, Clone)]
pub struct InitV1PublicInputs {
    pub table_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl InitV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.table_id, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct InitV1CallData {
    pub betting_contract_id: pallas::Base,
    pub house_edge_bp: u32,
    pub risk_profile: u8,
    pub nonce: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl InitV1CallData {
    pub fn new(betting_contract_id: pallas::Base, house_edge_bp: u32, risk_profile: u8, nonce: u64) -> Self {
        Self { betting_contract_id, house_edge_bp, risk_profile, nonce, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }
    pub fn compute_public_inputs(&self) -> InitV1PublicInputs {
        let table_id = poseidon_hash([pallas::Base::from(4), self.betting_contract_id, pallas::Base::from(self.nonce)]);
        InitV1PublicInputs { table_id, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.betting_contract_id)),
            Witness::Base(Value::known(pallas::Base::from(self.house_edge_bp as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.risk_profile as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

pub fn init_v1_proof(zkbin: &ZkBinary, pk: &ProvingKey, input: &InitV1CallData) -> Result<(Proof, InitV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();
    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;
    Ok((proof, public_inputs))
}

// ============================================================================
// StakeV1
// ============================================================================

#[derive(Debug, Clone)]
pub struct StakeV1PublicInputs {
    pub stake_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub staker_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl StakeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.stake_id, self.value_commit_x, self.value_commit_y, self.staker_nullifier, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct StakeV1CallData {
    pub table_id: pallas::Base,
    pub staker_secret: pallas::Base,
    pub staker_pub_x: pallas::Base,
    pub staker_pub_y: pallas::Base,
    pub amount: u64,
    pub token_id: pallas::Base,
    pub nonce: u64,
    pub staker_nullifier: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl StakeV1CallData {
    pub fn new(
        table_id: pallas::Base,
        staker_pub: PublicKey,
        staker_secret: pallas::Base,
        amount: u64,
        token_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        let (sx, sy) = staker_pub.xy().expect("pk not identity");
        let stake_id = poseidon_hash([pallas::Base::from(4), table_id, sx, sy, pallas::Base::from(amount), pallas::Base::from(nonce)]);
        let staker_nullifier = poseidon_hash([pallas::Base::from(1), stake_id, staker_secret]);
        Self { table_id, staker_secret, staker_pub_x: sx, staker_pub_y: sy, amount, token_id, nonce, staker_nullifier, value_blind, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }
    pub fn compute_public_inputs(&self) -> StakeV1PublicInputs {
        let stake_id = poseidon_hash([pallas::Base::from(4), self.table_id, self.staker_pub_x, self.staker_pub_y, pallas::Base::from(self.amount), pallas::Base::from(self.nonce)]);
        StakeV1PublicInputs { stake_id, value_commit_x: pallas::Base::zero(), value_commit_y: pallas::Base::zero(), staker_nullifier: self.staker_nullifier, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.table_id)),
            Witness::Base(Value::known(self.staker_secret)),
            Witness::Base(Value::known(self.staker_pub_x)),
            Witness::Base(Value::known(self.staker_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            Witness::Base(Value::known(self.staker_nullifier)),
            Witness::Scalar(Value::known(self.value_blind)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

pub fn stake_v1_proof(zkbin: &ZkBinary, pk: &ProvingKey, input: &StakeV1CallData) -> Result<(Proof, StakeV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();
    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;
    Ok((proof, public_inputs))
}

// ============================================================================
// UnstakeV1
// ============================================================================

#[derive(Debug, Clone)]
pub struct UnstakeV1PublicInputs {
    pub stake_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub staker_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UnstakeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.stake_id, self.value_commit_x, self.value_commit_y, self.staker_nullifier, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct UnstakeV1CallData {
    pub table_id: pallas::Base,
    pub staker_secret: pallas::Base,
    pub staker_pub_x: pallas::Base,
    pub staker_pub_y: pallas::Base,
    pub original_amount: u64,
    pub current_amount: u64,
    pub accumulated_earnings: u64,
    pub token_id: pallas::Base,
    pub nonce: u64,
    pub staker_nullifier: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UnstakeV1CallData {
    pub fn new(
        table_id: pallas::Base,
        staker_pub: PublicKey,
        staker_secret: pallas::Base,
        original_amount: u64,
        current_amount: u64,
        accumulated_earnings: u64,
        token_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        let (sx, sy) = staker_pub.xy().expect("pk not identity");
        let stake_id = poseidon_hash([pallas::Base::from(4), table_id, sx, sy, pallas::Base::from(original_amount), pallas::Base::from(nonce)]);
        let staker_nullifier = poseidon_hash([pallas::Base::from(1), stake_id, staker_secret]);
        Self { table_id, staker_secret, staker_pub_x: sx, staker_pub_y: sy, original_amount, current_amount, accumulated_earnings, token_id, nonce, staker_nullifier, value_blind, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }
    pub fn compute_public_inputs(&self) -> UnstakeV1PublicInputs {
        let stake_id = poseidon_hash([pallas::Base::from(4), self.table_id, self.staker_pub_x, self.staker_pub_y, pallas::Base::from(self.original_amount), pallas::Base::from(self.nonce)]);
        UnstakeV1PublicInputs { stake_id, value_commit_x: pallas::Base::zero(), value_commit_y: pallas::Base::zero(), staker_nullifier: self.staker_nullifier, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.table_id)),
            Witness::Base(Value::known(self.staker_secret)),
            Witness::Base(Value::known(self.staker_pub_x)),
            Witness::Base(Value::known(self.staker_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.original_amount))),
            Witness::Base(Value::known(pallas::Base::from(self.current_amount))),
            Witness::Base(Value::known(pallas::Base::from(self.accumulated_earnings))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            Witness::Base(Value::known(self.staker_nullifier)),
            Witness::Scalar(Value::known(self.value_blind)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

pub fn unstake_v1_proof(zkbin: &ZkBinary, pk: &ProvingKey, input: &UnstakeV1CallData) -> Result<(Proof, UnstakeV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();
    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;
    Ok((proof, public_inputs))
}

// ============================================================================
// ClaimV1
// ============================================================================

#[derive(Debug, Clone)]
pub struct ClaimV1PublicInputs {
    pub stake_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub staker_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ClaimV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.stake_id, self.value_commit_x, self.value_commit_y, self.staker_nullifier, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct ClaimV1CallData {
    pub table_id: pallas::Base,
    pub staker_secret: pallas::Base,
    pub staker_pub_x: pallas::Base,
    pub staker_pub_y: pallas::Base,
    pub current_amount: u64,
    pub accumulated_earnings: u64,
    pub token_id: pallas::Base,
    pub nonce: u64,
    pub staker_nullifier: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ClaimV1CallData {
    pub fn new(
        table_id: pallas::Base,
        staker_pub: PublicKey,
        staker_secret: pallas::Base,
        current_amount: u64,
        accumulated_earnings: u64,
        token_id: pallas::Base,
        nonce: u64,
        value_blind: pallas::Scalar,
    ) -> Self {
        let (sx, sy) = staker_pub.xy().expect("pk not identity");
        let stake_id = poseidon_hash([pallas::Base::from(4), table_id, sx, sy, pallas::Base::from(current_amount), pallas::Base::from(nonce)]);
        let staker_nullifier = poseidon_hash([pallas::Base::from(1), stake_id, staker_secret]);
        Self { table_id, staker_secret, staker_pub_x: sx, staker_pub_y: sy, current_amount, accumulated_earnings, token_id, nonce, staker_nullifier, value_blind, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }
    pub fn compute_public_inputs(&self) -> ClaimV1PublicInputs {
        let stake_id = poseidon_hash([pallas::Base::from(4), self.table_id, self.staker_pub_x, self.staker_pub_y, pallas::Base::from(self.current_amount), pallas::Base::from(self.nonce)]);
        ClaimV1PublicInputs { stake_id, value_commit_x: pallas::Base::zero(), value_commit_y: pallas::Base::zero(), staker_nullifier: self.staker_nullifier, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.table_id)),
            Witness::Base(Value::known(self.staker_secret)),
            Witness::Base(Value::known(self.staker_pub_x)),
            Witness::Base(Value::known(self.staker_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.current_amount))),
            Witness::Base(Value::known(pallas::Base::from(self.accumulated_earnings))),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            Witness::Base(Value::known(self.staker_nullifier)),
            Witness::Scalar(Value::known(self.value_blind)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

pub fn claim_v1_proof(zkbin: &ZkBinary, pk: &ProvingKey, input: &ClaimV1CallData) -> Result<(Proof, ClaimV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();
    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;
    Ok((proof, public_inputs))
}

// ============================================================================
// UpdateRiskV1
// ============================================================================

#[derive(Debug, Clone)]
pub struct UpdateRiskV1PublicInputs {
    pub table_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateRiskV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.table_id, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct UpdateRiskV1CallData {
    pub betting_contract_id: pallas::Base,
    pub total_stake: u64,
    pub accumulated_losses: u64,
    pub house_edge_bp: u32,
    pub risk_profile: u8,
    pub nonce: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl UpdateRiskV1CallData {
    pub fn new(betting_contract_id: pallas::Base, total_stake: u64, accumulated_losses: u64, house_edge_bp: u32, risk_profile: u8, nonce: u64) -> Self {
        Self { betting_contract_id, total_stake, accumulated_losses, house_edge_bp, risk_profile, nonce, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }
    pub fn compute_public_inputs(&self) -> UpdateRiskV1PublicInputs {
        let table_id = poseidon_hash([pallas::Base::from(4), self.betting_contract_id, pallas::Base::from(self.nonce)]);
        UpdateRiskV1PublicInputs { table_id, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.betting_contract_id)),
            Witness::Base(Value::known(pallas::Base::from(self.total_stake))),
            Witness::Base(Value::known(pallas::Base::from(self.accumulated_losses))),
            Witness::Base(Value::known(pallas::Base::from(self.house_edge_bp as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.risk_profile as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

pub fn update_risk_v1_proof(zkbin: &ZkBinary, pk: &ProvingKey, input: &UpdateRiskV1CallData) -> Result<(Proof, UpdateRiskV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();
    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;
    Ok((proof, public_inputs))
}
