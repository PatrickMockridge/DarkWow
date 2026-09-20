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
//!
//! The ratio is derived here, exactly as the specification derives it
//! (`sim/contracts/bearer_bond.py:271-275`):
//!
//!   total_obligation   = total_outstanding + total_interest_obligation
//!   coverage_ratio_bps = (reserve_amount * 10000) / total_obligation
//!
//! `ProveCoverage_V2` proves that arithmetic and exposes all four numbers as public inputs, so the
//! report the host stores is the report the proof checked. There is deliberately no
//! `coverage_ratio_bps` field on the input: a caller-supplied ratio is a claim, and the whole
//! point of the proof is that the ratio is computed from the amounts rather than asserted.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::ProveCoverageParamsV1;

/// Basis-points denominator — `BP_PRECISION` in the model.
const BP_PRECISION: u64 = 10000;

/// Public inputs for `ProveCoverage_V2`, in the circuit's `constrain_instance` order:
/// [reserve_amount, total_outstanding, total_interest_obligation, coverage_ratio_bps].
pub struct ProveCoverageRevealed {
    pub reserve_amount: pallas::Base,
    pub total_outstanding: pallas::Base,
    pub total_interest_obligation: pallas::Base,
    pub coverage_ratio_bps: pallas::Base,
}

impl ProveCoverageRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.reserve_amount,
            self.total_outstanding,
            self.total_interest_obligation,
            self.coverage_ratio_bps,
        ]
    }
}

/// Input for building a ProveCoverage call.
pub struct ProveCoverageCallInput {
    /// Staking pool series identifier
    pub series_asset_id: pallas::Base,
    /// Total staked principal across all stake commitments in the series
    pub total_outstanding: u64,
    /// Total accrued interest obligation across all outstanding stakes
    pub total_interest_obligation: u64,
    /// Issuer's reserve balance
    pub reserve_amount: u64,
    /// Block height of this report
    pub report_block: u64,
}

/// `coverage_ratio_bps = (reserve_amount * 10000) / total_obligation`, integer division, where
/// `total_obligation = total_outstanding + total_interest_obligation` — the model's formula.
///
/// `None` when the obligation is zero (the model rejects this too: "Total obligation is zero") or
/// when the ratio does not fit in 64 bits, which is the bound `ProveCoverage_V2` range-checks.
fn coverage_ratio_bps(
    reserve_amount: u64,
    total_outstanding: u64,
    total_interest_obligation: u64,
) -> Option<u64> {
    let total_obligation = total_outstanding.checked_add(total_interest_obligation)?;
    if total_obligation == 0 {
        return None;
    }
    // `reserve_amount * 10000` overflows u64 — widen. The quotient is < 10000 * 2^64, so it can
    // exceed u64 too; reject rather than truncate, since a truncated ratio is a wrong ratio and
    // the circuit's `range_check(64, coverage_ratio_bps)` would reject it anyway.
    let ratio = (u128::from(reserve_amount) * u128::from(BP_PRECISION)) / u128::from(total_obligation);
    u64::try_from(ratio).ok()
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
    /// `ProveCoverage_V2` zkas circuit ZkBinary
    pub prove_coverage_zkbin: ZkBinary,
    /// Proving key for ProveCoverage_V2
    pub prove_coverage_pk: ProvingKey,
}

impl ProveCoverageCallBuilder {
    /// Build the ProveCoverage call debris.
    pub fn build(self) -> Result<ProveCoverageCallDebris> {
        debug!(target: "contract::bearer_bond::client::prove_coverage", "Building BearerBond::ProveCoverageV1 contract call");

        let coverage_ratio_bps = coverage_ratio_bps(
            self.input.reserve_amount,
            self.input.total_outstanding,
            self.input.total_interest_obligation,
        )
        .ok_or_else(|| {
            dwow_core::Error::Custom(format!(
                "ProveCoverage: total obligation {} + {} is zero, or the ratio exceeds 64 bits",
                self.input.total_outstanding, self.input.total_interest_obligation,
            ))
        })?;

        let (proof, _revealed) = create_prove_coverage_proof(
            &self.prove_coverage_zkbin,
            &self.prove_coverage_pk,
            &self.input,
            coverage_ratio_bps,
        )?;

        Ok(ProveCoverageCallDebris {
            params: ProveCoverageParamsV1 {
                series_asset_id: self.input.series_asset_id,
                total_outstanding: self.input.total_outstanding,
                total_interest_obligation: self.input.total_interest_obligation,
                reserve_amount: self.input.reserve_amount,
                coverage_ratio_bps,
                report_block: self.input.report_block,
                proof: vec![],
            },
            proofs: vec![proof],
        })
    }
}

/// Create a `ProveCoverage_V2` ZK proof.
///
/// Witness order must match `ProveCoverage_V2`:
/// reserve_amount, total_outstanding, total_interest_obligation, coverage_ratio_bps
fn create_prove_coverage_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ProveCoverageCallInput,
    coverage_ratio_bps: u64,
) -> Result<(Proof, ProveCoverageRevealed)> {
    let public_inputs = ProveCoverageRevealed {
        reserve_amount: pallas::Base::from(input.reserve_amount),
        total_outstanding: pallas::Base::from(input.total_outstanding),
        total_interest_obligation: pallas::Base::from(input.total_interest_obligation),
        coverage_ratio_bps: pallas::Base::from(coverage_ratio_bps),
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(input.reserve_amount))),
        Witness::Base(Value::known(pallas::Base::from(input.total_outstanding))),
        Witness::Base(Value::known(pallas::Base::from(input.total_interest_obligation))),
        Witness::Base(Value::known(pallas::Base::from(coverage_ratio_bps))),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
