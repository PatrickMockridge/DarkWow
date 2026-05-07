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

//! Liquidate ZK proof generation (Poseidon-only)

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, BaseBlind},
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
            self.old_commitment,
            self.new_commitment,
            self.position_nullifier,
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
    /// Collateral blinding factor (BaseBlind, not ScalarBlind)
    pub collateral_blind: BaseBlind,
    /// Debt blinding factor (BaseBlind, not ScalarBlind)
    pub debt_blind: BaseBlind,
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
        collateral_blind: BaseBlind,
        debt_blind: BaseBlind,
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

    /// Compute the owner's public key (Poseidon hash of secret)
    pub fn owner_public_key(&self) -> pallas::Base {
        poseidon_hash([self.owner_secret])
    }

    /// Compute the Poseidon commitment for collateral
    pub fn collateral_commitment(&self, amount: u64) -> pallas::Base {
        poseidon_hash([pallas::Base::from(amount), self.collateral_blind.inner()])
    }

    /// Compute the Poseidon commitment for debt
    pub fn debt_commitment(&self, amount: u64) -> pallas::Base {
        poseidon_hash([pallas::Base::from(amount), self.debt_blind.inner()])
    }

    /// Compute the old position commitment
    pub fn old_position_commitment(&self) -> pallas::Base {
        let collateral_commit = self.collateral_commitment(self.collateral_amount);
        let debt_commit = self.debt_commitment(self.debt_amount);
        let owner_pub = self.owner_public_key();
        poseidon_hash([collateral_commit, debt_commit, owner_pub])
    }

    /// Compute the new position commitment after liquidation
    pub fn new_position_commitment(&self) -> pallas::Base {
        // New collateral after seizure
        let new_collateral = self.collateral_amount.saturating_sub(self.liquidator_reward);
        let collateral_commit = self.collateral_commitment(new_collateral);
        let debt_commit = self.debt_commitment(self.debt_amount);
        let owner_pub = self.owner_public_key();
        poseidon_hash([collateral_commit, debt_commit, owner_pub])
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> LiquidatePublicInputs {
        // Compute nullifier
        let position_nullifier = poseidon_hash([self.owner_secret, self.old_commitment]);

        LiquidatePublicInputs {
            old_commitment: self.old_commitment,
            new_commitment: self.new_position_commitment(),
            position_nullifier,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let owner_pub = self.owner_public_key();
        let new_position = self.new_position_commitment();
        let new_collateral = self.collateral_amount.saturating_sub(self.liquidator_reward);

        vec![
            // Public inputs
            Witness::Base(Value::known(self.old_commitment)), // old_commitment
            Witness::Base(Value::known(new_position)), // new_commitment
            Witness::Base(Value::known(self.compute_public_inputs().position_nullifier)), // position_nullifier
            Witness::Base(Value::known(pallas::Base::from(self.collateral_amount))), // collateral_amount
            Witness::Base(Value::known(pallas::Base::from(self.debt_amount))), // debt_amount
            Witness::Base(Value::known(pallas::Base::from(self.liquidation_penalty))), // liquidation_penalty
            Witness::Base(Value::known(pallas::Base::from(self.current_price))), // current_price
            Witness::Base(Value::known(pallas::Base::zero())), // position_root (placeholder)
            // Private inputs
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Base(Value::known(self.collateral_blind.inner())), // BaseBlind as Base
            Witness::Base(Value::known(self.debt_blind.inner())), // BaseBlind as Base
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

// ============================================================================
// MoneyV3 Integration: Liquidation via spend_hook
// ============================================================================
// Liquidation in the pooled debt model works differently than individual CDPs:
//
// 1. Pool is undercollateralized (global ratio < threshold)
// 2. Liquidator calls MoneyV3::BurnV1 to burn stablecoin (spend_hook = stablecoin)
// 3. spend_hook triggers stablecoin's exec() with seizure parameters
// 4. Stablecoin updates pool state, releases collateral proportionally
// 5. Liquidator receives seized collateral via MoneyV3::MintV1
//
// In the pooled model, there's no individual position - the entire pool is either:
// - Healthy (all collateral backs all debt)
// - Liquidated (seizure triggered, all positions affected)

// ============================================================================
// INTEGRATION NOTES: spend_hook Callback Handler
// ============================================================================
// The spend_hook mechanism enables atomic cross-contract operations:
//
// 1. User calls MoneyV3::BurnV1 with spend_hook = stablecoin_contract_id
//    user_data encodes: (mint_amount, stablecoin_token_id, sender_pub)
//
// 2. MoneyV3 verifies the burn proof, marks nullifier as spent
//
// 3. MoneyV3 calls stablecoin_contract.exec(user_data)
//    - This is the callback that stablecoin must implement
//    - stablecoin receives: mint_amount, sender, etc.
//
// 4. Stablecoin's exec() should:
//    - Verify the burn was valid (check nullifier)
//    - Update pool state (decrease debt, decrease collateral)
//    - Return success so MoneyV3 commits the burn
//
// 5. If stablecoin's exec() succeeds, MoneyV3 finalizes the burn
//    If stablecoin's exec() fails, the entire tx aborts (atomic)
//
// NOTE: The actual spend_hook exec() handler in stablecoin entrypoint
// has not been implemented yet - this is documented here as the
// integration point for future work.