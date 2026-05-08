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

use pyo3::prelude::PyAnyMethods;

/// Pallas and Vesta curves
mod pasta;

/// Merkle tree utilities
mod merkle;

/// Cryptographic utilities
mod crypto;

/// zkas definitions
mod zkas;

/// Contract definitions
mod contract;

/// Transaction definitions
mod tx;

#[pyo3::prelude::pymodule]
fn dwow_sdk(
    py: pyo3::Python<'_>,
    m: &pyo3::Bound<'_, pyo3::prelude::PyModule>,
) -> pyo3::PyResult<()> {
    let submodule = pasta::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.pasta", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = merkle::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.merkle", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = crypto::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.crypto", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = zkas::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.zkas", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = tx::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.tx", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = contract::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("dwow_sdk.contract", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    Ok(())
}
