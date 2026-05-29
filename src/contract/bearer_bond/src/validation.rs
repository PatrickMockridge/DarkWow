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

use crate::error::BearerBondError;
use crate::model::CoverageReport;

/// Minimum coverage ratio for solvency: 10000 bps = 100%.
///
/// Stake principal MUST be fully covered by reserves. Anything less is
/// insolvency — unlike stablecoins which allow fractional reserves.
pub const MIN_COVERAGE_RATIO_BPS: u64 = 10000;

/// Verify a coverage report proves full solvency.
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
