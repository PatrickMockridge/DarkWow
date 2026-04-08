/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! MintStable ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, pedersen_commitment_u64, ScalarBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// MintStable circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct MintStablePublicInputs {
    /// Old commitment (current position commitment)
    pub old_commitment: pallas::Base,
    /// New commitment (after minting)
    pub new_commitment: pallas::Base,
    /// Position nullifier
    pub position_nullifier: pallas::Base,
}

impl MintStablePublicInputs {
    /// Convert to vector for ZK proof creation
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.position_nullifier,  // nullifier_check constrained as instance
        ]
    }
}

/// Input data for MintStable proof generation
#[derive(Debug, Clone)]
pub struct MintStableCallData {
    /// Owner's secret key
    pub owner_secret: pallas::Base,
    /// Old collateral amount
    pub old_collateral: u64,
    /// Old debt amount
    pub old_debt: u64,
    /// New collateral amount (same as old for simple minting)
    pub new_collateral: u64,
    /// New debt amount (old_debt + mint_amount)
    pub new_debt: u64,
    /// Mint amount
    pub mint_amount: u64,
    /// Collateral blinding factor
    pub collateral_blind: ScalarBlind,
    /// Debt blinding factor
    pub debt_blind: ScalarBlind,
    /// Old commitment (position commitment from previous state)
    pub old_commitment: pallas::Base,
}

impl MintStableCallData {
    /// Create new call data
    pub fn new(
        owner_secret: pallas::Base,
        old_collateral: u64,
        old_debt: u64,
        mint_amount: u64,
        collateral_blind: ScalarBlind,
        debt_blind: ScalarBlind,
        old_commitment: pallas::Base,
    ) -> Self {
        let new_debt = old_debt + mint_amount;
        Self {
            owner_secret,
            old_collateral,
            old_debt,
            new_collateral: old_collateral, // collateral unchanged for simple minting
            new_debt,
            mint_amount,
            collateral_blind,
            debt_blind,
            old_commitment,
        }
    }

    /// Compute the Pedersen commitment for collateral
    pub fn collateral_commitment(&self, amount: u64) -> pallas::Point {
        pedersen_commitment_u64(amount, self.collateral_blind)
    }

    /// Compute the Pedersen commitment for debt
    pub fn debt_commitment(&self, amount: u64) -> pallas::Point {
        pedersen_commitment_u64(amount, self.debt_blind)
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> MintStablePublicInputs {
        // Compute nullifier
        let position_nullifier = poseidon_hash([self.owner_secret, self.old_commitment]);

        // For mint_stable, the circuit computes:
        // - nullifier_check = poseidon_hash(owner_secret, old_commitment)
        // The old_commitment and new_commitment are verified via Pedersen commitments
        // but the circuit doesn't directly constrain them as instances

        MintStablePublicInputs {
            old_commitment: self.old_commitment,
            new_commitment: pallas::Base::zero(), // Will be computed by circuit
            position_nullifier,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let _old_collateral_commit = self.collateral_commitment(self.old_collateral);
        let _old_debt_commit = self.debt_commitment(self.old_debt);
        let _new_collateral_commit = self.collateral_commitment(self.new_collateral);
        let _new_debt_commit = self.debt_commitment(self.new_debt);

        vec![
            // Public inputs
            Witness::Base(Value::known(self.old_commitment)), // old_commitment
            Witness::Base(Value::known(pallas::Base::zero())), // new_commitment (computed by circuit)
            Witness::Base(Value::known(self.compute_public_inputs().position_nullifier)), // position_nullifier
            Witness::Base(Value::known(pallas::Base::from(self.mint_amount))), // mint_amount
            Witness::Base(Value::known(pallas::Base::zero())), // position_root (placeholder)
            // Private inputs
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.old_collateral))),
            Witness::Base(Value::known(pallas::Base::from(self.old_debt))),
            Witness::Base(Value::known(pallas::Base::from(self.new_collateral))),
            Witness::Base(Value::known(pallas::Base::from(self.new_debt))),
            Witness::Scalar(Value::known(self.collateral_blind.inner())),
            Witness::Scalar(Value::known(self.debt_blind.inner())),
        ]
    }
}

/// Create a MintStable ZK proof
pub fn create_mint_stable_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &MintStableCallData,
) -> Result<(Proof, MintStablePublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
