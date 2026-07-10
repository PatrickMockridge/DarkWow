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

//! Circuit public input trait — structural prevention for ZK instance count mismatches.
//!
//! Every ZK circuit has a fixed number of `constrain_instance()` calls.
//! The `CircuitPublicInputs` trait makes this count a named constant
//! and provides the single source of truth for `to_public_inputs()`.
//!
//! The `circuit_instance_counts` integration test enforces that:
//! - `COUNT` matches the number of `constrain_instance()` calls in the .zk file
//! - `to_public_inputs()` returns exactly `COUNT` elements
//!
//! Note: `generic_const_exprs` is unstable, so `[pallas::Base; Self::COUNT]`
//! cannot be used as the return type. The test-time assertion provides the
//! equivalent enforcement until that feature stabilizes.

use pasta_curves::pallas;

/// Maps a ZK circuit namespace to its public input count and ordering.
///
/// Implemented once per circuit type. The `COUNT` constant is the SINGLE SOURCE
/// OF TRUTH for all callers — `to_vec()`, `get_metadata()`, and circuit
/// construction MUST all derive their element counts from this constant.
///
/// # Safety invariant (enforced at test time)
///
/// `COUNT` MUST equal the number of `constrain_instance()` calls in the
/// corresponding `.zk` file. `to_public_inputs()` MUST return exactly
/// `COUNT` elements in the same order as the circuit's `constrain_instance()`
/// statements.
pub trait CircuitPublicInputs {
    /// Number of public inputs this circuit constrains.
    /// Must equal the number of `constrain_instance()` calls in the .zk file.
    const COUNT: usize;

    /// Ordered public inputs matching the circuit's `constrain_instance()` sequence.
    /// The caller MUST verify that `.len() == COUNT` if using directly.
    fn to_public_inputs(&self) -> Vec<pallas::Base>;
}
