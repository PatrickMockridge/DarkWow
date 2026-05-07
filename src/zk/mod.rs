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

/// Halo2 zkas virtual machine
pub mod vm;
pub use vm::ZkCircuit;

/// VM heap variable definitions and utility functions
pub mod vm_heap;
pub use vm_heap::{empty_witnesses, Witness};

/// ZK gadget implementations
pub mod gadget;

/// Proof creation API
pub mod proof;
pub use proof::{Proof, ProvingKey, VerifyingKey};

/// Pure ZK proof verification (stateless, deterministic)
pub mod verifier;
pub use verifier::{verify_zkp, ZkVerifyResult};

/// Trace computation of intermediate values in circuit
mod tracer;
pub use tracer::DebugOpValue;

mod debug;
pub use debug::zkas_type_checks;

#[cfg(test)]
mod merkle_root_test;
#[cfg(feature = "tinyjson")]
pub use debug::{export_witness_json, import_witness_json};

pub mod halo2 {
    pub use halo2_proofs::{
        arithmetic::Field,
        circuit::{AssignedCell, Layouter, Value},
        dev, plonk,
        plonk::{Advice, Assigned, Column},
    };
}

//pub(in crate::zk) fn assign_free_advice<F: Field, V: Copy>(
pub fn assign_free_advice<F: halo2::Field, V: Copy>(
    mut layouter: impl halo2::Layouter<F>,
    column: halo2::Column<halo2::Advice>,
    value: halo2::Value<V>,
) -> Result<halo2::AssignedCell<V, F>, halo2::plonk::Error>
where
    for<'v> halo2::Assigned<F>: From<&'v V>,
{
    layouter.assign_region(
        || "load private",
        |mut region| region.assign_advice(|| "load private", column, 0, || value),
    )
}
