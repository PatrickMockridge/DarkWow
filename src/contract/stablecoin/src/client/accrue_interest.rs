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

//! AccrueInterest ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// AccrueInterest circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct AccrueInterestPublicInputs {
    /// Old total debt before interest accrual
    pub old_total_debt: pallas::Base,
    /// New total debt after interest accrual
    pub new_total_debt: pallas::Base,
    /// Interest amount calculated
    pub interest_amount: pallas::Base,
    /// Interest rate per second (in basis points)
    pub rate_per_second: pallas::Base,
    /// Time elapsed in seconds
    pub time_elapsed: pallas::Base,
    /// Accumulator public key X coordinate
    pub accumulator_pub_x: pallas::Base,
    /// Accumulator public key Y coordinate
    pub accumulator_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AccrueInterestPublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order matches constrain_instance calls in accrue_interest.zk:
    /// old_total_debt, tx_binding, tx_nonce
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.old_total_debt,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for AccrueInterest proof generation
#[derive(Debug, Clone)]
pub struct AccrueInterestCallData {
    /// Accumulator's secret key
    pub accumulator_secret: pallas::Base,
    /// Old total debt
    pub old_total_debt: u64,
    /// Interest rate per second (in basis points)
    pub rate_per_second: u64,
    /// Time elapsed since last accrual (in seconds)
    pub time_elapsed: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AccrueInterestCallData {
    /// Create new call data
    pub fn new(
        accumulator_secret: pallas::Base,
        old_total_debt: u64,
        rate_per_second: u64,
        time_elapsed: u64,
    ) -> Self {
        Self { accumulator_secret, old_total_debt, rate_per_second, time_elapsed, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    /// Compute the interest amount
    pub fn compute_interest(&self) -> u64 {
        // interest = debt * rate * time / denominator
        // denominator = 365 * 86400 * 10000 = 315360000000
        let denominator = 315_360_000_000u64;
        ((self.old_total_debt as u128)
            .saturating_mul(self.rate_per_second as u128)
            .saturating_mul(self.time_elapsed as u128)
            .saturating_div(denominator as u128)) as u64
    }

    /// Compute the new total debt
    pub fn compute_new_total_debt(&self) -> u64 {
        self.old_total_debt.saturating_add(self.compute_interest())
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> AccrueInterestPublicInputs {
        // Derive accumulator public key from secret
        let accumulator_public = PublicKey::from_secret(SecretKey::from_base(self.accumulator_secret));
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (accumulator_pub_x, accumulator_pub_y) = accumulator_public.xy().expect("pk not identity");

        let interest_amount = self.compute_interest();
        let new_total_debt = self.compute_new_total_debt();

        AccrueInterestPublicInputs {
            old_total_debt: pallas::Base::from(self.old_total_debt),
            new_total_debt: pallas::Base::from(new_total_debt),
            interest_amount: pallas::Base::from(interest_amount),
            rate_per_second: pallas::Base::from(self.rate_per_second),
            time_elapsed: pallas::Base::from(self.time_elapsed),
            accumulator_pub_x,
            accumulator_pub_y,
            tx_binding: poseidon_hash([pallas::Base::from(3), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();

        vec![
            // Public inputs
            Witness::Base(Value::known(public_inputs.old_total_debt)),
            Witness::Base(Value::known(public_inputs.new_total_debt)),
            Witness::Base(Value::known(public_inputs.interest_amount)),
            Witness::Base(Value::known(public_inputs.rate_per_second)),
            Witness::Base(Value::known(public_inputs.time_elapsed)),
            Witness::Base(Value::known(public_inputs.accumulator_pub_x)),
            Witness::Base(Value::known(public_inputs.accumulator_pub_y)),
            // Private inputs
            Witness::Base(Value::known(self.accumulator_secret)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create an AccrueInterest ZK proof
pub fn create_accrue_interest_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AccrueInterestCallData,
) -> Result<(Proof, AccrueInterestPublicInputs)> {
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