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

//! OpenPosition ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, pedersen_commitment_u64, PublicKey, SecretKey, ScalarBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// OpenPosition circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct OpenPositionPublicInputs {
    /// Position commitment = poseidon_hash(collateral_x, collateral_y, debt_x, debt_y, owner_pub_x, owner_pub_y)
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
    /// Owner's secret key
    pub owner_secret: SecretKey,
    /// Owner's public key (derived from secret)
    pub owner_public: PublicKey,
    /// Collateral amount (u64)
    pub collateral_amount: u64,
    /// Debt amount (u64)
    pub debt_amount: u64,
    /// Collateral type
    pub collateral_type: pallas::Base,
    /// Collateral blinding factor
    pub collateral_blind: ScalarBlind,
    /// Debt blinding factor
    pub debt_blind: ScalarBlind,
}

impl OpenPositionCallData {
    /// Create new call data with random blinds
    pub fn new(
        owner_secret: SecretKey,
        collateral_amount: u64,
        debt_amount: u64,
        collateral_type: pallas::Base,
    ) -> Self {
        let owner_public = PublicKey::from_secret(owner_secret);
        let collateral_blind = ScalarBlind::random(&mut OsRng);
        let debt_blind = ScalarBlind::random(&mut OsRng);

        Self { owner_secret, owner_public, collateral_amount, debt_amount, collateral_type, collateral_blind, debt_blind }
    }

    /// Compute the Pedersen commitment for collateral
    pub fn collateral_commitment(&self) -> pallas::Point {
        pedersen_commitment_u64(self.collateral_amount, self.collateral_blind)
    }

    /// Compute the Pedersen commitment for debt
    pub fn debt_commitment(&self) -> pallas::Point {
        pedersen_commitment_u64(self.debt_amount, self.debt_blind)
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> OpenPositionPublicInputs {
        let collateral_commit = self.collateral_commitment();
        let debt_commit = self.debt_commitment();

        let collateral_coords = collateral_commit.to_affine().coordinates().unwrap();
        let debt_coords = debt_commit.to_affine().coordinates().unwrap();

        let (owner_pub_x, owner_pub_y) = self.owner_public.xy();

        // Compute position commitment
        let position_commitment = poseidon_hash(
            [*collateral_coords.x(), *debt_coords.x(), owner_pub_x, owner_pub_y]
        );

        // Compute nullifier
        let position_nullifier = poseidon_hash([self.owner_secret.inner(), position_commitment]);

        OpenPositionPublicInputs { position_commitment, position_nullifier }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let collateral_commit = self.collateral_commitment();
        let debt_commit = self.debt_commitment();

        let collateral_coords = collateral_commit.to_affine().coordinates().unwrap();
        let debt_coords = debt_commit.to_affine().coordinates().unwrap();

        let (owner_pub_x, owner_pub_y) = self.owner_public.xy();
        let position_commitment = poseidon_hash(
            [*collateral_coords.x(), *debt_coords.x(), owner_pub_x, owner_pub_y]
        );
        let position_nullifier = poseidon_hash([self.owner_secret.inner(), position_commitment]);

        vec![
            // Public inputs as witnesses (for constrain_instance)
            Witness::Base(Value::known(position_commitment)),
            Witness::Base(Value::known(owner_pub_x)),
            Witness::Base(Value::known(owner_pub_y)),
            Witness::Base(Value::known(self.collateral_type)),
            Witness::Base(Value::known(position_nullifier)),
            // Merkle proof placeholders (not implemented yet - would need actual tree)
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
            // Private inputs
            Witness::Base(Value::known(self.owner_secret.inner())),
            Witness::Base(Value::known(pallas::Base::from(self.collateral_amount))),
            Witness::Base(Value::known(pallas::Base::from(self.debt_amount))),
            Witness::Scalar(Value::known(self.collateral_blind.inner())),
            Witness::Scalar(Value::known(self.debt_blind.inner())),
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
