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

//! Liquidate ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, ScalarBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// Liquidate circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct LiquidatePublicInputs {
    /// Old commitment (position commitment before liquidation)
    pub old_commitment: pallas::Base,
    /// New commitment (after liquidation)
    pub new_commitment: pallas::Base,
    /// Position nullifier
    pub position_nullifier: pallas::Base,
}

impl LiquidatePublicInputs {
    /// Convert to vector for ZK proof creation
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.position_nullifier,  // nullifier_check constrained as instance
        ]
    }
}

/// Input data for Liquidate proof generation
#[derive(Debug, Clone)]
pub struct LiquidateCallData {
    /// Owner's secret key
    pub owner_secret: pallas::Base,
    /// Collateral amount at time of liquidation
    pub collateral_amount: u64,
    /// Debt amount at time of liquidation
    pub debt_amount: u64,
    /// Liquidation penalty (basis points)
    pub liquidation_penalty: u64,
    /// Current price of collateral (in stablecoin terms)
    pub current_price: u64,
    /// Reward to liquidator (seized collateral)
    pub liquidator_reward: u64,
    /// Collateral blinding factor
    pub collateral_blind: ScalarBlind,
    /// Debt blinding factor
    pub debt_blind: ScalarBlind,
    /// Old commitment (position commitment before liquidation)
    pub old_commitment: pallas::Base,
}

impl LiquidateCallData {
    /// Create new call data
    pub fn new(
        owner_secret: pallas::Base,
        collateral_amount: u64,
        debt_amount: u64,
        liquidation_penalty: u64,
        current_price: u64,
        liquidator_reward: u64,
        collateral_blind: ScalarBlind,
        debt_blind: ScalarBlind,
        old_commitment: pallas::Base,
    ) -> Self {
        Self {
            owner_secret,
            collateral_amount,
            debt_amount,
            liquidation_penalty,
            current_price,
            liquidator_reward,
            collateral_blind,
            debt_blind,
            old_commitment,
        }
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> LiquidatePublicInputs {
        // Compute nullifier
        let position_nullifier = poseidon_hash([self.owner_secret, self.old_commitment]);

        LiquidatePublicInputs {
            old_commitment: self.old_commitment,
            new_commitment: pallas::Base::zero(), // Will be computed by circuit
            position_nullifier,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs
            Witness::Base(Value::known(self.old_commitment)), // old_commitment
            Witness::Base(Value::known(pallas::Base::zero())), // new_commitment (computed by circuit)
            Witness::Base(Value::known(self.compute_public_inputs().position_nullifier)), // position_nullifier
            Witness::Base(Value::known(pallas::Base::from(self.collateral_amount))), // collateral_amount
            Witness::Base(Value::known(pallas::Base::from(self.debt_amount))), // debt_amount
            Witness::Base(Value::known(pallas::Base::from(self.liquidation_penalty))), // liquidation_penalty
            Witness::Base(Value::known(pallas::Base::from(self.current_price))), // current_price
            Witness::Base(Value::known(pallas::Base::zero())), // position_root (placeholder)
            // Private inputs
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Scalar(Value::known(self.collateral_blind.inner())),
            Witness::Scalar(Value::known(self.debt_blind.inner())),
            Witness::Base(Value::known(pallas::Base::from(self.liquidator_reward))), // liquidator_reward
        ]
    }
}

/// Create a Liquidate ZK proof
pub fn create_liquidate_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &LiquidateCallData,
) -> Result<(Proof, LiquidatePublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
