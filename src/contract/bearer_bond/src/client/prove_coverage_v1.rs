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

//! Bearer Bond ProveCoverageV1 Client API
//!
//! Issuer proves that reserves cover outstanding stake obligations.
//! The ZK circuit (ProveCoverage_V1) uses `base_div` to compute
//! `coverage_ratio_bps = reserve_amount / total_outstanding * 10000`
//! and constrains it against the submitted public input.
//!
//! The entrypoint independently verifies `reserve_amount >= total_outstanding`
//! (>= 100% coverage required).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::ProveCoverageParamsV1;

/// Public input for ProveCoverage_V1: the coverage ratio in basis points.
pub struct ProveCoverageRevealed {
    pub coverage_ratio_bps: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ProveCoverageRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.coverage_ratio_bps, self.tx_binding,
            self.tx_nonce]
    }
}

/// Input for building a ProveCoverage call.
pub struct ProveCoverageCallInput {
    /// Staking pool series identifier
    pub series_token_id: pallas::Base,
    /// Total staked principal across all stake coins in the series
    pub total_outstanding: u64,
    /// Total accrued interest obligation across all outstanding stakes
    pub total_interest_obligation: u64,
    /// Issuer's reserve balance
    pub reserve_amount: u64,
    /// coverage_ratio_bps = reserve_amount / (total_outstanding + total_interest_obligation) * 10000
    pub coverage_ratio_bps: u64,
    /// Block height of this report
    pub report_block: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Debris produced by building a ProveCoverage call.
pub struct ProveCoverageCallDebris {
    /// The contract call parameters
    pub params: ProveCoverageParamsV1,
    /// The ZK proof
    pub proofs: Vec<Proof>,
}

/// Builder for `BearerBond::ProveCoverageV1` contract call.
pub struct ProveCoverageCallBuilder {
    /// Coverage report input
    pub input: ProveCoverageCallInput,
    /// `ProveCoverage_V1` zkas circuit ZkBinary
    pub prove_coverage_zkbin: ZkBinary,
    /// Proving key for ProveCoverage_V1
    pub prove_coverage_pk: ProvingKey,
}

impl ProveCoverageCallBuilder {
    /// Build the ProveCoverage call debris.
    pub fn build(self) -> Result<ProveCoverageCallDebris> {
        debug!(target: "contract::bearer_bond::client::prove_coverage", "Building BearerBond::ProveCoverageV1 contract call");

        let (proof, _revealed) = create_prove_coverage_proof(
            &self.prove_coverage_zkbin,
            &self.prove_coverage_pk,
            &self.input,
        )?;

        Ok(ProveCoverageCallDebris {
            params: ProveCoverageParamsV1 {
                series_token_id: self.input.series_token_id,
                total_outstanding: self.input.total_outstanding,
                total_interest_obligation: self.input.total_interest_obligation,
                reserve_amount: self.input.reserve_amount,
                coverage_ratio_bps: self.input.coverage_ratio_bps,
                report_block: self.input.report_block,
                proof: vec![],
            },
            proofs: vec![proof],
        })
    }
}

/// Create a ProveCoverage_V1 ZK proof.
///
/// Witness order must match ProveCoverage_V1 circuit:
/// reserve_amount, total_outstanding, coverage_ratio_bps
fn create_prove_coverage_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ProveCoverageCallInput,
) -> Result<(Proof, ProveCoverageRevealed)> {
    let public_inputs = ProveCoverageRevealed {
        coverage_ratio_bps: pallas::Base::from(input.coverage_ratio_bps),
        tx_binding: poseidon_hash([input.tx_commitment, input.tx_nonce]),
            tx_nonce: input.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(input.reserve_amount))),
        Witness::Base(Value::known(pallas::Base::from(input.total_outstanding))),
        Witness::Base(Value::known(pallas::Base::from(input.coverage_ratio_bps))),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
