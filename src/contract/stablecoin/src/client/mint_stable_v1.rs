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

//! MintStable ZK proof generation (Poseidon-only)
//!
//! ## MoneyV3 Integration
//!
//! When minting stablecoin:
//! 1. User burns collateral receipt tokens via MoneyV3::BurnV1 with spend_hook
//! 2. The spend_hook triggers stablecoin's exec() callback
//! 3. Stablecoin verifies the burn, then mints stablecoin tokens via MoneyV3::MintV1
//! 4. User receives stablecoin tokens
//!
//! The spend_hook enables atomic: burn collateral → mint stablecoin

use dwow::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, BaseBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;

use crate::model::{CollateralType, MintStableParams};

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
            self.old_commitment,
            self.new_commitment,
            self.position_nullifier,
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
    /// Collateral blinding factor (BaseBlind, not ScalarBlind)
    pub collateral_blind: BaseBlind,
    /// Debt blinding factor (BaseBlind, not ScalarBlind)
    pub debt_blind: BaseBlind,
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
        collateral_blind: BaseBlind,
        debt_blind: BaseBlind,
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

    /// Compute the Poseidon commitment for collateral
    /// Uses poseidon_hash(amount, blind) instead of Pedersen
    pub fn collateral_commitment(&self, amount: u64) -> pallas::Base {
        poseidon_hash([pallas::Base::from(amount), self.collateral_blind.inner()])
    }

    /// Compute the Poseidon commitment for debt
    /// Uses poseidon_hash(amount, blind) instead of Pedersen
    pub fn debt_commitment(&self, amount: u64) -> pallas::Base {
        poseidon_hash([pallas::Base::from(amount), self.debt_blind.inner()])
    }

    /// Compute the owner's public key (Poseidon hash of secret)
    pub fn owner_public_key(&self) -> pallas::Base {
        poseidon_hash([self.owner_secret])
    }

    /// Compute the old position commitment
    pub fn old_position_commitment(&self) -> pallas::Base {
        let collateral_commit = self.collateral_commitment(self.old_collateral);
        let debt_commit = self.debt_commitment(self.old_debt);
        let owner_pub = self.owner_public_key();
        poseidon_hash([collateral_commit, debt_commit, owner_pub])
    }

    /// Compute the new position commitment
    pub fn new_position_commitment(&self) -> pallas::Base {
        let collateral_commit = self.collateral_commitment(self.new_collateral);
        let debt_commit = self.debt_commitment(self.new_debt);
        let owner_pub = self.owner_public_key();
        poseidon_hash([collateral_commit, debt_commit, owner_pub])
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> MintStablePublicInputs {
        // Compute nullifier
        let position_nullifier = poseidon_hash([self.owner_secret, self.old_commitment]);

        MintStablePublicInputs {
            old_commitment: self.old_commitment,
            new_commitment: self.new_position_commitment(),
            position_nullifier,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let owner_pub = self.owner_public_key();
        let old_collateral_commit = self.collateral_commitment(self.old_collateral);
        let old_debt_commit = self.debt_commitment(self.old_debt);
        let new_collateral_commit = self.collateral_commitment(self.new_collateral);
        let new_debt_commit = self.debt_commitment(self.new_debt);
        let new_position = self.new_position_commitment();

        vec![
            // Public inputs
            Witness::Base(Value::known(self.old_commitment)), // old_commitment
            Witness::Base(Value::known(new_position)), // new_commitment
            Witness::Base(Value::known(self.compute_public_inputs().position_nullifier)), // position_nullifier
            Witness::Base(Value::known(pallas::Base::from(self.mint_amount))), // mint_amount
            Witness::Base(Value::known(pallas::Base::zero())), // position_root (placeholder)
            // Private inputs
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.old_collateral))),
            Witness::Base(Value::known(pallas::Base::from(self.old_debt))),
            Witness::Base(Value::known(pallas::Base::from(self.new_collateral))),
            Witness::Base(Value::known(pallas::Base::from(self.new_debt))),
            Witness::Base(Value::known(self.collateral_blind.inner())), // BaseBlind as Base, not Scalar
            Witness::Base(Value::known(self.debt_blind.inner())), // BaseBlind as Base, not Scalar
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

// ============================================================================
// MoneyV3 Integration: Collateral Burn for Stablecoin Mint
// ============================================================================

/// Debris for burning collateral tokens via MoneyV3 to mint stablecoin
///
/// Flow:
/// 1. User calls MoneyV3::BurnV1 with spend_hook = stablecoin contract
/// 2. user_data encodes the mint parameters
/// 3. Stablecoin's exec() is called with user_data
/// 4. Stablecoin verifies burn, then mints stablecoin to user via MoneyV3::MintV1
#[derive(Debug, Clone)]
pub struct CollateralBurnDebris {
    /// The coin being burned (collateral receipt)
    pub coin: pallas::Base,
    /// Nullifier for the burned coin
    pub nullifier: pallas::Base,
    /// Stablecoin contract ID (spend_hook target)
    pub spend_hook: pallas::Base,
    /// User data encoding mint parameters for stablecoin
    pub user_data: pallas::Base,
    /// Mint amount (stablecoin to receive)
    pub mint_amount: u64,
}

/// Builder for burning collateral to mint stablecoin
///
/// Usage:
/// ```ignore
/// let burn_debris = CollateralBurnBuilder {
///     collateral_coin: coin_from_open_position,
///     owner_secret: user_secret,
///     mint_amount: 1000,
///     stablecoin_contract_id: stablecoin_id,
///     token_id: collateral_token_id,
/// }.build();
///
/// // Execute MoneyV3::BurnV1 with spend_hook to stablecoin
/// // Stablecoin's exec() verifies burn and mints stablecoin
/// ```
pub struct CollateralBurnBuilder {
    /// The collateral receipt coin to burn
    pub collateral_coin: pallas::Base,
    /// Owner's secret key
    pub owner_secret: pallas::Base,
    /// Amount of stablecoin to mint
    pub mint_amount: u64,
    /// Stablecoin contract ID (for spend_hook)
    pub stablecoin_contract_id: pallas::Base,
    /// Token ID for stablecoin
    pub stablecoin_token_id: pallas::Base,
    /// Collateral token ID (being burned)
    pub collateral_token_id: pallas::Base,
}

impl CollateralBurnBuilder {
    /// Build the collateral burn debris for MoneyV3::BurnV1
    pub fn build(&self) -> CollateralBurnDebris {
        // Compute nullifier: poseidon_hash(secret, coin)
        let nullifier = poseidon_hash([self.owner_secret, self.collateral_coin]);

        // User data encodes mint params: (mint_amount, stablecoin_token_id, sender_pub)
        // This gets passed to stablecoin's exec() during spend_hook callback
        let user_data = poseidon_hash([
            pallas::Base::from(self.mint_amount),
            self.stablecoin_token_id,
            poseidon_hash([self.owner_secret]), // sender public key
        ]);

        CollateralBurnDebris {
            coin: self.collateral_coin,
            nullifier,
            spend_hook: self.stablecoin_contract_id,
            user_data,
            mint_amount: self.mint_amount,
        }
    }
}