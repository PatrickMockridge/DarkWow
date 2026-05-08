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

//! OpenPosition ZK proof generation (Poseidon-only)
//!
//! ## MoneyV3 Integration
//!
//! When opening a position, the user:
//! 1. Deposits collateral (e.g., wXMR) to stablecoin
//! 2. Receives collateral receipt tokens via MoneyV3 MintV1
//!
//! The collateral receipt token is minted with spend_hook = stablecoin contract ID,
//! enabling atomic operations like MintStableV1 (burn collateral, mint stablecoin).

use dwow::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, BaseBlind, ContractId},
    pasta::pallas,
};
use rand::rngs::OsRng;

use crate::model::CollateralType;

/// OpenPosition circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct OpenPositionPublicInputs {
    /// Position commitment = poseidon_hash(collateral_commit, debt_commit, owner_pub, collateral_type)
    pub position_commitment: pallas::Base,
    /// Nullifier = poseidon_hash(owner_secret, position_commitment)
    pub position_nullifier: pallas::Base,
}

impl OpenPositionPublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in open_position_v1.zk
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.position_nullifier,  // nullifier_check constrained as instance
            self.position_commitment,  // position_check constrained as instance
        ]
    }
}

/// Input data for OpenPosition proof generation
#[derive(Debug, Clone)]
pub struct OpenPositionCallData {
    /// Owner's secret key (as pallas::Base field element, not SecretKey)
    pub owner_secret: pallas::Base,
    /// Collateral amount (u64)
    pub collateral_amount: u64,
    /// Debt amount (u64)
    pub debt_amount: u64,
    /// Collateral type (as pallas::Base field element)
    pub collateral_type: pallas::Base,
    /// Collateral blinding factor (BaseBlind, not ScalarBlind)
    pub collateral_blind: BaseBlind,
    /// Debt blinding factor (BaseBlind, not ScalarBlind)
    pub debt_blind: BaseBlind,
}

impl OpenPositionCallData {
    /// Create new call data with random blinds
    pub fn new(
        owner_secret: pallas::Base,
        collateral_amount: u64,
        debt_amount: u64,
        collateral_type: pallas::Base,
    ) -> Self {
        let collateral_blind = BaseBlind::random(&mut OsRng);
        let debt_blind = BaseBlind::random(&mut OsRng);

        Self {
            owner_secret,
            collateral_amount,
            debt_amount,
            collateral_type,
            collateral_blind,
            debt_blind,
        }
    }

    /// Compute the Poseidon commitment for collateral
    /// Uses poseidon_hash(amount, blind) instead of Pedersen
    pub fn collateral_commitment(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(self.collateral_amount), self.collateral_blind.inner()])
    }

    /// Compute the Poseidon commitment for debt
    /// Uses poseidon_hash(amount, blind) instead of Pedersen
    pub fn debt_commitment(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(self.debt_amount), self.debt_blind.inner()])
    }

    /// Compute the owner's public key (Poseidon hash of secret)
    pub fn owner_public_key(&self) -> pallas::Base {
        poseidon_hash([self.owner_secret])
    }

    /// Compute the position commitment
    pub fn position_commitment(&self) -> pallas::Base {
        let collateral_commit = self.collateral_commitment();
        let debt_commit = self.debt_commitment();
        let owner_pub = self.owner_public_key();
        poseidon_hash([collateral_commit, debt_commit, owner_pub, self.collateral_type])
    }

    /// Compute the position nullifier
    pub fn position_nullifier(&self) -> pallas::Base {
        let pos_commit = self.position_commitment();
        poseidon_hash([self.owner_secret, pos_commit])
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> OpenPositionPublicInputs {
        OpenPositionPublicInputs {
            position_commitment: self.position_commitment(),
            position_nullifier: self.position_nullifier(),
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let owner_pub = self.owner_public_key();
        let _collateral_commit = self.collateral_commitment();
        let _debt_commit = self.debt_commitment();
        let position_commitment = self.position_commitment();
        let position_nullifier = self.position_nullifier();

        vec![
            // Public inputs as witnesses (for constrain_instance)
            Witness::Base(Value::known(position_commitment)),
            Witness::Base(Value::known(owner_pub)),
            Witness::Base(Value::known(self.collateral_type)),
            Witness::Base(Value::known(position_nullifier)),
            // Merkle proof placeholders (not implemented yet - would need actual tree)
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
            // Private inputs
            Witness::Base(Value::known(self.owner_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.collateral_amount))),
            Witness::Base(Value::known(pallas::Base::from(self.debt_amount))),
            Witness::Base(Value::known(self.collateral_blind.inner())),
            Witness::Base(Value::known(self.debt_blind.inner())),
            // Merkle proof witnesses (4 levels)
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
        ]
    }
}

/// Create an OpenPosition ZK proof
pub fn create_open_position_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &OpenPositionCallData,
) -> Result<(Proof, OpenPositionPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

// ============================================================================
// MoneyV3 Integration: Collateral Token Minting
// ============================================================================

/// Debris for minting collateral receipt tokens via MoneyV3
///
/// When opening a position, the user mints collateral receipt tokens
/// via MoneyV3. These tokens have spend_hook = stablecoin contract ID,
/// enabling atomic operations (burn collateral → mint stablecoin).
#[derive(Debug, Clone)]
pub struct CollateralMintDebris {
    /// The recipient public key for minted tokens
    pub recipient_pub: pallas::Base,
    /// Amount of collateral tokens to mint
    pub mint_amount: u64,
    /// Token ID for collateral tokens
    pub token_id: pallas::Base,
    /// Spend hook - stablecoin contract ID for atomic operations
    pub spend_hook: pallas::Base,
    /// User data passed to spend hook
    pub user_data: pallas::Base,
}

/// Builder for creating collateral receipt tokens via MoneyV3
///
/// Flow:
/// 1. User calls OpenPositionV1 on stablecoin
/// 2. Client also calls MoneyV3::MintV1 to mint collateral receipt tokens
/// 3. User receives collateral tokens with spend_hook = stablecoin
/// 4. When user calls MintStableV1, tokens are burned with spend_hook trigger
pub struct CollateralMintBuilder {
    /// Owner's public key (poseidon_hash of secret)
    pub owner_pub: pallas::Base,
    /// Collateral amount
    pub collateral_amount: u64,
    /// Collateral type
    pub collateral_type: CollateralType,
    /// Token ID for this collateral type
    pub token_id: pallas::Base,
    /// Stablecoin contract ID (for spend_hook)
    pub stablecoin_contract_id: ContractId,
    /// User data for spend_hook (position commitment)
    pub user_data: pallas::Base,
}

impl CollateralMintBuilder {
    /// Build the collateral mint call debris
    pub fn build(&self) -> CollateralMintDebris {
        // The spend_hook is the stablecoin contract ID, enabling atomic
        // operations like burn collateral → mint stablecoin
        let spend_hook = self.stablecoin_contract_id.inner();

        CollateralMintDebris {
            recipient_pub: self.owner_pub,
            mint_amount: self.collateral_amount,
            token_id: self.token_id,
            spend_hook,
            user_data: self.user_data,
        }
    }
}