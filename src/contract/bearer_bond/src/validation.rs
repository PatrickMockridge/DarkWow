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

//! Cross-contract validation helpers for parent contracts calling bearer_bond.
//!
//! These functions are always compiled (not behind `no-entrypoint`) so that
//! caller contracts can import and use them regardless of feature flags.
//!
//! ## Governance verification
//!
//! Parent contracts that embed bearer bond for capital formation don't need
//! bespoke governance — they call `verify_coverage()` to check issuer solvency
//! against the standard coverage reports stored in the bonds_info tree.
//!
//! ## Emergency conditions
//!
//! `is_coverage_voided()` tells callers whether the bond terms have been
//! voided due to insufficient coverage, which permits emergency unstaking.

use crate::error::BearerBondError;
use crate::model::CoverageReport;

/// Minimum coverage ratio for solvency: 10000 bps = 100%.
///
/// Both principal AND interest obligations must be fully covered by reserves.
/// Anything less is insolvency and voids the bond terms.
pub const MIN_COVERAGE_RATIO_BPS: u64 = 10000;

/// Verify a coverage report proves full solvency for principal + interest.
///
/// Parent contracts call this after reading the latest [`CoverageReport`]
/// from the bearer bond's `bonds_info` tree. The parent handles the DB
/// lookup; this helper does the mathematical threshold check.
///
/// # Errors
///
/// Returns [`BearerBondError::InsufficientCoverage`] if the coverage
/// ratio is below [`MIN_COVERAGE_RATIO_BPS`].
pub fn verify_coverage(report: &CoverageReport) -> Result<(), BearerBondError> {
    if report.coverage_ratio_bps < MIN_COVERAGE_RATIO_BPS {
        return Err(BearerBondError::InsufficientCoverage {
            reported: report.coverage_ratio_bps,
        });
    }
    Ok(())
}

/// Check whether a coverage report indicates the bond terms are voided.
///
/// Returns `true` if coverage is below the minimum threshold, meaning
/// emergency unstake is permitted.
pub fn is_coverage_voided(report: &CoverageReport) -> bool {
    report.coverage_ratio_bps < MIN_COVERAGE_RATIO_BPS
}
