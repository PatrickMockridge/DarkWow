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

//! GovernanceReport ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// GovernanceReport circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct GovernanceReportPublicInputs {
    /// Total collateral in the system
    pub total_collateral: pallas::Base,
    /// Total debt in the system
    pub total_debt: pallas::Base,
    /// Collateral ratio in basis points (e.g., 15000 = 150%)
    pub collateral_ratio_bps: pallas::Base,
    /// Interest accrued
    pub interest_accrued: pallas::Base,
    /// Report timestamp
    pub report_timestamp: pallas::Base,
    /// Reporter public key X coordinate
    pub reporter_pub_x: pallas::Base,
    /// Reporter public key Y coordinate
    pub reporter_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
}

impl GovernanceReportPublicInputs {
    /// Convert to vector for ZK proof creation
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.total_collateral,
            self.total_debt,
            self.collateral_ratio_bps,
            self.interest_accrued,
            self.report_timestamp,
            self.reporter_pub_x,
            self.reporter_pub_y,
            self.tx_commitment,
        ]
    }
}

/// Input data for GovernanceReport proof generation
#[derive(Debug, Clone)]
pub struct GovernanceReportCallData {
    /// Reporter's secret key
    pub reporter_secret: pallas::Base,
    /// Total collateral amount
    pub total_collateral: u64,
    /// Total debt amount
    pub total_debt: u64,
    /// Interest rate per second (in basis points)
    pub rate_per_second: u64,
    /// Time elapsed since last report (in seconds)
    pub time_elapsed: u64,
    /// Interest accrued (computed)
    pub interest_accrued: u64,
    /// Report timestamp
    pub report_timestamp: u64,
    /// Collateral ratio in basis points
    pub collateral_ratio_bps: u64,
    pub tx_commitment: pallas::Base,
}

impl GovernanceReportCallData {
    /// Create new call data
    pub fn new(
        reporter_secret: pallas::Base,
        total_collateral: u64,
        total_debt: u64,
        rate_per_second: u64,
        time_elapsed: u64,
        report_timestamp: u64,
    ) -> Self {
        // Compute interest: debt * rate * time / denominator
        // denominator = 365 * 86400 * 10000 = 315360000000
        let denominator = 315_360_000_000u64;
        let interest_accrued = (total_debt as u128)
            .saturating_mul(rate_per_second as u128)
            .saturating_mul(time_elapsed as u128)
            .saturating_div(denominator as u128) as u64;

        // Compute collateral ratio: collateral / debt * 10000
        let collateral_ratio_bps = if total_debt > 0 {
            ((total_collateral as u128) * 10000u128 / total_debt as u128) as u64
        } else {
            0
        };

        Self {
            reporter_secret,
            total_collateral,
            total_debt,
            rate_per_second,
            time_elapsed,
            interest_accrued,
            report_timestamp,
            collateral_ratio_bps,
            tx_commitment: pallas::Base::zero(),
        }
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> GovernanceReportPublicInputs {
        // Derive reporter public key from secret
        let reporter_public = PublicKey::from_secret(SecretKey::from(self.reporter_secret));
        let (reporter_pub_x, reporter_pub_y) = reporter_public.xy();

        GovernanceReportPublicInputs {
            total_collateral: pallas::Base::from(self.total_collateral),
            total_debt: pallas::Base::from(self.total_debt),
            collateral_ratio_bps: pallas::Base::from(self.collateral_ratio_bps),
            interest_accrued: pallas::Base::from(self.interest_accrued),
            report_timestamp: pallas::Base::from(self.report_timestamp),
            reporter_pub_x,
            reporter_pub_y,
            tx_commitment: self.tx_commitment,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();

        vec![
            // Public inputs
            Witness::Base(Value::known(public_inputs.total_collateral)),
            Witness::Base(Value::known(public_inputs.total_debt)),
            Witness::Base(Value::known(public_inputs.collateral_ratio_bps)),
            Witness::Base(Value::known(public_inputs.interest_accrued)),
            Witness::Base(Value::known(public_inputs.report_timestamp)),
            Witness::Base(Value::known(public_inputs.reporter_pub_x)),
            Witness::Base(Value::known(public_inputs.reporter_pub_y)),
            // Private inputs
            Witness::Base(Value::known(self.reporter_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.rate_per_second))),
            Witness::Base(Value::known(pallas::Base::from(self.time_elapsed))),
        ]
    }
}

/// Create a GovernanceReport ZK proof
pub fn create_governance_report_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &GovernanceReportCallData,
) -> Result<(Proof, GovernanceReportPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}